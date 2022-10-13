use std::io::Write;

use super::BailType;

use ::strings::BuildString;
use ::numbers::number_data_to_term;

use ::tree_spec::ValueType;
use ::tree_spec::NodeId;

use rustler::{Env, Term, Encoder};
use rustler::types::map::map_new;
use rustler::types::binary::OwnedBinary;

use ::iterative_json_parser::{Bailable, Source, Sink, Pos, PeekResult, Position, NumberData,
                              StringPosition};
use iterative_json_parser::Range as PRange;

use ::input_provider::InputProvider;
use ::input_provider::streaming::{StreamingInputProvider, StreamingInputResult};

use ::path_tracker::PathTracker;

pub struct StreamingSS<'a, 'b>
    where 'a: 'b
{
    pub env: Env<'a>,
    pub input: StreamingInputProvider<'a, 'b>,
    pub next_reschedule: usize,
    pub out_stack: Vec<Term<'a>>,
    pub state: &'b mut SSState,
    pub yields: Vec<Term<'a>>,
}

pub struct SSState {
    pub path_tracker: PathTracker,

    pub position: usize,
    pub first_needed: usize,
    pub current_string: BuildString,
}

impl<'a, 'b> Bailable for StreamingSS<'a, 'b> {
    type Bail = BailType;
}

impl<'a, 'b> Source for StreamingSS<'a, 'b> {
    fn position(&self) -> Pos {
        self.state.position.into()
    }
    fn skip(&mut self, num: usize) {
        self.state.position += num
    }
    fn peek_char(&mut self) -> PeekResult<BailType> {
        if self.state.position == self.next_reschedule {
            PeekResult::Bail(BailType::Reschedule)
        } else {
            match self.input.byte(self.state.position) {
                StreamingInputResult::Ok(byte) => PeekResult::Ok(byte),
                StreamingInputResult::AwaitInput => PeekResult::Bail(BailType::AwaitInput),
                StreamingInputResult::Eof => unimplemented!(),
            }
        }
    }
    fn peek_slice<'c>(&'c self, _length: usize) -> Option<&'c [u8]> {
        None
    }
}

impl<'a, 'b> StreamingSS<'a, 'b> {

    fn do_stream(&mut self, node_id_opt: Option<NodeId>) -> Result<(), BailType> {
        match node_id_opt {
            Some(node_id) => {
                let node = self.state.path_tracker.walker.spec.get(node_id);
                if node.options.stream {
                    let path = self.state.path_tracker.path.encode(self.env);
                    let term = self.out_stack.pop().unwrap();
                    self.out_stack.push(::atoms::streamed().encode(self.env));
                    self.yields.push((::atoms::yield_(), (path, term)).encode(self.env))
                }
            }
            None => (),
        }
        Ok(())
    }

}

impl<'a, 'b> Sink for StreamingSS<'a, 'b> {
    fn push_map(&mut self, pos: Position) {
        self.out_stack.push(map_new(self.env));

        self.state.path_tracker.enter_map(pos);
        self.state.first_needed = self.state.position;
    }
    fn push_array(&mut self, pos: Position) {
        let arr: Vec<Term> = Vec::new();
        self.out_stack.push(arr.encode(self.env));

        self.state.path_tracker.enter_array(pos);
        self.state.first_needed = self.state.position;
    }
    fn push_number(&mut self, pos: Position, num: NumberData) -> Result<(), Self::Bail> {
        let term = number_data_to_term(self.env, num, |r, b| self.input.push_range(r, b));
        self.out_stack.push(term);

        let curr_node = self.state.path_tracker.visit_terminal(pos, ValueType::Number);
        self.do_stream(curr_node.current)?;

        self.state.first_needed = self.state.position;
        Ok(())
    }
    fn push_bool(&mut self, pos: Position, val: bool) -> Result<(), Self::Bail> {
        self.out_stack.push(val.encode(self.env));

        let curr_node = self.state.path_tracker.visit_terminal(pos, ValueType::Boolean);
        self.do_stream(curr_node.current)?;

        self.state.first_needed = self.state.position;
        Ok(())
    }
    fn push_null(&mut self, pos: Position) -> Result<(), Self::Bail> {
        self.out_stack.push(::atoms::nil().encode(self.env));

        let curr_node = self.state.path_tracker.visit_terminal(pos, ValueType::Null);
        self.do_stream(curr_node.current)?;

        self.state.first_needed = self.state.position;
        Ok(())
    }

    fn start_string(&mut self, pos: StringPosition) {
        self.state.current_string = match pos {
            StringPosition::MapKey => BuildString::new_owned(),
            _ => BuildString::new(),
        };
    }
    fn append_string_range(&mut self, range: PRange) {
        let input = &self.input;
        self.state.current_string.append_range(range, |r, b| input.push_range(r, b));
    }
    fn append_string_single(&mut self, character: u8) {
        let input = &self.input;
        self.state.current_string.append_single(character, |r, b| input.push_range(r, b));
    }
    fn append_string_codepoint(&mut self, codepoint: char) {
        let input = &self.input;
        self.state.current_string.append_codepoint(codepoint, |r, b| input.push_range(r, b));
    }
    fn finalize_string(&mut self, pos: StringPosition) -> Result<(), Self::Bail> {
        let string = ::std::mem::replace(&mut self.state.current_string, BuildString::None);
        match pos {
            StringPosition::MapKey => {
                let key = string.owned_to_vec();

                let mut bin = OwnedBinary::new(key.len()).unwrap();
                bin.as_mut_slice().write(&key).unwrap();
                self.out_stack.push(bin.release(self.env).encode(self.env));

                self.state.path_tracker.enter_key(key);
            }
            _ => {
                let string_term = string.to_term(&mut self.input, self.env);
                self.out_stack.push(string_term);

                let curr_node = self.state.path_tracker.visit_terminal(pos.to_position(), ValueType::String);
                self.do_stream(curr_node.current)?;
            }
        }
        self.state.first_needed = self.state.position;
        Ok(())
    }

    fn finalize_map(&mut self, _pos: Position) -> Result<(), Self::Bail> {
        self.state.first_needed = self.state.position;

        let curr_node = self.state.path_tracker.exit_map();
        self.do_stream(curr_node.current)?;

        Ok(())
    }
    fn finalize_array(&mut self, _pos: Position) -> Result<(), Self::Bail> {
        let term = self.out_stack.pop().unwrap();
        self.out_stack.push(term.list_reverse().ok().unwrap());

        self.state.first_needed = self.state.position;

        let curr_node = self.state.path_tracker.exit_array();
        self.do_stream(curr_node.current)?;

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
