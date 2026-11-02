use super::{GraphParser, ParseError, edge_weight, newlines};
use crate::modules::common::graph::DEFAULT_EDGE_WEIGHT;
use crate::modules::data::graph_builder::GraphBuilder;

/// Reads edge lists: one edge per line, `u v [weight]`, separated by spaces,
/// tabs or commas. Blank lines and lines starting with `#` are skipped. The
/// vertices are already indices, so the highest one sets the vertex count.
pub struct EdgeListParser;

impl GraphParser for EdgeListParser {
    fn parse(&self, bytes: &[u8]) -> Result<GraphBuilder, ParseError> {
        let text = std::str::from_utf8(bytes).map_err(|error| {
            let line = 1 + newlines(&bytes[..error.valid_up_to()]);
            ParseError::new(line, error.to_string())
        })?;

        let mut builder = GraphBuilder::default();

        for (index, line) in text.lines().enumerate() {
            let number = index + 1;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let mut fields = line.split([' ', ',', '\t']).filter(|field| !field.is_empty());
            let mut vertex = |name: &str| {
                let field = fields
                    .next()
                    .ok_or_else(|| ParseError::new(number, format!("missing {name}")))?;
                field
                    .parse::<u32>()
                    .map_err(|_| ParseError::new(number, format!("invalid {name} '{field}'")))
            };

            let source = vertex("source vertex")?;
            let target = vertex("target vertex")?;
            let weight = match fields.next() {
                Some(raw) => edge_weight(raw.as_bytes(), number)?,
                None => DEFAULT_EDGE_WEIGHT,
            };

            builder.add_edge(source, target, weight);
        }

        Ok(builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::common::graph::Graph;
    use std::sync::Arc;

    fn parse(text: &str) -> Arc<dyn Graph> {
        EdgeListParser.parse(text.as_bytes()).unwrap().build().0
    }

    fn error_line(text: &str) -> usize {
        EdgeListParser.parse(text.as_bytes()).unwrap_err().line
    }

    #[test]
    fn reads_one_edge_per_line() {
        let graph = parse("0 1\n1 2\n0 2\n");

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 3);
        assert_eq!(graph.neighbors(1), &[0, 2]);
    }

    #[test]
    fn accepts_commas_and_tabs_as_separators() {
        assert_eq!(parse("0,1\n1\t2\n").neighbors(1), &[0, 2]);
    }

    #[test]
    fn reads_the_optional_weight() {
        assert_eq!(parse("0 1 2.5\n1 2\n").neighbor_weights(1), Some(&[2.5, 1.0][..]));
    }

    #[test]
    fn a_list_without_weights_builds_an_unweighted_graph() {
        assert_eq!(parse("0 1\n1 2\n").neighbor_weights(0), None);
    }

    #[test]
    fn ignores_blank_lines_and_comments() {
        let graph = parse("# comment\n\n0 1\n");

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.neighbors(0), &[1]);
    }

    #[test]
    fn reports_a_malformed_line_and_an_invalid_weight_with_their_number() {
        assert_eq!(error_line("0 1\n0 not_a_vertex\n"), 2);
        assert_eq!(error_line("0 1 NaN\n"), 1);
        assert_eq!(error_line("0 1\n\n0 \u{FFFD}"), 3);
        assert_eq!(EdgeListParser.parse(b"0 1\n\xFF").unwrap_err().line, 2);
    }
}
