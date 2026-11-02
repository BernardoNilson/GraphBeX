use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use memmap2::Mmap;

use super::error::IngestionError;
use super::format::GraphFormat;
use super::vertex_attributes::VertexAttributes;
use crate::modules::common::graph::Graph;

/// Reads a graph file in the two stages of the TCC: the file is
/// memory-mapped, and the parser of its format reads the mapped bytes.
pub fn load(path: &Path) -> Result<(Arc<dyn Graph>, VertexAttributes), IngestionError> {
    let format = GraphFormat::detect(path)
        .ok_or_else(|| IngestionError::UnknownFormat(path.display().to_string()))?;

    let file = File::open(path)?;
    // SAFETY: the mapping is only read while parsing. Another process could
    // still change the file meanwhile, which is the accepted cost of mmap.
    let bytes = unsafe { Mmap::map(&file)? };

    let builder = format.parser().parse(&bytes).map_err(|error| IngestionError::Malformed {
        format: format.label(),
        line: error.line,
        reason: error.reason,
    })?;

    let representation = if builder.is_weighted() { "weighted" } else { "unweighted" };
    log::info!("{} file read into a {representation} CSR graph.", format.label());

    Ok(builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a temporary file, loads it and removes it. The result is mapped
    /// to the vertex and edge counts, since `dyn Graph` is not `Debug`.
    fn load_counts(name: &str, content: &str) -> Result<(usize, usize), IngestionError> {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, content).expect("failed to write the temp file");

        let result = load(&path).map(|(graph, _)| (graph.node_count(), graph.edge_count()));
        std::fs::remove_file(path).ok();
        result
    }

    #[test]
    fn loads_each_format_by_its_extension() {
        let files = [
            ("ingestion_test.el", "0 1\n1 2\n"),
            (
                "ingestion_test.graphml",
                r#"<graphml><graph><edge source="a" target="b"/><edge source="b" target="c"/></graph></graphml>"#,
            ),
            ("ingestion_test.gml", "graph [ edge [ source 0 target 1 ] edge [ source 1 target 2 ] ]"),
            ("ingestion_test.dot", "graph { a -- b -- c }"),
        ];

        for (name, content) in files {
            assert_eq!(load_counts(name, content).unwrap(), (3, 2), "{name}");
        }
    }

    #[test]
    fn reports_unknown_extensions_missing_files_and_malformed_content() {
        assert!(matches!(load_counts("ingestion_test.unknown", "0 1\n"), Err(IngestionError::UnknownFormat(_))));
        assert!(matches!(load(Path::new("missing_ingestion_test.el")), Err(IngestionError::Io(_))));
        assert!(matches!(
            load_counts("ingestion_malformed_test.gml", "graph [\n node [ ] ]"),
            Err(IngestionError::Malformed { format: "GML", line: 2, .. })
        ));
    }
}
