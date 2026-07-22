//! The internal RDF triple store that backs the world.
//!
//! Triples are stored in a merged model like lilv's, but each object is
//! tagged with the graph (bundle) it was loaded from so that a bundle can be
//! dropped again when it is replaced by a newer version of a plugin.

use crate::node::Node;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// FxHash (the rustc/Firefox hash): much faster than SipHash for the short
/// string keys that dominate this model. DoS resistance is irrelevant here:
/// all data comes from local bundle files the user chose to install.
#[derive(Default)]
struct FxHasher {
    hash: u64,
}

impl FxHasher {
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            self.add(u64::from_le_bytes(bytes[..8].try_into().unwrap()));
            bytes = &bytes[8..];
        }
        if bytes.len() >= 4 {
            self.add(u64::from(u32::from_le_bytes(
                bytes[..4].try_into().unwrap(),
            )));
            bytes = &bytes[4..];
        }
        for &byte in bytes {
            self.add(u64::from(byte));
        }
    }

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

type FxBuild = BuildHasherDefault<FxHasher>;
type FxHashMap<K, V> = HashMap<K, V, FxBuild>;

/// A single statement. The predicate is always a URI.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Triple {
    pub subject: Node,
    pub predicate: String,
    pub object: Node,
}

#[derive(Default)]
pub(crate) struct Model {
    /// predicate -> subject -> (object, graph id), merged over all graphs.
    /// Nested maps so lookups borrow both keys: querying never allocates.
    by_sp: FxHashMap<String, FxHashMap<Node, Vec<(Node, u32)>>>,
    /// Graph (bundle URI) -> numeric tag used in `by_sp`.
    graph_ids: FxHashMap<String, u32>,
    next_graph: u32,
}

impl Model {
    fn graph_id(&mut self, graph: &str) -> u32 {
        if let Some(&id) = self.graph_ids.get(graph) {
            return id;
        }
        let id = self.next_graph;
        self.next_graph += 1;
        self.graph_ids.insert(graph.to_string(), id);
        id
    }

    pub(crate) fn insert(&mut self, graph: &str, triple: &Triple) {
        let graph = self.graph_id(graph);
        let by_subject = self.by_sp.entry(triple.predicate.clone()).or_default();
        let objects = by_subject.entry(triple.subject.clone()).or_default();
        if !objects
            .iter()
            .any(|(object, g)| *g == graph && *object == triple.object)
        {
            objects.push((triple.object.clone(), graph));
        }
    }

    /// Removes every triple that was loaded from `graph`.
    pub(crate) fn remove_graph(&mut self, graph: &str) {
        let Some(graph) = self.graph_ids.remove(graph) else {
            return;
        };
        self.by_sp.retain(|_, by_subject| {
            by_subject.retain(|_, objects| {
                objects.retain(|(_, g)| *g != graph);
                !objects.is_empty()
            });
            !by_subject.is_empty()
        });
    }

    /// All objects `o` such that `(subject, predicate, o)` is in the model.
    ///
    /// The same triple asserted by several graphs is stored once per graph
    /// (so `remove_graph` stays correct); this view deduplicates them.
    pub(crate) fn objects(&self, subject: &Node, predicate: &str) -> Vec<Node> {
        let mut unique: Vec<Node> = Vec::new();
        for object in self.objects_iter(subject, predicate) {
            if !unique.contains(object) {
                unique.push(object.clone());
            }
        }
        unique
    }

    /// Borrowed variant of [`Model::objects`] for hot paths: no allocation
    /// or cloning, just two hash lookups.
    pub(crate) fn objects_iter<'a>(
        &'a self,
        subject: &'a Node,
        predicate: &'a str,
    ) -> impl Iterator<Item = &'a Node> {
        self.by_sp
            .get(predicate)
            .and_then(|by_subject| by_subject.get(subject))
            .into_iter()
            .flatten()
            .map(|(object, _)| object)
    }

    /// The first object for `(subject, predicate)`, if any.
    pub(crate) fn first(&self, subject: &Node, predicate: &str) -> Option<Node> {
        self.objects_iter(subject, predicate).next().cloned()
    }

    /// Returns true iff `(subject, predicate, object)` is in the model.
    pub(crate) fn ask(&self, subject: &Node, predicate: &str, object: &Node) -> bool {
        self.objects_iter(subject, predicate).any(|o| o == object)
    }

    /// Find all triples matching the given pattern. `None` is a wildcard.
    pub(crate) fn find(
        &self,
        subject: Option<&Node>,
        predicate: Option<&str>,
        object: Option<&Node>,
    ) -> Vec<Triple> {
        let mut triples = Vec::new();
        for (pred, by_subject) in &self.by_sp {
            if let Some(predicate) = predicate
                && pred != predicate
            {
                continue;
            }
            for (subj, objects) in by_subject {
                if let Some(subject) = subject
                    && subj != subject
                {
                    continue;
                }
                for (obj, _) in objects {
                    if let Some(object) = object
                        && obj != object
                    {
                        continue;
                    }
                    let triple = Triple {
                        subject: subj.clone(),
                        predicate: pred.clone(),
                        object: obj.clone(),
                    };
                    if !triples.contains(&triple) {
                        triples.push(triple);
                    }
                }
            }
        }
        triples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triple(s: &str, p: &str, o: &str) -> Triple {
        Triple {
            subject: Node::uri(s),
            predicate: p.to_string(),
            object: Node::uri(o),
        }
    }

    #[test]
    fn insert_query_remove() {
        let mut model = Model::default();
        model.insert("g1", &triple("s", "p", "o1"));
        model.insert("g1", &triple("s", "p", "o2"));
        model.insert("g2", &triple("s", "p", "o2"));
        // Duplicate in the same graph is ignored.
        model.insert("g1", &triple("s", "p", "o1"));

        assert_eq!(model.objects(&Node::uri("s"), "p").len(), 2);
        assert!(model.ask(&Node::uri("s"), "p", &Node::uri("o1")));
        assert!(!model.ask(&Node::uri("s"), "p", &Node::uri("o3")));

        model.remove_graph("g1");
        assert_eq!(model.objects(&Node::uri("s"), "p").len(), 1);
        assert!(!model.ask(&Node::uri("s"), "p", &Node::uri("o1")));
        assert!(model.ask(&Node::uri("s"), "p", &Node::uri("o2")));
    }
}
