use std::path::Path;

use super::parsers::GraphParser;
use super::parsers::dot::DotParser;
use super::parsers::edge_list::EdgeListParser;
use super::parsers::gml::GmlParser;
use super::parsers::graphml::GraphMlParser;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    EdgeList,
    GraphMl,
    Gml,
    Dot,
}

impl GraphFormat {
    pub const ALL: [GraphFormat; 4] = [Self::EdgeList, Self::GraphMl, Self::Gml, Self::Dot];

    /// The format of a file, from its extension.
    pub fn detect(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();

        Self::ALL
            .into_iter()
            .find(|format| format.extensions().contains(&extension.as_str()))
    }

    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::EdgeList => &["el", "csv", "txt"],
            Self::GraphMl => &["graphml"],
            Self::Gml => &["gml"],
            Self::Dot => &["dot", "gv"],
        }
    }

    /// The extensions of every format, for the filter of the file dialog.
    pub fn supported_extensions() -> Vec<&'static str> {
        Self::ALL.iter().flat_map(|format| format.extensions().iter().copied()).collect()
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::EdgeList => "Edge List",
            Self::GraphMl => "GraphML",
            Self::Gml => "GML",
            Self::Dot => "DOT",
        }
    }

    /// No fallback arm: a new format does not compile until it has a parser.
    pub fn parser(self) -> &'static dyn GraphParser {
        match self {
            Self::EdgeList => &EdgeListParser,
            Self::GraphMl => &GraphMlParser,
            Self::Gml => &GmlParser,
            Self::Dot => &DotParser,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_format_by_extension() {
        let cases = [
            ("graph.el", Some(GraphFormat::EdgeList)),
            ("graph.csv", Some(GraphFormat::EdgeList)),
            ("graph.graphml", Some(GraphFormat::GraphMl)),
            ("graph.gml", Some(GraphFormat::Gml)),
            ("graph.txt", Some(GraphFormat::EdgeList)),
            ("graph.dot", Some(GraphFormat::Dot)),
            ("graph.gv", Some(GraphFormat::Dot)),
            ("GRAPH.GML", Some(GraphFormat::Gml)),
            ("graph.json", None),
            ("graph", None),
        ];

        for (path, format) in cases {
            assert_eq!(GraphFormat::detect(Path::new(path)), format, "{path}");
        }
    }
}
