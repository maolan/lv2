//! Ports describe the IO of plugins.

use crate::node::{Node, Nodes};
use crate::plugin::Plugin;
use crate::uris;
use std::fmt::Debug;

/// Internal data of a single port, built when the port table is loaded.
#[derive(Clone)]
pub(crate) struct PortData {
    pub node: Node,
    pub index: usize,
    pub symbol: String,
}

/// A port on a plugin.
#[derive(Clone)]
pub struct Port {
    pub(crate) plugin: Plugin,
    pub(crate) node: Node,
    pub(crate) index: usize,
    pub(crate) symbol: String,
}

impl Port {
    /// Get the node for the port.
    #[must_use]
    pub fn node(&self) -> Node {
        self.node.clone()
    }

    /// Get the value associated with the port in a plugin's data files.
    ///
    /// Returns the objects of all triples of the form
    /// `(port, predicate, object)`.
    #[must_use]
    pub fn value(&self, predicate: &Node) -> Nodes {
        let Some(predicate) = predicate.as_uri() else {
            return Nodes::default();
        };
        let inner = self.plugin.world.lock();
        Nodes::new(inner.objects(&self.node, predicate))
    }

    /// Get a single property value of a port.
    ///
    /// This is equivalent to getting the first iterable value of
    /// `self.value(predicate)`.
    #[must_use]
    pub fn get(&self, predicate: &Node) -> Option<Node> {
        let predicate = predicate.as_uri()?;
        let inner = self.plugin.world.lock();
        inner.first(&self.node, predicate)
    }

    /// Return the LV2 port properties of a port.
    #[must_use]
    pub fn properties(&self) -> Nodes {
        self.value(&Node::uri(uris::LV2_PORT_PROPERTY))
    }

    /// Returns true if the port has the given property.
    #[must_use]
    pub fn has_property(&self, property_uri: &Node) -> bool {
        let inner = self.plugin.world.lock();
        inner
            .model
            .ask(&self.node, uris::LV2_PORT_PROPERTY, property_uri)
    }

    /// Human-readable comment/description for this port.
    #[must_use]
    pub fn comment(&self) -> Option<Node> {
        self.get(&Node::uri(uris::RDFS_COMMENT))
    }

    /// Port group resource, if declared.
    #[must_use]
    pub fn group(&self) -> Option<Node> {
        self.get(&Node::uri(uris::PORT_GROUPS_GROUP))
    }

    #[must_use]
    pub fn is_input(&self) -> bool {
        self.is_a(&Node::uri(uris::LV2_INPUT_PORT))
    }

    #[must_use]
    pub fn is_output(&self) -> bool {
        self.is_a(&Node::uri(uris::LV2_OUTPUT_PORT))
    }

    #[must_use]
    pub fn is_audio(&self) -> bool {
        self.is_a(&Node::uri(uris::LV2_AUDIO_PORT))
    }

    #[must_use]
    pub fn is_cv(&self) -> bool {
        self.is_a(&Node::uri(uris::LV2_CV_PORT))
    }

    #[must_use]
    pub fn is_control(&self) -> bool {
        self.is_a(&Node::uri(uris::LV2_CONTROL_PORT))
    }

    #[must_use]
    pub fn is_connection_optional(&self) -> bool {
        self.has_property(&Node::uri(uris::LV2_CONNECTION_OPTIONAL))
    }

    #[must_use]
    pub fn is_sample_rate(&self) -> bool {
        self.has_property(&Node::uri(uris::LV2_SAMPLE_RATE))
    }

    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.has_property(&Node::uri(uris::LV2_INTEGER))
    }

    #[must_use]
    pub fn is_enumeration(&self) -> bool {
        self.has_property(&Node::uri(uris::LV2_ENUMERATION))
    }

    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.has_property(&Node::uri(uris::PORT_PROPS_NOT_ON_GUI))
    }

    #[must_use]
    pub fn is_logarithmic(&self) -> bool {
        self.has_property(&Node::uri(uris::PORT_PROPS_LOGARITHMIC))
    }

    /// Numeric range values as floats, with lv2:sampleRate scaling applied when requested.
    #[must_use]
    pub fn range_float(&self, sample_rate: f32) -> (Option<f32>, Option<f32>, Option<f32>) {
        let range = self.range();
        let scale = if self.is_sample_rate() {
            sample_rate
        } else {
            1.0
        };
        (
            range
                .minimum
                .as_ref()
                .and_then(Node::as_float)
                .map(|v| v * scale),
            range
                .maximum
                .as_ref()
                .and_then(Node::as_float)
                .map(|v| v * scale),
            range.default.as_ref().and_then(Node::as_float),
        )
    }

    /// Returns true if the port has the given designation.
    #[must_use]
    pub fn has_designation(&self, designation: &Node) -> bool {
        let inner = self.plugin.world.lock();
        inner
            .model
            .ask(&self.node, uris::LV2_DESIGNATION, designation)
    }

    /// Returns `true` if the port supports a certain event type.
    ///
    /// More precisely, this returns `true` if and only if the port has an
    /// `atom:supports` or an `ev:supportsEvent` property with `event_type` as
    /// the value.
    #[must_use]
    pub fn supports_event(&self, event_type: &Node) -> bool {
        let inner = self.plugin.world.lock();
        inner.model.ask(&self.node, uris::ATOM_SUPPORTS, event_type)
            || inner
                .model
                .ask(&self.node, uris::EVENT_SUPPORTS_EVENT, event_type)
    }

    /// Returns the index of the port within the plugin.
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// Get the symbol of a port.
    ///
    /// Symbol is a short string.
    #[must_use]
    pub fn symbol(&self) -> Option<Node> {
        Some(Node::string(&self.symbol))
    }

    /// Get the name of a port.
    ///
    /// This returns the untranslated name (the `lv2:name` in the data file
    /// without a language tag).
    #[must_use]
    pub fn name(&self) -> Option<Node> {
        let inner = self.plugin.world.lock();
        let names: Vec<Node> = inner
            .objects(&self.node, uris::LV2_NAME)
            .into_iter()
            .filter(|n| n.is_string())
            .collect();
        names
            .iter()
            .find(|n| {
                n.as_literal_parts()
                    .is_some_and(|(_, _, lang)| lang.is_none())
            })
            .cloned()
            .or_else(|| names.first().cloned())
    }

    /// Get all the classes of the port.
    #[must_use]
    pub fn classes(&self) -> Nodes {
        self.value(&Node::uri(uris::RDF_TYPE))
    }

    /// Returns `true` if the port is of the given type.
    #[must_use]
    pub fn is_a(&self, port_class: &Node) -> bool {
        let inner = self.plugin.world.lock();
        inner.model.ask(&self.node, uris::RDF_TYPE, port_class)
    }

    /// The range (default, minimum, maximum) values of the port.
    #[must_use]
    pub fn range(&self) -> Range {
        let inner = self.plugin.world.lock();
        Range {
            default: inner.model.first(&self.node, uris::LV2_DEFAULT),
            minimum: inner.model.first(&self.node, uris::LV2_MINIMUM),
            maximum: inner.model.first(&self.node, uris::LV2_MAXIMUM),
        }
    }

    /// Scale points declared for this port.
    #[must_use]
    pub fn scale_points(&self) -> ScalePoints {
        let inner = self.plugin.world.lock();
        let points = inner
            .model
            .objects(&self.node, uris::LV2_SCALE_POINT)
            .into_iter()
            .filter_map(|point| {
                let label = inner.first(&point, uris::RDFS_LABEL)?;
                let value = inner.model.first(&point, uris::RDF_VALUE)?;
                Some(ScalePoint { label, value })
            })
            .collect();
        ScalePoints { points }
    }
}

impl Debug for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Port")
            .field("name", &self.name())
            .field("symbol", &self.symbol())
            .field("classes", &self.classes())
            .field("range", &self.range())
            .field("properties", &self.properties())
            .finish()
    }
}

/// Describes the ranges of the port if possible.
#[derive(Clone, Debug, PartialEq)]
pub struct Range {
    /// The default value of the port.
    pub default: Option<Node>,
    /// The minimum value of the port.
    pub minimum: Option<Node>,
    /// The maximum value of the port.
    pub maximum: Option<Node>,
}

/// One port scale point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalePoint {
    label: Node,
    value: Node,
}

impl ScalePoint {
    #[must_use]
    pub fn label(&self) -> Node {
        self.label.clone()
    }

    #[must_use]
    pub fn value(&self) -> Node {
        self.value.clone()
    }
}

/// A collection of scale points.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScalePoints {
    points: Vec<ScalePoint>,
}

impl ScalePoints {
    #[must_use]
    pub fn count(&self) -> usize {
        self.points.len()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn iter(&self) -> impl '_ + Iterator<Item = ScalePoint> {
        self.points.iter().cloned()
    }
}

impl IntoIterator for ScalePoints {
    type Item = ScalePoint;
    type IntoIter = std::vec::IntoIter<ScalePoint>;

    fn into_iter(self) -> Self::IntoIter {
        self.points.into_iter()
    }
}
