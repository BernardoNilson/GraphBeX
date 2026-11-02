//! A cursor over the bytes of a text format, shared by the GML and DOT
//! tokenizers. It counts the lines it passes, for the error messages.

use super::newlines;

pub struct Scanner<'a> {
    bytes: &'a [u8],
    position: usize,

    /// The line of the next byte.
    pub line: usize,
}

impl<'a> Scanner<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            line: 1,
        }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.position + offset).copied()
    }

    pub fn skip(&mut self, count: usize) {
        let end = (self.position + count).min(self.bytes.len());
        self.line += newlines(&self.bytes[self.position..end]);
        self.position = end;
    }

    pub fn take_while(&mut self, mut accept: impl FnMut(u8) -> bool) -> &'a [u8] {
        let start = self.position;
        let rest = &self.bytes[start..];
        let length = rest.iter().position(|&byte| !accept(byte)).unwrap_or(rest.len());

        self.skip(length);
        &self.bytes[start..start + length]
    }

    /// Consumes everything up to and including `end`, and returns what came
    /// before it. `None` if `end` never comes.
    pub fn take_until(&mut self, end: &[u8]) -> Option<&'a [u8]> {
        let start = self.position;
        let length = self.bytes[start..]
            .windows(end.len())
            .position(|window| window == end)?;

        self.skip(length + end.len());
        Some(&self.bytes[start..start + length])
    }

    pub fn skip_whitespace(&mut self) {
        self.take_while(|byte| byte.is_ascii_whitespace());
    }

    /// Skips the rest of the line, up to its line break.
    pub fn skip_line(&mut self) {
        self.take_while(|byte| byte != b'\n');
    }

    pub fn since(&self, start: usize) -> &'a [u8] {
        &self.bytes[start..self.position]
    }
}

