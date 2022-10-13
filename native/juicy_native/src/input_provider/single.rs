use rustler::types::binary::Binary;
use rustler::{Encoder, Env, Term};

use super::InputProvider;

use iterative_json_parser::Range as PRange;

/// Provides data from a single binary.
pub struct SingleBinaryProvider<'a> {
    binary: Binary<'a>,
}

impl<'a> SingleBinaryProvider<'a> {
    pub fn new(binary: Binary<'a>) -> Self {
        SingleBinaryProvider { binary: binary }
    }
}

impl<'a> InputProvider<Option<u8>> for SingleBinaryProvider<'a> {
    fn byte(&self, pos: usize) -> Option<u8> {
        self.binary.as_slice().get(pos).cloned()
    }

    fn push_range(&self, range: PRange, buf: &mut Vec<u8>) {
        let bin = self.binary.as_slice();
        buf.extend_from_slice(&bin[range.start..range.end]);
    }

    fn range_to_term<'b>(&self, env: Env<'b>, range: PRange) -> Term<'b> {
        self.binary
            .make_subbinary(range.start, range.end - range.start)
            .ok()
            .unwrap()
            .encode(env)
    }
}
