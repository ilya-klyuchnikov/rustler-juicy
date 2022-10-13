use std::collections::HashMap;
use ::rustler::{ Term, NifResult, Error };
use ::rustler::types::list::ListIterator;
use ::rustler::types::map::MapIterator;
use ::rustler::types::atom::Atom;

use super::{
    NodeOptions,
    NodeId,
    Node,
    NodeVariant,
    Spec,
};

mod atoms {
    atoms! {
        stream,
        any,
        map,
        map_keys,
        array,
        struct_atom,
        atom_keys,
        ignore_non_atoms,
    }
}

fn read_opts<'a>(term: Term<'a>, stream_collect: bool) -> NifResult<NodeOptions> {
    let iterator: ListIterator = term.decode()?;
    let mut opts = NodeOptions::default();
    for decoded in iterator.map(|term| term.decode::<(Term, Term)>()) {
        let (key, value) = decoded?;

        if atoms::stream() == key {
            opts.stream = value.decode()?;
        } else if atoms::struct_atom() == key {
            opts.struct_atom = Some(value.decode()?);
        } else if atoms::atom_keys() == key {
            let mut map: HashMap<Vec<u8>, Atom> = HashMap::new();
            let iterator: ListIterator = value.decode()?;
            for atom_term in iterator {
                let atom_str: String = atom_term.atom_to_string()?;
                let atom: Atom = atom_term.decode()?;
                map.insert(atom_str.into_bytes(), atom);
            }
            opts.atom_mappings = Some(map);
        } else if atoms::ignore_non_atoms() == key {
            opts.ignore_non_atoms = value.decode()?;
        }

    }
    opts.stream_collect = opts.stream | stream_collect;
    Ok(opts)
}

fn read_node<'a>(node: Term<'a>, nodes: &mut Vec<Node>, parent: NodeId, stream_collect: bool) -> NifResult<NodeId> {
    let current = NodeId(nodes.len());

    // Arity 3
    match node.decode::<(Term, Term, Term)>() {
        Ok((typ, opts, data)) => {
            let opts = read_opts(opts, stream_collect)?;
            let child_stream_collect = opts.stream_collect;

            return if atoms::map() == typ {
                nodes.push(Node {
                    variant: NodeVariant::Sentinel,
                    options: opts,
                    parent: Some(parent),
                });

                let child = read_node(data, nodes, current, child_stream_collect)?;
                nodes[current.0].variant = NodeVariant::Map {
                    child: child,
                };

                Ok(current)
            } else if atoms::map_keys() == typ {
                nodes.push(Node {
                    variant: NodeVariant::Sentinel,
                    options: opts,
                    parent: Some(parent),
                });

                let mut children = HashMap::<String, NodeId>::new();
                for (key, value) in data.decode::<MapIterator>()? {
                    let child = read_node(value, nodes, current, child_stream_collect)?;
                    children.insert(key.decode()?, child);
                }
                nodes[current.0].variant = NodeVariant::MapKeys {
                    children: children,
                };

                Ok(current)
            } else if atoms::array() == typ {
                nodes.push(Node {
                    variant: NodeVariant::Sentinel,
                    options: opts,
                    parent: Some(parent),
                });

                let child = read_node(data, nodes, current, child_stream_collect)?;
                nodes[current.0].variant = NodeVariant::Array {
                    child: child,
                };

                Ok(current)
            } else {
                Err(Error::BadArg)
            };

        },
        Err(_) => (),
    }

    // Arity 2
    match node.decode::<(Term, Term)>() {
        Ok((typ, opts)) => {
            let opts = read_opts(opts, stream_collect)?;

            return if atoms::any() == typ {
                nodes.push(Node {
                    variant: NodeVariant::Any,
                    options: opts,
                    parent: Some(parent),
                });
                Ok(current)
            } else {
                Err(Error::BadArg)
            };

        },
        Err(_) => (),
    }

    Err(Error::BadArg)
}

pub fn spec_from_term<'a>(root: Term<'a>) -> NifResult<Spec> {
    let mut nodes = Vec::<Node>::new();

    let sentinel = Node {
        variant: NodeVariant::Sentinel,
        options: NodeOptions::default(),
        parent: None,
    };
    nodes.push(sentinel);
    let sentinel_id = NodeId(0);

    assert_eq!(read_node(root, &mut nodes, sentinel_id, false)?, NodeId(1));

    Ok(Spec {
        nodes: nodes,
        root: sentinel_id,
    })
}
