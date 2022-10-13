#[macro_use]
extern crate rustler;
extern crate lazy_static;
extern crate rustler_codegen;

extern crate num_bigint;
extern crate num_traits;

use rustler::{Encoder, Env, NifResult, Term};

extern crate iterative_json_parser;

mod input_provider;
mod numbers;
mod path_tracker;
mod strings;
mod tree_spec;

mod basic;
mod basic_spec;
mod streaming;

mod atoms {
    rustler::atoms! {
        ok,
        nil,
        error,
        unexpected,
        iter,
        streamed,
        yield_ = "yield",
        await_input,
        finished,
        __struct__,
    }
}

#[rustler::nif]
fn parse_init<'a>(env: Env<'a>, input_term: Term<'a>) -> NifResult<Term<'a>> {
    basic::parse(env, input_term)
}

#[rustler::nif]
fn parse_iter<'a>(
    env: Env<'a>,
    input_term: Term<'a>,
    stack_term: Term<'a>,
    resource_term: Term<'a>,
) -> NifResult<Term<'a>> {
    basic::parse_iter(env, input_term, stack_term, resource_term)
}

#[rustler::nif]
fn spec_parse_init<'a>(
    env: Env<'a>,
    binary_term: Term<'a>,
    spec_term: Term<'a>,
) -> NifResult<Term<'a>> {
    basic_spec::parse_init(env, binary_term, spec_term)
}

#[rustler::nif]
fn spec_parse_iter<'a>(env: Env<'a>, term: Term<'a>) -> NifResult<Term<'a>> {
    basic_spec::parse_iter(env, term)
}

#[rustler::nif]
fn stream_parse_init<'a>(env: Env<'a>, term: Term<'a>) -> NifResult<Term<'a>> {
    streaming::parse_init(env, term)
}

#[rustler::nif]
fn stream_parse_iter<'a>(
    env: Env<'a>,
    binaries: Term<'a>,
    parser: Term<'a>,
) -> NifResult<Term<'a>> {
    streaming::parse_iter(env, binaries, parser)
}

#[rustler::nif]
fn validate_spec<'a>(env: Env<'a>, term: Term<'a>) -> NifResult<Term<'a>> {
    match tree_spec::spec_from_term(term) {
        Ok(_) => Ok(atoms::ok().encode(env)),
        Err(_) => Ok(atoms::error().encode(env)),
    }
}

fn load<'a>(env: Env<'a>, _load_info: Term<'a>) -> bool {
    resource!(basic::IterStateWrapper, env);
    resource!(basic_spec::BasicSpecIterStateWrapper, env);
    resource!(streaming::StreamingIterStateWrapper, env);
    true
}

rustler::init!(
    "Elixir.Juicy.Native",
    [
        parse_init,
        parse_iter,
        spec_parse_init,
        spec_parse_iter,
        stream_parse_init,
        stream_parse_iter,
        validate_spec
    ],
    load = load
);
