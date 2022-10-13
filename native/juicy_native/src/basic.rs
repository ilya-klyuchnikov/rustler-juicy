use iterative_json_parser::{
    Bailable, NumberData, ParseError, Parser, PeekResult, Pos, Position, Range, Sink, Source,
    StringPosition, Unexpected,
};

use rustler::resource::ResourceArc;
use rustler::types::binary::Binary;
use rustler::types::binary::OwnedBinary;
use rustler::types::map::map_new;
use rustler::{Encoder, Env, NifResult, Term};

use input_provider::single::SingleBinaryProvider;
use input_provider::InputProvider;
use numbers::number_data_to_term;
use strings::BuildString;

use std::io::Write;
use std::ops::DerefMut;
use std::sync::Mutex;

struct BasicSS<'a, 'b> {
    env: Env<'a>,

    input: SingleBinaryProvider<'a>,

    position: usize,
    next_reschedule: usize,

    out_stack: Vec<Term<'a>>,
    current_string: &'b mut BuildString,
}

impl<'a, 'b> Bailable for BasicSS<'a, 'b> {
    type Bail = ();
}
impl<'a, 'b> Source for BasicSS<'a, 'b> {
    fn position(&self) -> Pos {
        self.position.into()
    }
    fn skip(&mut self, num: usize) {
        self.position += num
    }
    fn peek_char(&mut self) -> PeekResult<()> {
        if self.position == self.next_reschedule {
            PeekResult::Bail(())
        } else if let Some(character) = self.input.byte(self.position) {
            PeekResult::Ok(character)
        } else {
            PeekResult::Eof
        }
    }
    fn peek_slice<'c>(&'c self, _length: usize) -> Option<&'c [u8]> {
        // let (_, slice) = self.source.split_at(self.position);
        // if slice.len() >= length {
        //    Some(slice)
        // else {
        //    None
        //
        None
    }
}

impl<'a, 'b> Sink for BasicSS<'a, 'b> {
    fn push_map(&mut self, _pos: Position) {
        self.out_stack.push(map_new(self.env));
    }
    fn push_array(&mut self, _pos: Position) {
        let arr: Vec<Term> = Vec::new();
        self.out_stack.push(arr.encode(self.env));
    }
    fn push_number(&mut self, _pos: Position, num: NumberData) -> Result<(), Self::Bail> {
        let term = number_data_to_term(self.env, num, |r, b| {
            self.input.push_range(r, b);
        });
        self.out_stack.push(term);
        Ok(())
    }
    fn push_bool(&mut self, _pos: Position, val: bool) -> Result<(), Self::Bail> {
        self.out_stack.push(val.encode(self.env));
        Ok(())
    }
    fn push_null(&mut self, _pos: Position) -> Result<(), Self::Bail> {
        self.out_stack.push(::atoms::nil().encode(self.env));
        Ok(())
    }

    fn start_string(&mut self, _pos: StringPosition) {
        *self.current_string = BuildString::new();
    }
    fn append_string_range(&mut self, range: Range) {
        let input = &self.input;
        self.current_string.append_range(range, |r, b| {
            input.push_range(r, b);
        });
    }
    fn append_string_single(&mut self, character: u8) {
        let input = &self.input;
        self.current_string.append_single(character, |r, b| {
            input.push_range(r, b);
        });
    }
    fn append_string_codepoint(&mut self, codepoint: char) {
        let input = &self.input;
        self.current_string.append_codepoint(codepoint, |r, b| {
            input.push_range(r, b);
        });
    }
    fn finalize_string(&mut self, _pos: StringPosition) -> Result<(), Self::Bail> {
        let string_term = match *self.current_string {
            BuildString::None => "".encode(self.env),
            BuildString::Range(range) => self.input.range_to_term(self.env, range),
            BuildString::Owned(ref buf) => {
                let mut bin = OwnedBinary::new(buf.len()).unwrap();
                bin.as_mut_slice().write(buf).unwrap();
                bin.release(self.env).encode(self.env)
            }
        };
        *self.current_string = BuildString::None;
        self.out_stack.push(string_term);
        Ok(())
    }

    fn finalize_map(&mut self, _pos: Position) -> Result<(), Self::Bail> {
        Ok(())
    }
    fn finalize_array(&mut self, _pos: Position) -> Result<(), Self::Bail> {
        let term = self.out_stack.pop().unwrap();
        self.out_stack.push(term.list_reverse().ok().unwrap());
        Ok(())
    }
    fn pop_into_map(&mut self) {
        let value = self.out_stack.pop().unwrap();
        let key = self.out_stack.pop().unwrap();
        let map = self.out_stack.pop().unwrap();
        self.out_stack.push(map.map_put(key, value).ok().unwrap());
    }
    fn pop_into_array(&mut self) {
        let value = self.out_stack.pop().unwrap();
        let array = self.out_stack.pop().unwrap();
        self.out_stack.push(array.list_prepend(value));
    }
}

fn format_unexpected<'a>(env: Env<'a>, parser: &Parser, pos: Pos, reason: Unexpected) -> Term<'a> {
    let parser_state = format!("{:?}", parser).encode(env);
    let position = pos.0 as u64;
    let explaination = reason.explain().encode(env);
    (
        ::atoms::error(),
        (::atoms::unexpected(), position, explaination, parser_state),
    )
        .encode(env)
}

pub struct IterState {
    parser: Parser,
    source_pos: usize,
    sink_string_state: BuildString,
}
pub struct IterStateWrapper(Mutex<IterState>);

fn parse_inner<'a>(
    env: Env<'a>,
    input: Binary<'a>,
    stack: Vec<Term<'a>>,
    iter_state: &mut IterState,
) -> Result<Term<'a>, Vec<Term<'a>>> {
    let mut ss = BasicSS {
        env: env,
        input: SingleBinaryProvider::new(input),
        position: iter_state.source_pos,
        next_reschedule: iter_state.source_pos + 40_000,
        out_stack: stack,
        current_string: &mut iter_state.sink_string_state,
    };

    let result = iter_state.parser.run(&mut ss);
    iter_state.source_pos = ss.position;

    match result {
        Ok(()) => {
            let term = ss.out_stack.pop().unwrap();
            Ok((::atoms::ok(), term).encode(env))
        }
        Err(ParseError::SourceBail(())) => Err(ss.out_stack),
        Err(ParseError::Unexpected(pos, reason)) => {
            Ok(format_unexpected(env, &iter_state.parser, pos, reason))
        }
        err => panic!("{:?}", err),
    }
}

pub fn parse<'a>(env: Env<'a>, input_term: Term<'a>) -> NifResult<Term<'a>> {
    let input: Binary = input_term.decode()?;

    let mut iter_state = IterState {
        parser: Parser::new(),
        source_pos: 0,
        sink_string_state: BuildString::None,
    };

    match parse_inner(env, input, vec![], &mut iter_state) {
        Ok(res) => Ok(res),
        Err(stack) => {
            let resource = ResourceArc::new(IterStateWrapper(Mutex::new(iter_state)));
            Ok((::atoms::iter(), stack, resource).encode(env))
        }
    }
}

pub fn parse_iter<'a>(
    env: Env<'a>,
    input_term: Term<'a>,
    stack_term: Term<'a>,
    resource_term: Term<'a>,
) -> NifResult<Term<'a>> {
    let input: Binary = input_term.decode()?;
    let stack: Vec<Term<'a>> = stack_term.decode()?;
    let resource: ResourceArc<IterStateWrapper> = resource_term.decode()?;
    let mut resource_inner_guard = resource.0.lock().unwrap();
    let resource_inner = resource_inner_guard.deref_mut();

    match parse_inner(env, input, stack, resource_inner) {
        Ok(res) => Ok(res),
        Err(stack) => Ok((::atoms::iter(), stack, resource_term).encode(env)),
    }
}
