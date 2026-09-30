//! Node identity keys.
//!
//! Every node has a stable, unique `key`.  Notes are identified by their
//! relative file path (`note:<path>`), while all other nodes (entities,
//! tags, directories) are identified by their label (`node:<label>`).
//!
//! These helpers centralise the key format so that the schema migration,
//! the graph builder and the demo seed all agree on it.

/// Identity key for a note node, derived from its relative file path.
pub fn note_key(path: &str) -> String {
    format!("note:{}", path)
}

/// Identity key for a non-note node (entity, tag, directory), derived
/// from its label.
pub fn node_key(label: &str) -> String {
    format!("node:{}", label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_key_prefixes_path() {
        assert_eq!(note_key("a/foo.md"), "note:a/foo.md");
    }

    #[test]
    fn node_key_prefixes_label() {
        assert_eq!(node_key("Python"), "node:Python");
    }
}
