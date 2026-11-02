use super::scanner::Scanner;
use super::{GraphParser, ParseError, edge_weight, lossy, parse_weight};
use crate::modules::common::graph::DEFAULT_EDGE_WEIGHT;
use crate::modules::data::graph_builder::GraphBuilder;

/// Reads GML files: a tree of `key value` pairs, where a value is a number, a
/// quoted string or a list between brackets.
///
/// ```text
/// graph [
///   node [ id 0 label "a" ]
///   edge [ source 0 target 1 weight 2.5 ]
/// ]
/// ```
///
/// Only the `node` and `edge` lists directly inside `graph` are read. Any other
/// list, such as `graphics`, is skipped. The weight is `weight`, or else a
/// numeric `value`, the key of Newman's datasets.
pub struct GmlParser;

impl GraphParser for GmlParser {
    fn parse(&self, bytes: &[u8]) -> Result<GraphBuilder, ParseError> {
        let mut scanner = Scanner::new(bytes);
        let mut document = GmlDocument::default();

        while let Some((token, line)) = next_token(&mut scanner)? {
            match token {
                Token::Close => document.close(line)?,
                Token::Word(key) if is_key(key) => match next_token(&mut scanner)? {
                    Some((Token::Open, _)) => document.open(key, line),
                    Some((Token::Word(value) | Token::Text(value), _)) => {
                        document.attribute(key, value, line)?
                    }
                    _ => {
                        let reason = format!("the key '{}' has no value", lossy(key));
                        return Err(ParseError::new(line, reason));
                    }
                },
                _ => return Err(ParseError::new(line, "expected a key")),
            }
        }

        document.finish(scanner.line)
    }
}

/// The lists the parser tells apart. The content of any other is skipped.
#[derive(Clone, Copy, PartialEq, Eq)]
enum List {
    Graph,
    Node,
    Edge,
    Other,
}

/// The node or edge being read. Values borrow the mapped bytes.
#[derive(Default)]
struct Element<'a> {
    /// Where the element opened, which is where its errors are reported.
    line: usize,
    id: Option<&'a [u8]>,
    source: Option<&'a [u8]>,
    target: Option<&'a [u8]>,
    weight: Option<f32>,
    value: Option<f32>,
    attributes: Vec<(&'a [u8], &'a [u8])>,
}

#[derive(Default)]
struct GmlDocument<'a> {
    /// The lists currently open, innermost last.
    lists: Vec<List>,
    element: Element<'a>,
    /// Files without a `graph` list are rejected rather than read as empty.
    found_graph: bool,
    builder: GraphBuilder,
}

impl<'a> GmlDocument<'a> {
    fn open(&mut self, key: &[u8], line: usize) {
        let list = match (self.lists.last(), key) {
            (None, b"graph") => List::Graph,
            (Some(List::Graph), b"node") => List::Node,
            (Some(List::Graph), b"edge") => List::Edge,
            _ => List::Other,
        };

        match list {
            List::Graph => self.found_graph = true,
            List::Node | List::Edge => self.element = Element { line, ..Element::default() },
            List::Other => (),
        }

        self.lists.push(list);
    }

    fn attribute(&mut self, key: &'a [u8], value: &'a [u8], line: usize) -> Result<(), ParseError> {
        let element = &mut self.element;

        match (self.lists.last(), key) {
            (Some(List::Node), b"id") => element.id = Some(value),
            (Some(List::Node), _) => element.attributes.push((key, value)),
            (Some(List::Edge), b"source") => element.source = Some(value),
            (Some(List::Edge), b"target") => element.target = Some(value),
            (Some(List::Edge), b"weight") => element.weight = Some(edge_weight(value, line)?),
            (Some(List::Edge), b"value") => element.value = parse_weight(value),
            _ => (),
        }

        Ok(())
    }

    /// Records the node or edge whose list just closed.
    fn close(&mut self, line: usize) -> Result<(), ParseError> {
        let list = self
            .lists
            .pop()
            .ok_or_else(|| ParseError::new(line, "']' closes no list"))?;

        if !matches!(list, List::Node | List::Edge) {
            return Ok(());
        }

        let element = std::mem::take(&mut self.element);

        if list == List::Node {
            let id = element
                .id
                .ok_or_else(|| ParseError::new(element.line, "a node has no id"))?;
            let vertex = self.builder.vertex(id);

            for (name, value) in element.attributes {
                self.builder.set_attribute(vertex, name, value);
            }
            return Ok(());
        }

        let (Some(source), Some(target)) = (element.source, element.target) else {
            return Err(ParseError::new(element.line, "an edge needs a source and a target"));
        };
        let source = self.builder.vertex(source);
        let target = self.builder.vertex(target);
        let weight = element.weight.or(element.value).unwrap_or(DEFAULT_EDGE_WEIGHT);

        self.builder.add_edge(source, target, weight);
        Ok(())
    }

    fn finish(self, line: usize) -> Result<GraphBuilder, ParseError> {
        if !self.lists.is_empty() {
            return Err(ParseError::new(line, "a list is never closed"));
        }
        if !self.found_graph {
            return Err(ParseError::new(line, "the file declares no graph"));
        }

        Ok(self.builder)
    }
}

enum Token<'a> {
    Open,
    Close,
    /// A key or a number, told apart by where it appears.
    Word(&'a [u8]),
    Text(&'a [u8]),
}

/// The next token and the line it starts on, or `None` at the end. Lines
/// starting with `#` are comments.
fn next_token<'a>(scanner: &mut Scanner<'a>) -> Result<Option<(Token<'a>, usize)>, ParseError> {
    loop {
        scanner.skip_whitespace();
        if scanner.peek(0) != Some(b'#') {
            break;
        }
        scanner.skip_line();
    }

    let line = scanner.line;
    let token = match scanner.peek(0) {
        None => return Ok(None),
        Some(b'[') => {
            scanner.skip(1);
            Token::Open
        }
        Some(b']') => {
            scanner.skip(1);
            Token::Close
        }
        // GML has no escapes: the first quote ends the string.
        Some(b'"') => {
            scanner.skip(1);
            let text = scanner
                .take_until(b"\"")
                .ok_or_else(|| ParseError::new(line, "a string is never closed"))?;
            Token::Text(text)
        }
        Some(_) => Token::Word(scanner.take_while(|byte| {
            !byte.is_ascii_whitespace() && !matches!(byte, b'[' | b']' | b'"')
        })),
    };

    Ok(Some((token, line)))
}

/// A key starts with a letter. Anything else is a value.
fn is_key(word: &[u8]) -> bool {
    word.first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::common::graph::Graph;
    use std::sync::Arc;

    fn parse(text: &str) -> Arc<dyn Graph> {
        GmlParser.parse(text.as_bytes()).unwrap().build().0
    }

    fn error_line(text: &str) -> usize {
        GmlParser.parse(text.as_bytes()).unwrap_err().line
    }

    /// A triangle plus an isolated vertex, as in Newman's datasets, with a
    /// weight on one edge.
    const GML: &str = r#"Creator "test"
graph
[
  directed 0
  node
  [
    id 10
    label "a"
  ]
  node [ id 20 label "b" ]
  node [ id 30 label "c" ]
  node [ id 40 label "alone" ]
  edge [ source 10 target 20 weight 2.5 ]
  edge [ source 20 target 30 ]
  edge [ source 30 target 10 ]
]
"#;

    #[test]
    fn maps_identifiers_to_indices_in_order() {
        let graph = parse(GML);

        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 3);
        assert_eq!(graph.neighbors(0), &[1, 2]);
        assert_eq!(graph.neighbors(1), &[0, 2]);
        assert!(graph.neighbors(3).is_empty());
    }

    #[test]
    fn reads_the_weight_and_defaults_the_rest() {
        assert_eq!(parse(GML).neighbor_weights(0), Some(&[2.5, 1.0][..]));
    }

    #[test]
    fn a_file_without_weights_builds_an_unweighted_graph() {
        assert_eq!(parse("graph [ edge [ source 0 target 1 ] ]").neighbor_weights(0), None);
    }

    #[test]
    fn reads_a_numeric_value_as_the_weight_unless_there_is_a_weight() {
        let weights = |text| parse(text).neighbor_weights(0).map(<[f32]>::to_vec);

        assert_eq!(weights("graph [ edge [ source 0 target 1 value 0.25 ] ]"), Some(vec![0.25]));
        assert_eq!(weights("graph [ edge [ source 0 target 1 value 3 weight 2 ] ]"), Some(vec![2.0]));
        assert_eq!(weights(r#"graph [ edge [ source 0 target 1 value "fast" ] ]"#), None);
    }

    #[test]
    fn skips_nested_lists_and_unknown_keys() {
        // The `id` inside `graphics` must not rename the vertex, and the
        // brackets inside the string must not open a list.
        let graph = parse(
            r#"graph [
                label "a [weird] name"
                node [ id 0 graphics [ id 99 x 1.5 ] Latitude -8.05 ]
                node [ id 1 ]
                edge [ source 0 target 1 LinkLabel "10Gbps" ]
            ]"#,
        );

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.neighbors(0), &[1]);
    }

    #[test]
    fn ignores_comment_lines() {
        let graph = parse("# a comment\ngraph [\n# another\nedge [ source 0 target 1 ] ]");

        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn accepts_quoted_identifiers_and_edges_before_nodes() {
        let graph = parse(r#"graph [ edge [ source "x" target "y" ] node [ id "x" ] ]"#);

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.neighbors(0), &[1]);
    }

    #[test]
    fn reads_vertex_attributes_but_not_nested_lists() {
        let text = r#"graph [ node [ id 0 gt 7 graphics [ x 1 ] ] node [ id 1 gt 3 ] ]"#;
        let (_, attributes) = GmlParser.parse(text.as_bytes()).unwrap().build();

        assert_eq!(attributes.0.len(), 1);
        assert_eq!(attributes.0[0].name, "gt");
        assert_eq!(attributes.0[0].categories, vec![0, 1]);
    }

    #[test]
    fn reports_errors_at_their_line() {
        // An element's errors are reported where it opened.
        assert_eq!(error_line("graph [\n  edge [\n    source 0\n  ]\n]"), 2);
        assert_eq!(error_line("graph [\n  node [ label \"a\" ]\n]"), 2);
        assert_eq!(error_line("graph [\n edge [ source 0 target 1 weight heavy ] ]"), 2);
        // The lines inside strings count.
        assert_eq!(error_line("graph [\n label \"two\nlines\"\n node [ ] ]"), 4);
    }

    #[test]
    fn rejects_malformed_files() {
        for text in [
            "graph [ node [ id 0 ]",
            "graph [ ] ]",
            "graph [ directed ]",
            "graph [ 42 ]",
            "graph [ label \"open ]",
            "Creator \"nobody\"",
        ] {
            assert!(GmlParser.parse(text.as_bytes()).is_err(), "{text}");
        }
    }
}
