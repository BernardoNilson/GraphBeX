//! One parser per file format. Each reads the mapped bytes of a file into a
//! `GraphBuilder`, where every format meets before the CSR is built.

pub mod dot;
pub mod edge_list;
pub mod gml;
pub mod graphml;
mod scanner;

use std::borrow::Cow;

use super::graph_builder::GraphBuilder;

pub trait GraphParser {
    fn parse(&self, bytes: &[u8]) -> Result<GraphBuilder, ParseError>;
}

/// A problem in the content of a file, at the line where it was found.
#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub reason: String,
}

impl ParseError {
    pub fn new(line: usize, reason: impl Into<String>) -> Self {
        Self {
            line,
            reason: reason.into(),
        }
    }
}

/// A weight written as text. Only finite numbers are accepted: NaN or
/// infinity would poison every layout computation.
fn parse_weight(raw: &[u8]) -> Option<f32> {
    std::str::from_utf8(raw)
        .ok()?
        .parse::<f32>()
        .ok()
        .filter(|weight| weight.is_finite())
}

/// The weight of an edge, or the error to report at its line.
fn edge_weight(raw: &[u8], line: usize) -> Result<f32, ParseError> {
    parse_weight(raw)
        .ok_or_else(|| ParseError::new(line, format!("invalid edge weight '{}'", lossy(raw))))
}

fn lossy(bytes: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

fn newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&byte| byte == b'\n').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_weight_accepts_finite_numbers_only() {
        assert_eq!(parse_weight(b"2.5"), Some(2.5));
        assert_eq!(parse_weight(b"-3"), Some(-3.0));
        assert_eq!(parse_weight(b"1e2"), Some(100.0));
        assert_eq!(parse_weight(b"NaN"), None);
        assert_eq!(parse_weight(b"inf"), None);
        assert_eq!(parse_weight(b"heavy"), None);
    }
}
