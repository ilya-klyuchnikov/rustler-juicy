#[macro_use]
extern crate rustler;
#[macro_use]
extern crate rustler_codegen;
#[macro_use]
extern crate lazy_static;

extern crate num_traits;
extern crate num_bigint;

use rustler::{Env, Term, NifResult, Encoder};

extern crate iterative_json_parser;

mod numbers;
mod strings;
mod tree_spec;
mod input_provider;
mod path_tracker;

mod basic;
mod basic_spec;
mod streaming;

mod atoms {
    rustler_atoms! {
        atom ok;
        atom nil;
        atom error;
        atom unexpected;
        atom iter;
        atom streamed;
        atom yield_ = "yield";
        atom await_input;
        atom finished;
        atom __struct__;
    }
}

rustler_export_nifs! {
    "Elixir.Juicy.Native",
    [
        ("parse_init", 1, basic::parse),
        ("parse_iter", 3, basic::parse_iter),

        ("spec_parse_init", 2, basic_spec::parse_init),
        ("spec_parse_iter", 1, basic_spec::parse_iter),

        ("stream_parse_init", 1, streaming::parse_init),
        ("stream_parse_iter", 2, streaming::parse_iter),

        ("validate_spec", 1, validate_spec),
    ],
    Some(on_init)
}

fn validate_spec<'a>(env: Env<'a>, args: &[Term<'a>]) -> NifResult<Term<'a>> {
    match tree_spec::spec_from_term(args[0]) {
        Ok(_) => Ok(atoms::ok().encode(env)),
        Err(_) => Ok(atoms::error().encode(env)),
    }
}

fn on_init<'a>(env: Env<'a>, _load_info: Term<'a>) -> bool {
    resource_struct_init!(basic::IterStateWrapper, env);
    resource_struct_init!(basic_spec::BasicSpecIterStateWrapper, env);
    resource_struct_init!(streaming::StreamingIterStateWrapper, env);
    true
}
