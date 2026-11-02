use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::{GraphParser, ParseError, edge_weight, newlines};
use crate::modules::common::graph::DEFAULT_EDGE_WEIGHT;
use crate::modules::data::graph_builder::GraphBuilder;

/// Reads GraphML documents as a stream of events, so the XML tree is never
/// held in memory. An edge weighs what its `<data>` of the key declared with
/// `attr.name="weight"` says. The graph is undirected whatever `edgedefault`
/// says.
pub struct GraphMlParser;

impl GraphParser for GraphMlParser {
    fn parse(&self, bytes: &[u8]) -> Result<GraphBuilder, ParseError> {
        let mut reader = Reader::from_reader(bytes);
        reader.config_mut().trim_text(true);

        let mut document = GraphMlDocument::default();
        let mut buffer = Vec::new();
        let mut line = 1;
        let mut counted = 0;

        loop {
            let event = reader.read_event_into(&mut buffer).map_err(|error| {
                let at = (reader.error_position() as usize).min(bytes.len());
                ParseError::new(1 + newlines(&bytes[..at]), error.to_string())
            })?;

            // Counting as the reading goes keeps the whole count linear.
            let position = (reader.buffer_position() as usize).min(bytes.len());
            line += newlines(&bytes[counted..position]);
            counted = position;

            match event {
                Event::Eof => break,
                Event::Start(element) => document.open(&element, line)?,
                // A self-closing element has no `End` event.
                Event::Empty(element) => {
                    document.open(&element, line)?;
                    document.close(element.local_name().as_ref());
                }
                Event::Text(text) => {
                    let value = text
                        .decode()
                        .map_err(|error| ParseError::new(line, error.to_string()))?;
                    document.read_text(value.trim(), line)?;
                }
                Event::End(element) => document.close(element.local_name().as_ref()),
                _ => (),
            }

            buffer.clear();
        }

        Ok(document.builder)
    }
}

#[derive(Default)]
struct GraphMlDocument {
    builder: GraphBuilder,
    /// The `<key>` ids declared as the weight of an edge.
    weight_keys: HashSet<String>,
    /// The `<key>` ids of vertex attributes, mapped to their names.
    node_keys: HashMap<String, String>,
    node: Option<u32>,
    edge: Option<(u32, u32, f32)>,
    /// Whether the text being read is the weight of the open edge.
    reading_weight: bool,
    /// The vertex attribute the text being read belongs to.
    reading_attribute: Option<String>,
}

impl GraphMlDocument {
    fn open(&mut self, element: &BytesStart, line: usize) -> Result<(), ParseError> {
        match element.local_name().as_ref() {
            b"key" => self.declare_key(element, line)?,

            b"node" => {
                let id = attribute(element, b"id", line)?
                    .ok_or_else(|| ParseError::new(line, "a node has no id"))?;
                self.node = Some(self.builder.vertex(id.as_bytes()));
            }

            b"edge" => {
                let mut endpoint = |name: &str| {
                    let id = attribute(element, name.as_bytes(), line)?
                        .ok_or_else(|| ParseError::new(line, format!("an edge has no {name}")))?;
                    Ok::<_, ParseError>(self.builder.vertex(id.as_bytes()))
                };
                let source = endpoint("source")?;
                let target = endpoint("target")?;

                self.edge = Some((source, target, DEFAULT_EDGE_WEIGHT));
            }

            b"data" => {
                let key = attribute(element, b"key", line)?;
                let key = key.as_deref().unwrap_or_default();

                self.reading_weight = self.weight_keys.contains(key);
                self.reading_attribute = self.node.and(self.node_keys.get(key).cloned());
            }

            _ => (),
        }

        Ok(())
    }

    fn read_text(&mut self, value: &str, line: usize) -> Result<(), ParseError> {
        if let (Some(node), Some(name)) = (self.node, &self.reading_attribute) {
            self.builder.set_attribute(node, name.as_bytes(), value.as_bytes());
        }

        if let (true, Some((_, _, weight))) = (self.reading_weight, self.edge.as_mut()) {
            *weight = edge_weight(value.as_bytes(), line)?;
        }

        Ok(())
    }

    fn close(&mut self, name: &[u8]) {
        match name {
            b"data" => {
                self.reading_weight = false;
                self.reading_attribute = None;
            }
            b"node" => self.node = None,
            b"edge" => {
                if let Some((source, target, weight)) = self.edge.take() {
                    self.builder.add_edge(source, target, weight);
                }
            }
            _ => (),
        }
    }

    /// Records the keys of edge weights and of vertex attributes.
    fn declare_key(&mut self, element: &BytesStart, line: usize) -> Result<(), ParseError> {
        let applies_to = attribute(element, b"for", line)?;
        let applies = |kind| applies_to.as_deref().is_some_and(|value| value == kind || value == "all");
        let name = attribute(element, b"attr.name", line)?;

        let Some(id) = attribute(element, b"id", line)? else {
            return Ok(());
        };

        if applies("edge") && name.as_deref() == Some("weight") {
            self.weight_keys.insert(id.to_string());
        }
        if let (true, Some(name)) = (applies("node"), name) {
            self.node_keys.insert(id.into_owned(), name.into_owned());
        }

        Ok(())
    }
}

/// An attribute of an element, unescaped. It is copied only when
/// unescaping changed it.
fn attribute<'a>(
    element: &'a BytesStart,
    name: &[u8],
    line: usize,
) -> Result<Option<Cow<'a, str>>, ParseError> {
    let error = |cause: &dyn std::fmt::Display| ParseError::new(line, cause.to_string());

    for found in element.attributes() {
        let found = found.map_err(|cause| error(&cause))?;

        if found.key.local_name().as_ref() == name {
            return found.unescape_value().map(Some).map_err(|cause| error(&cause));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::common::graph::Graph;
    use std::sync::Arc;

    fn parse(document: &str) -> Arc<dyn Graph> {
        GraphMlParser.parse(document.as_bytes()).unwrap().build().0
    }

    /// A triangle plus an isolated vertex, with a weight declared on one edge.
    const GRAPHML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<graphml xmlns="http://graphml.graphdrawing.org/xmlns">
  <key id="w" for="edge" attr.name="weight" attr.type="double"/>
  <graph id="G" edgedefault="undirected">
    <node id="a"/>
    <node id="b"/>
    <node id="c"/>
    <node id="alone"/>
    <edge source="a" target="b">
      <data key="w">2.5</data>
    </edge>
    <edge source="b" target="c"/>
    <edge source="c" target="a"/>
  </graph>
</graphml>
"#;

    #[test]
    fn maps_identifiers_to_indices_in_order_and_keeps_isolated_vertices() {
        let graph = parse(GRAPHML);

        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 3);
        assert_eq!(graph.neighbors(0), &[1, 2]);
        assert_eq!(graph.neighbors(1), &[0, 2]);
        assert_eq!(graph.neighbors(2), &[0, 1]);
        assert_eq!(graph.degree(3), 0);
    }

    #[test]
    fn reads_the_declared_weight_and_defaults_the_rest() {
        let graph = parse(GRAPHML);

        assert_eq!(graph.neighbor_weights(0), Some(&[2.5, 1.0][..]));
        assert_eq!(graph.neighbor_weights(1), Some(&[2.5, 1.0][..]));
    }

    #[test]
    fn a_document_without_weights_builds_an_unweighted_graph() {
        let graph = parse(r#"<graphml><graph><edge source="a" target="b"/></graph></graphml>"#);

        assert_eq!(graph.neighbor_weights(0), None);
    }

    #[test]
    fn accepts_an_edge_before_its_nodes() {
        let graph = parse(
            r#"<graphml><graph>
                <edge source="x" target="y"/>
                <node id="x"/>
                <node id="y"/>
            </graph></graphml>"#,
        );

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.neighbors(0), &[1]);
    }

    #[test]
    fn ignores_data_of_other_keys() {
        let graph = parse(
            r#"<graphml>
                <key id="label" for="edge" attr.name="label" attr.type="string"/>
                <graph>
                    <edge source="a" target="b"><data key="label">not a weight</data></edge>
                </graph>
            </graphml>"#,
        );

        assert_eq!(graph.neighbor_weights(0), None);
    }

    #[test]
    fn reads_vertex_attributes() {
        let document = r#"<graphml>
            <key id="c" for="node" attr.name="community"/>
            <graph>
                <node id="a"><data key="c">x</data></node>
                <node id="b"><data key="c">y</data></node>
                <node id="z"/>
            </graph>
        </graphml>"#;

        let (_, attributes) = GraphMlParser.parse(document.as_bytes()).unwrap().build();

        assert_eq!(attributes.0[0].name, "community");
        assert_eq!(attributes.0[0].categories, vec![0, 1, u32::MAX]);
    }

    #[test]
    fn reports_errors_at_their_line() {
        let error = |text: &str| GraphMlParser.parse(text.as_bytes()).unwrap_err().line;

        assert_eq!(error("<graphml><graph>\n<edge source=\"a\"/></graph></graphml>"), 2);
        assert_eq!(error("<graphml>\n<graph>\n<node id=\"a\">\n</graphml>"), 4);
    }
}
