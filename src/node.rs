//! Nodes represent RDF terms: URIs, blank nodes, and literals.

use crate::uris;
use std::fmt::Debug;

/// The kind of value a [`Node`] holds.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum NodeKind {
    /// A URI reference (resource).
    Uri(String),
    /// A blank node identifier.
    Blank(String),
    /// A literal value with optional datatype URI and language tag.
    Literal {
        value: String,
        datatype: Option<String>,
        lang: Option<String>,
    },
}

/// An owned RDF node, analogous to `LilvNode`.
///
/// Nodes are plain data: they are `Clone + PartialEq + Eq + Hash` and both
/// `Send` and `Sync`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Node {
    pub(crate) kind: NodeKind,
}

impl Node {
    pub(crate) fn uri(uri: impl Into<String>) -> Self {
        Self {
            kind: NodeKind::Uri(uri.into()),
        }
    }

    pub(crate) fn blank(id: impl Into<String>) -> Self {
        Self {
            kind: NodeKind::Blank(id.into()),
        }
    }

    pub(crate) fn literal(
        value: impl Into<String>,
        datatype: Option<String>,
        lang: Option<String>,
    ) -> Self {
        Self {
            kind: NodeKind::Literal {
                value: value.into(),
                datatype,
                lang,
            },
        }
    }

    /// Creates a new plain string literal node.
    #[must_use]
    pub fn string(value: impl Into<String>) -> Self {
        Self::literal(value, None, None)
    }

    /// Creates a new string literal node with a language tag.
    #[must_use]
    pub fn string_lang(value: impl Into<String>, lang: impl Into<String>) -> Self {
        Self::literal(value, None, Some(lang.into()))
    }

    /// Creates a new file URI node from a filesystem path.
    #[must_use]
    pub fn file_uri(path: impl AsRef<std::path::Path>) -> Self {
        let path = path.as_ref().to_string_lossy();
        Self::uri(uris::path_to_file_uri(&path))
    }

    /// Creates a new integer literal node.
    #[must_use]
    pub fn int(value: i32) -> Self {
        Self::literal(value.to_string(), Some(uris::XSD_INTEGER.to_string()), None)
    }

    /// Creates a new floating-point literal node.
    #[must_use]
    pub fn float(value: f32) -> Self {
        Self::literal(value.to_string(), Some(uris::XSD_FLOAT.to_string()), None)
    }

    /// Creates a new boolean literal node.
    #[must_use]
    pub fn bool(value: bool) -> Self {
        Self::literal(
            if value { "true" } else { "false" },
            Some(uris::XSD_BOOLEAN.to_string()),
            None,
        )
    }

    /// Returns this value as a Turtle/SPARQL token.
    #[must_use]
    pub fn turtle_token(&self) -> String {
        match &self.kind {
            NodeKind::Uri(uri) => format!("<{uri}>"),
            NodeKind::Blank(id) => format!("_:{id}"),
            NodeKind::Literal {
                value,
                datatype,
                lang,
            } => {
                let mut token = String::with_capacity(value.len() + 2);
                token.push('"');
                for c in value.chars() {
                    match c {
                        '"' => token.push_str("\\\""),
                        '\\' => token.push_str("\\\\"),
                        '\n' => token.push_str("\\n"),
                        '\r' => token.push_str("\\r"),
                        '\t' => token.push_str("\\t"),
                        _ => token.push(c),
                    }
                }
                token.push('"');
                if let Some(lang) = lang {
                    token.push('@');
                    token.push_str(lang);
                } else if let Some(datatype) = datatype {
                    token.push_str("^^<");
                    token.push_str(datatype);
                    token.push('>');
                }
                token
            }
        }
    }

    /// Returns whether the value is a URI (resource).
    #[must_use]
    pub fn is_uri(&self) -> bool {
        matches!(self.kind, NodeKind::Uri(_))
    }

    /// Returns this value as a URI string or `None` if it is not a uri.
    #[must_use]
    pub fn as_uri(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Uri(uri) => Some(uri),
            _ => None,
        }
    }

    /// Returns whether the value is a blank node (resource with no URI).
    #[must_use]
    pub fn is_blank(&self) -> bool {
        matches!(self.kind, NodeKind::Blank(_))
    }

    /// Returns this value as a blank node identifier.
    #[must_use]
    pub fn as_blank(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Blank(id) => Some(id),
            _ => None,
        }
    }

    /// Returns `true` if the value is a literal (i.e. not a URI or blank node).
    #[must_use]
    pub fn is_literal(&self) -> bool {
        matches!(self.kind, NodeKind::Literal { .. })
    }

    /// Returns this literal's language tag, if present.
    #[must_use]
    pub fn language(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Literal { lang, .. } => lang.as_deref(),
            _ => None,
        }
    }

    /// Returns whether this value is a string literal.
    #[must_use]
    pub fn is_string(&self) -> bool {
        match &self.kind {
            NodeKind::Literal { datatype, .. } => {
                datatype.as_deref().is_none_or(|d| d == uris::XSD_STRING)
            }
            _ => false,
        }
    }

    /// Return the value as a string or `None` if it is not a string.
    ///
    /// Like lilv's `lilv_node_as_string`, this returns the lexical value of
    /// literals, the URI of URI nodes, and the identifier of blank nodes.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Uri(uri) => Some(uri),
            NodeKind::Blank(id) => Some(id),
            NodeKind::Literal { value, .. } => Some(value),
        }
    }

    /// Returns `true` if the value is a decimal literal.
    #[must_use]
    pub fn is_float(&self) -> bool {
        match &self.kind {
            NodeKind::Literal { datatype, .. } => datatype.as_deref().is_some_and(|d| {
                d == uris::XSD_DECIMAL || d == uris::XSD_DOUBLE || d == uris::XSD_FLOAT
            }),
            _ => false,
        }
    }

    /// Returns the value as an `f32` if it is a float or int. `None` is returned
    /// otherwise.
    #[must_use]
    pub fn as_float(&self) -> Option<f32> {
        match &self.kind {
            NodeKind::Literal { value, .. } if self.is_float() || self.is_int() => {
                value.parse().ok()
            }
            _ => None,
        }
    }

    /// Returns `true` if the value is an integer literal.
    #[must_use]
    pub fn is_int(&self) -> bool {
        match &self.kind {
            NodeKind::Literal { datatype, .. } => datatype.as_deref() == Some(uris::XSD_INTEGER),
            _ => false,
        }
    }

    /// Returns the value as an `i32` if it is an int (not float) or `None`
    /// otherwise.
    #[must_use]
    pub fn as_int(&self) -> Option<i32> {
        match &self.kind {
            NodeKind::Literal { value, .. } if self.is_int() => value.parse().ok(),
            _ => None,
        }
    }

    /// Returns `true` if the value is a boolean literal.
    #[must_use]
    pub fn is_bool(&self) -> bool {
        match &self.kind {
            NodeKind::Literal { datatype, .. } => datatype.as_deref() == Some(uris::XSD_BOOLEAN),
            _ => false,
        }
    }

    /// Returns the value as a `bool` if it is a boolean or `None` otherwise.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match &self.kind {
            NodeKind::Literal { value, .. } if self.is_bool() => {
                Some(value == "true" || value == "1")
            }
            _ => None,
        }
    }

    /// Returns the filesystem path for a file URI node.
    #[must_use]
    pub fn path(&self) -> Option<std::path::PathBuf> {
        self.as_uri()
            .and_then(uris::file_uri_to_path)
            .map(std::path::PathBuf::from)
    }

    pub(crate) fn as_literal_parts(&self) -> Option<(&str, Option<&str>, Option<&str>)> {
        match &self.kind {
            NodeKind::Literal {
                value,
                datatype,
                lang,
            } => Some((value, datatype.as_deref(), lang.as_deref())),
            _ => None,
        }
    }
}

impl Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Node").field(&self.turtle_token()).finish()
    }
}

/// A collection of `Node`.
#[derive(Clone, Default)]
pub struct Nodes {
    pub(crate) nodes: Vec<Node>,
}

impl Nodes {
    pub(crate) fn new(nodes: Vec<Node>) -> Self {
        Self { nodes }
    }

    /// The number of nodes in the collection.
    #[must_use]
    pub fn count(&self) -> usize {
        self.nodes.len()
    }

    /// The number of nodes in the collection.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns true if this collection has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the first node in this collection.
    #[must_use]
    pub fn first(&self) -> Option<Node> {
        self.nodes.first().cloned()
    }

    /// Returns `true` if a node with the given value exists in the collection.
    #[must_use]
    pub fn contains(&self, value: &Node) -> bool {
        self.nodes.contains(value)
    }

    /// Return a new collection of `Nodes` that contains all the nodes from
    /// `self` and `other`.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        let mut nodes = self.nodes.clone();
        for node in &other.nodes {
            if !nodes.contains(node) {
                nodes.push(node.clone());
            }
        }
        Self { nodes }
    }

    /// An iterator over all the nodes.
    pub fn iter(&self) -> impl '_ + Iterator<Item = Node> {
        self.nodes.iter().cloned()
    }
}

impl Debug for Nodes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.nodes.iter()).finish()
    }
}

impl IntoIterator for Nodes {
    type Item = Node;
    type IntoIter = std::vec::IntoIter<Node>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        let int = Node::literal("5", Some(uris::XSD_INTEGER.to_string()), None);
        assert!(int.is_int());
        assert_eq!(int.as_int(), Some(5));
        assert_eq!(int.as_float(), Some(5.0));

        let float = Node::literal("-90.0", Some(uris::XSD_DECIMAL.to_string()), None);
        assert!(float.is_float());
        assert_eq!(float.as_float(), Some(-90.0));
        assert!(float.as_int().is_none());

        let boolean = Node::literal("true", Some(uris::XSD_BOOLEAN.to_string()), None);
        assert_eq!(boolean.as_bool(), Some(true));

        let string = Node::string("hello");
        assert!(string.is_string());
        assert_eq!(string.as_str(), Some("hello"));
        assert!(string.as_float().is_none());

        let uri = Node::uri("http://example.com/x");
        assert!(uri.is_uri());
        assert_eq!(uri.as_uri(), Some("http://example.com/x"));
        assert!(!uri.is_literal());
    }

    #[test]
    fn tokens() {
        assert_eq!(
            Node::uri("http://example.com/x").turtle_token(),
            "<http://example.com/x>"
        );
        assert_eq!(Node::blank("b1").turtle_token(), "_:b1");
        assert_eq!(Node::string("a\"b").turtle_token(), "\"a\\\"b\"");
        assert_eq!(
            Node::literal("5", Some(uris::XSD_INTEGER.to_string()), None).turtle_token(),
            "\"5\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
        assert_eq!(
            Node::literal("bonjour", None, Some("fr".to_string())).turtle_token(),
            "\"bonjour\"@fr"
        );
    }
}
