use super::vertex_ids::VertexIds;

/// Marks a vertex without a value for an attribute.
pub const NO_CATEGORY: u32 = u32::MAX;

/// A vertex attribute of the file, each distinct value turned into a category
/// number, which is all the coloring needs.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    /// The category of each vertex, or `NO_CATEGORY`.
    pub categories: Vec<u32>,
}

/// The vertex attributes of a file, in order of first appearance.
#[derive(Debug, Default, Clone)]
pub struct VertexAttributes(pub Vec<Attribute>);

#[derive(Debug, Default)]
pub struct AttributeCollector {
    attributes: Vec<(Attribute, VertexIds)>,
}

impl AttributeCollector {
    pub fn set(&mut self, vertex: u32, name: &[u8], value: &[u8]) {
        let name = String::from_utf8_lossy(name);
        let index = match self.attributes.iter().position(|(attribute, _)| attribute.name == name) {
            Some(index) => index,
            None => {
                let attribute = Attribute { name: name.into_owned(), categories: Vec::new() };
                self.attributes.push((attribute, VertexIds::default()));
                self.attributes.len() - 1
            }
        };

        let (attribute, values) = &mut self.attributes[index];
        let vertex = vertex as usize;

        if attribute.categories.len() <= vertex {
            attribute.categories.resize(vertex + 1, NO_CATEGORY);
        }
        attribute.categories[vertex] = values.index_of(value);
    }

    /// The attributes, with one entry per vertex.
    pub fn finish(self, vertices: usize) -> VertexAttributes {
        let attributes = self.attributes.into_iter().map(|(mut attribute, _)| {
            attribute.categories.resize(vertices, NO_CATEGORY);
            attribute
        });

        VertexAttributes(attributes.collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_become_categories_per_attribute() {
        let mut collector = AttributeCollector::default();
        collector.set(0, b"group", b"red");
        collector.set(2, b"group", b"blue");
        collector.set(1, b"group", b"red");

        let attributes = collector.finish(4);
        let group = &attributes.0[0];

        assert_eq!(group.name, "group");
        assert_eq!(group.categories, vec![0, 0, 1, NO_CATEGORY]);
    }
}
