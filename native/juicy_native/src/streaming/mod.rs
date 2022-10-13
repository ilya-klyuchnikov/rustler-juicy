use iterative_json_parser::{ParseError, Parser, Pos, Unexpected};

use rustler::resource::ResourceArc;
use rustler::types::binary::Binary;
use rustler::types::list::ListIterator;
use rustler::{Encoder, Env, NifResult, Term};

use strings::BuildString;

use tree_spec::spec_from_term;
use tree_spec::SpecWalker;

use input_provider::streaming::StreamingInputProvider;

use path_tracker::PathTracker;

use std::ops::DerefMut;
use std::ops::Range;
use std::sync::Mutex;

mod source_sink;
use self::source_sink::{SSState, StreamingSS};

#[derive(Copy, Clone)]
pub enum BailType {
    Reschedule,
    AwaitInput,
}

fn format_unexpected<'a>(env: Env<'a>, pos: Pos, reason: Unexpected) -> Term<'a> {
    let position = pos.0 as u64;
    let explaination = reason.explain().encode(env);
    (
        ::atoms::error(),
        (::atoms::unexpected(), position, explaination),
    )
        .encode(env)
}

pub struct StreamingIterState {
    parser: Parser,
    ss_state: SSState,
}
pub struct StreamingIterStateWrapper(Mutex<StreamingIterState>);

fn read_binaries<'a>(term: Term<'a>) -> NifResult<Vec<(Range<usize>, Binary<'a>)>> {
    let binaries_iter: ListIterator = term.decode()?;
    let mut binaries_ranges: Vec<(Range<usize>, Binary)> = Vec::new();
    for term in binaries_iter {
        let (start, bin): (usize, Binary) = term.decode()?;
        let range = start..(start + bin.len());
        binaries_ranges.push((range, bin));
    }
    Ok(binaries_ranges)
}

fn write_binaries<'a>(
    env: Env<'a>,
    binaries: &Vec<(Range<usize>, Binary<'a>)>,
    last_needed: usize,
) -> Term<'a> {
    let res: Vec<Term> = binaries
        .iter()
        .filter(|&&(ref range, _)| range.end >= last_needed)
        .map(|&(ref range, bin)| (range.start, bin).encode(env))
        .collect();
    res.encode(env)
}

pub fn parse_init<'a>(env: Env<'a>, term: Term<'a>) -> NifResult<Term<'a>> {
    let spec = spec_from_term(term)?;

    let ss_state = SSState {
        path_tracker: PathTracker {
            path: Vec::new(),
            walker: SpecWalker::new(spec),
        },

        position: 0,
        first_needed: 0,
        current_string: BuildString::None,
    };

    let iter_state = StreamingIterState {
        parser: Parser::new(),
        ss_state: ss_state,
    };

    let resource = ResourceArc::new(StreamingIterStateWrapper(Mutex::new(iter_state)));
    let stack: [u8; 0] = [];
    let state = (&stack as &[u8], resource).encode(env);
    Ok((::atoms::ok(), state).encode(env))
}

pub fn parse_iter<'a>(env: Env<'a>, binaries: Term<'a>, parser: Term<'a>) -> NifResult<Term<'a>> {
    let binaries_ranges: Vec<(Range<usize>, Binary)> = read_binaries(binaries)?;
    let (stack, resource): (Vec<Term<'a>>, ResourceArc<StreamingIterStateWrapper>) =
        parser.decode()?;

    let (res, out_stack, mut yields, first_needed) = {
        let mut resource_inner_guard = resource.0.lock().unwrap();
        let mut iter_state = resource_inner_guard.deref_mut();

        let mut ss = StreamingSS {
            env: env,
            input: StreamingInputProvider {
                binaries: &binaries_ranges,
            },
            next_reschedule: iter_state.ss_state.position + 40_000,
            out_stack: stack,
            state: &mut iter_state.ss_state,
            yields: Vec::new(),
        };

        let res = iter_state.parser.run(&mut ss);
        (res, ss.out_stack, ss.yields, ss.state.first_needed)
    };

    let binaries_out = write_binaries(env, &binaries_ranges, first_needed);

    match res {
        Ok(()) => {
            yields.push(::atoms::finished().encode(env));
            let state = (out_stack, resource).encode(env);
            Ok((::atoms::finished(), yields, binaries_out, state).encode(env))
        }
        Err(ParseError::SourceBail(BailType::Reschedule)) => {
            let state = (out_stack, resource).encode(env);
            Ok((::atoms::iter(), yields, binaries_out, state).encode(env))
        }
        Err(ParseError::SourceBail(BailType::AwaitInput)) => {
            let state = (out_stack, resource).encode(env);
            Ok((::atoms::await_input(), yields, binaries_out, state).encode(env))
        }
        Err(ParseError::Unexpected(pos, reason)) => {
            let error = format_unexpected(env, pos, reason);
            yields.push(error);
            let state = (out_stack, resource).encode(env);
            Ok((::atoms::finished(), yields, binaries_out, state).encode(env))
        }
        Err(_) => panic!("TODO: Add proper error"),
    }
}
