use std::collections::HashMap;

/// Numbers byte strings in order of first appearance: the vertex names of a
/// file, or the values of an attribute. Bytes are compared as they are, so any
/// encoding works.
#[derive(Debug, Default)]
pub struct VertexIds {
    indices: HashMap<Box<[u8]>, u32>,
}

impl VertexIds {
    /// The index of `id`, assigning the next free one on first sight.
    pub fn index_of(&mut self, id: &[u8]) -> u32 {
        if let Some(&index) = self.indices.get(id) {
            return index;
        }

        let next = self.indices.len() as u32;
        self.indices.insert(id.into(), next);
        next
    }

    pub fn len(&self) -> usize {
        self.indices.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_receive_indices_in_order_of_appearance() {
        let mut ids = VertexIds::default();

        assert_eq!(ids.index_of(b"b"), 0);
        assert_eq!(ids.index_of(b"a"), 1);
        assert_eq!(ids.index_of(b"b"), 0);
        assert_eq!(ids.len(), 2);
    }
}
