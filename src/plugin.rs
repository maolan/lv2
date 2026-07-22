//! Plugins: metadata queries and instantiation.

use crate::instance::Instance;
use crate::node::{Node, Nodes};
use crate::port::{Port, PortData};
use crate::raw::LV2Feature;
use crate::uris;
use crate::world::World;
use std::fmt::Debug;
use std::sync::Arc;

/// Metadata for one UI advertised by an LV2 plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ui {
    /// URI of the UI descriptor.
    pub uri: String,
    /// Primary UI class/type URI, for example `ui:X11UI`.
    pub class_uri: String,
    /// All RDF types declared for the UI.
    pub class_uris: Vec<String>,
    /// Extension data interfaces declared for the UI.
    pub extension_data_uris: Vec<String>,
    /// Bundle URI that provided this UI metadata.
    pub bundle_uri: String,
    /// Shared library URI for the UI.
    pub binary_uri: String,
}

impl Ui {
    #[must_use]
    pub fn uri(&self) -> Node {
        Node::uri(&self.uri)
    }

    #[must_use]
    pub fn classes(&self) -> Nodes {
        Nodes::new(self.class_uris.iter().map(Node::uri).collect())
    }

    #[must_use]
    pub fn is_a(&self, class: &Node) -> bool {
        class
            .as_uri()
            .is_some_and(|uri| self.class_uris.iter().any(|c| c == uri))
    }

    #[must_use]
    pub fn bundle_uri(&self) -> Node {
        Node::uri(&self.bundle_uri)
    }

    #[must_use]
    pub fn binary_uri(&self) -> Node {
        Node::uri(&self.binary_uri)
    }

    #[must_use]
    pub fn extension_data(&self) -> Nodes {
        Nodes::new(self.extension_data_uris.iter().map(Node::uri).collect())
    }

    /// Returns the first supported UI class from `native_classes`.
    #[must_use]
    pub fn supported_class<'a>(
        &self,
        native_classes: impl IntoIterator<Item = &'a Node>,
    ) -> Option<Node> {
        native_classes
            .into_iter()
            .find(|class| self.is_a(class))
            .cloned()
    }

    /// Returns the best supported UI type and quality using a Lilv-style callback.
    #[must_use]
    pub fn support_quality<F>(&self, container_type: &Node, mut supported: F) -> Option<(u32, Node)>
    where
        F: FnMut(&str, &str) -> u32,
    {
        let container_type = container_type.as_uri()?;
        self.class_uris
            .iter()
            .filter_map(|class| {
                let quality = supported(container_type, class);
                (quality > 0).then(|| (quality, Node::uri(class)))
            })
            .max_by_key(|(quality, _)| *quality)
    }
}

/// Counts of common plugin IO ports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortCounts {
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
}

/// A discovered LV2 plugin. Cheap to clone; all clones refer to the same
/// plugin in the shared world.
#[derive(Clone)]
pub struct Plugin {
    pub(crate) world: World,
    pub(crate) uri: String,
}

impl Plugin {
    /// Returns true if the plugin is valid: its data was parsed without
    /// errors and it has a type, a name, a binary, and well-formed ports
    /// (see `lilv_plugin_verify`).
    #[must_use]
    pub fn verify(&self) -> bool {
        let mut inner = self.world.lock();
        let Some(data) = inner.plugin_data(&self.uri) else {
            return false;
        };
        if inner.bundle_has_parse_errors(&data.bundle_uri) {
            return false;
        }
        let subject = Node::uri(&self.uri);
        if inner
            .model
            .objects_iter(&subject, uris::RDF_TYPE)
            .next()
            .is_none()
        {
            return false;
        }
        if inner
            .model
            .objects_iter(&subject, uris::DOAP_NAME)
            .next()
            .is_none()
        {
            return false;
        }
        if data.binary_uri.is_none() {
            return false;
        }
        let ports = load_ports(&mut inner, &self.uri);
        !ports.is_empty() && ports.iter().all(|port| port.is_some())
    }

    /// The uri of the plugin.
    #[must_use]
    pub fn uri(&self) -> Node {
        Node::uri(&self.uri)
    }

    /// The uri of the plugin's bundle.
    #[must_use]
    pub fn bundle_uri(&self) -> Node {
        let inner = self.world.lock();
        inner
            .plugin_data(&self.uri)
            .map(|data| Node::uri(&data.bundle_uri))
            .unwrap_or_else(|| Node::uri(""))
    }

    /// The uri for the library, if the plugin has one.
    #[must_use]
    pub fn library_uri(&self) -> Option<Node> {
        let inner = self.world.lock();
        inner
            .plugin_data(&self.uri)
            .and_then(|data| data.binary_uri.as_ref())
            .map(Node::uri)
    }

    /// The (human readable) name of the plugin. Returns an empty string node
    /// when the plugin has no `doap:name`.
    #[must_use]
    pub fn name(&self) -> Node {
        let inner = self.world.lock();
        let subject = Node::uri(&self.uri);
        plain_string(inner.objects(&subject, uris::DOAP_NAME)).unwrap_or_else(|| Node::string(""))
    }

    /// The class of the plugin.
    #[must_use]
    pub fn class(&self) -> Class {
        let inner = self.world.lock();
        let subject = Node::uri(&self.uri);
        let types = inner.model.objects(&subject, uris::RDF_TYPE);

        // The plugin class is the first type (other than lv2:Plugin itself)
        // that is a subclass of lv2:Plugin (see lilv_plugin_get_class).
        let class_uri = types
            .iter()
            .filter_map(|t| t.as_uri())
            .find(|t| *t != uris::LV2_PLUGIN && is_plugin_class(&inner, t))
            .unwrap_or(uris::LV2_PLUGIN)
            .to_string();

        let class_node = Node::uri(&class_uri);
        let label = plain_string(inner.objects(&class_node, uris::RDFS_LABEL))
            .and_then(|node| node.as_str().map(str::to_string))
            .unwrap_or_else(|| class_uri.clone());

        Class {
            world: self.world.clone(),
            uri: Some(class_uri),
            label,
        }
    }

    /// The value of the predicate. `Nodes` may be empty if the plugin does
    /// not have one.
    #[must_use]
    pub fn value(&self, predicate: &Node) -> Nodes {
        let inner = self.world.lock();
        let subject = Node::uri(&self.uri);
        let Some(predicate) = predicate.as_uri() else {
            return Nodes::default();
        };
        Nodes::new(inner.objects(&subject, predicate))
    }

    /// `true` if the plugin supports the feature.
    #[must_use]
    pub fn has_feature(&self, feature_uri: &Node) -> bool {
        self.required_features().contains(feature_uri)
            || self.optional_features().contains(feature_uri)
    }

    /// All supported features, required and optional.
    #[must_use]
    pub fn supported_features(&self) -> Nodes {
        self.required_features().merge(&self.optional_features())
    }

    /// The set of features that are required to instantiate the plugin.
    #[must_use]
    pub fn required_features(&self) -> Nodes {
        self.value(&Node::uri(uris::LV2_REQUIRED_FEATURE))
    }

    /// The set of features that are optional to instantiate the plugin.
    #[must_use]
    pub fn optional_features(&self) -> Nodes {
        self.value(&Node::uri(uris::LV2_OPTIONAL_FEATURE))
    }

    /// Returns `true` if the plugin has extension data for `uri`.
    #[must_use]
    pub fn has_extension_data(&self, uri: &Node) -> bool {
        let inner = self.world.lock();
        let subject = Node::uri(&self.uri);
        inner.model.ask(&subject, uris::LV2_EXTENSION_DATA, uri)
    }

    /// Extension data URIs advertised by this plugin.
    #[must_use]
    pub fn extension_data(&self) -> Nodes {
        self.value(&Node::uri(uris::LV2_EXTENSION_DATA))
    }

    /// Returns UIs advertised by this plugin.
    #[must_use]
    pub fn uis(&self) -> Vec<Ui> {
        let inner = self.world.lock();
        let subject = Node::uri(&self.uri);
        inner
            .model
            .objects(&subject, uris::UI_UI)
            .into_iter()
            .filter_map(|ui_node| {
                let ui_uri = ui_node.as_uri()?.to_string();
                let class_uris: Vec<String> = inner
                    .model
                    .objects(&ui_node, uris::RDF_TYPE)
                    .into_iter()
                    .filter_map(|node| node.as_uri().map(str::to_string))
                    .collect();
                let class_uri = class_uris.first()?.clone();
                let extension_data_uris: Vec<String> = inner
                    .model
                    .objects(&ui_node, uris::LV2_EXTENSION_DATA)
                    .into_iter()
                    .filter_map(|node| node.as_uri().map(str::to_string))
                    .collect();
                let binary_uri = inner
                    .model
                    .first(&ui_node, uris::LV2_BINARY)
                    .or_else(|| inner.model.first(&ui_node, uris::UI_BINARY))
                    .and_then(|node| node.as_uri().map(str::to_string))?;
                Some(Ui {
                    uri: ui_uri,
                    class_uri,
                    class_uris,
                    extension_data_uris,
                    bundle_uri: inner.plugin_data(&self.uri)?.bundle_uri.clone(),
                    binary_uri,
                })
            })
            .collect()
    }

    /// Returns UIs that are directly supported by common desktop UI classes.
    #[must_use]
    pub fn supported_uis(&self) -> Vec<Ui> {
        let native_classes = [
            Node::uri(uris::UI_X11_UI),
            Node::uri(uris::UI_GTK_UI),
            Node::uri(uris::UI_GTK3_UI),
            Node::uri(uris::UI_QT5_UI),
            Node::uri(uris::UI_QT4_UI),
        ];
        self.uis()
            .into_iter()
            .filter(|ui| native_classes.iter().any(|class| ui.is_a(class)))
            .collect()
    }

    /// Returns the number of ports for the plugin.
    #[must_use]
    pub fn ports_count(&self) -> usize {
        let mut inner = self.world.lock();
        load_ports(&mut inner, &self.uri).len()
    }

    /// Counts audio and MIDI ports in one pass over the cached port table.
    #[must_use]
    pub fn port_counts(&self) -> PortCounts {
        let mut inner = self.world.lock();
        let ports = load_ports(&mut inner, &self.uri);
        let mut counts = PortCounts::default();
        let input_port = Node::uri(uris::LV2_INPUT_PORT);
        let output_port = Node::uri(uris::LV2_OUTPUT_PORT);
        let audio_port = Node::uri(uris::LV2_AUDIO_PORT);
        let atom_port = Node::uri(uris::ATOM_ATOM_PORT);
        let event_port = Node::uri(uris::EVENT_EVENT_PORT);
        let midi_event = Node::uri(uris::MIDI_MIDI_EVENT);

        for data in ports.iter().flatten() {
            let is_input = inner.model.ask(&data.node, uris::RDF_TYPE, &input_port);
            let is_output = inner.model.ask(&data.node, uris::RDF_TYPE, &output_port);
            if inner.model.ask(&data.node, uris::RDF_TYPE, &audio_port) {
                if is_input {
                    counts.audio_inputs += 1;
                }
                if is_output {
                    counts.audio_outputs += 1;
                }
            }
            let is_event_or_atom = inner.model.ask(&data.node, uris::RDF_TYPE, &atom_port)
                || inner.model.ask(&data.node, uris::RDF_TYPE, &event_port);
            let is_midi = is_event_or_atom
                && (inner
                    .model
                    .ask(&data.node, uris::ATOM_SUPPORTS, &midi_event)
                    || inner
                        .model
                        .ask(&data.node, uris::EVENT_SUPPORTS_EVENT, &midi_event));
            if is_midi {
                if is_input {
                    counts.midi_inputs += 1;
                }
                if is_output {
                    counts.midi_outputs += 1;
                }
            }
        }
        counts
    }

    /// Returns an iterator over all the ports.
    pub fn iter_ports(&self) -> impl Iterator<Item = Port> {
        let ports = {
            let mut inner = self.world.lock();
            load_ports(&mut inner, &self.uri)
        };
        PortsIter {
            plugin: self.clone(),
            ports,
            index: 0,
        }
    }

    /// Return the port by index or `None` if it does not exist.
    #[must_use]
    pub fn port_by_index(&self, index: usize) -> Option<Port> {
        let mut inner = self.world.lock();
        let ports = load_ports(&mut inner, &self.uri);
        let data = ports.get(index)?.clone()?;
        Some(Port {
            plugin: self.clone(),
            node: data.node,
            index: data.index,
            symbol: data.symbol,
        })
    }

    /// Get the port by the symbol.
    ///
    /// Note: This function is slower than `port_by_index`, especially on
    /// plugins with a very large number of ports.
    #[must_use]
    pub fn port_by_symbol(&self, symbol: &Node) -> Option<Port> {
        let symbol = symbol.as_str()?;
        let mut inner = self.world.lock();
        let ports = load_ports(&mut inner, &self.uri);
        let data = ports.iter().flatten().find(|p| p.symbol == symbol)?.clone();
        Some(Port {
            plugin: self.clone(),
            node: data.node,
            index: data.index,
            symbol: data.symbol,
        })
    }

    /// Returns wether or not the plugin has a latency port.
    #[must_use]
    pub fn has_latency(&self) -> bool {
        self.latency_port_index().is_some()
    }

    /// Return the index of the plugin's latency port or `None` if it does
    /// not exist (see `lilv_plugin_get_latency_port_index`).
    #[must_use]
    pub fn latency_port_index(&self) -> Option<usize> {
        let mut inner = self.world.lock();
        let ports = load_ports(&mut inner, &self.uri);

        // A port with the lv2:reportsLatency property wins...
        for data in ports.iter().flatten() {
            if inner.model.ask(
                &data.node,
                uris::LV2_PORT_PROPERTY,
                &Node::uri(uris::LV2_REPORTS_LATENCY),
            ) {
                return Some(data.index);
            }
        }

        // ...otherwise an output port designated lv2:latency.
        let output_port = Node::uri(uris::LV2_OUTPUT_PORT);
        for data in ports.iter().flatten() {
            let designated = inner.model.ask(
                &data.node,
                uris::LV2_DESIGNATION,
                &Node::uri(uris::LV2_LATENCY),
            );
            let is_output = inner
                .model
                .objects_iter(&data.node, uris::RDF_TYPE)
                .any(|o| *o == output_port);
            if designated && is_output {
                return Some(data.index);
            }
        }
        None
    }

    /// Return the port matching a class and designation.
    #[must_use]
    pub fn port_by_designation(&self, port_class: &Node, designation: &Node) -> Option<Port> {
        self.port_by_designation_optional(Some(port_class), designation)
    }

    /// Return the port matching a designation, optionally constrained by class.
    #[must_use]
    pub fn port_by_designation_optional(
        &self,
        port_class: Option<&Node>,
        designation: &Node,
    ) -> Option<Port> {
        self.iter_ports().find(|port| {
            port_class.is_none_or(|port_class| port.is_a(port_class))
                && port.has_designation(designation)
        })
    }

    /// Count ports matching all given classes.
    #[must_use]
    pub fn num_ports_of_class<'a>(&self, classes: impl IntoIterator<Item = &'a Node>) -> usize {
        let classes: Vec<&Node> = classes.into_iter().collect();
        self.iter_ports()
            .filter(|port| classes.iter().all(|class| port.is_a(class)))
            .count()
    }

    /// Batch range query for all ports. Missing values are returned as `None`.
    #[must_use]
    pub fn port_ranges_float(&self) -> Vec<(Option<f32>, Option<f32>, Option<f32>)> {
        self.iter_ports()
            .map(|port| {
                let range = port.range();
                (
                    range.minimum.as_ref().and_then(Node::as_float),
                    range.maximum.as_ref().and_then(Node::as_float),
                    range.default.as_ref().and_then(Node::as_float),
                )
            })
            .collect()
    }

    /// Project resource associated with this plugin.
    #[must_use]
    pub fn project(&self) -> Option<Node> {
        self.value(&Node::uri(uris::LV2_PROJECT)).first()
    }

    /// Author name, if declared.
    #[must_use]
    pub fn author_name(&self) -> Option<Node> {
        self.author_value(uris::FOAF_NAME)
    }

    /// Author email URI, if declared.
    #[must_use]
    pub fn author_email(&self) -> Option<Node> {
        self.author_value(uris::FOAF_MBOX)
    }

    /// Author homepage URI, if declared.
    #[must_use]
    pub fn author_homepage(&self) -> Option<Node> {
        self.author_value(uris::FOAF_HOMEPAGE)
            .or_else(|| self.author_value(uris::DOAP_HOMEPAGE))
    }

    /// Returns true if another loaded resource declares `dct:replaces` for this plugin.
    #[must_use]
    pub fn is_replaced(&self) -> bool {
        self.world.lock().is_replaced(&self.uri)
    }

    /// Data files related to this plugin.
    #[must_use]
    pub fn data_uris(&self) -> Nodes {
        self.value(&Node::uri(uris::RDFS_SEE_ALSO))
    }

    /// Resources that apply to this plugin and optionally have `rdf:type == type_uri`.
    #[must_use]
    pub fn related(&self, type_uri: Option<&Node>) -> Nodes {
        let inner = self.world.lock();
        let subject = Node::uri(&self.uri);
        let mut nodes = Vec::new();
        for triple in inner
            .model
            .find(None, Some(uris::LV2_APPLIES_TO), Some(&subject))
        {
            let matches_type = type_uri
                .is_none_or(|type_uri| inner.model.ask(&triple.subject, uris::RDF_TYPE, type_uri));
            if matches_type && !nodes.contains(&triple.subject) {
                nodes.push(triple.subject);
            }
        }
        Nodes::new(nodes)
    }

    /// Presets that apply to this plugin and are already loaded in the world.
    #[must_use]
    pub fn presets(&self) -> Nodes {
        self.related(Some(&Node::uri(uris::PRESETS_PRESET)))
    }

    fn author_value(&self, predicate: &str) -> Option<Node> {
        let inner = self.world.lock();
        let subject = Node::uri(&self.uri);
        let author = inner
            .model
            .first(&subject, uris::DOAP_MAINTAINER)
            .or_else(|| {
                let project = inner.model.first(&subject, uris::LV2_PROJECT)?;
                inner.model.first(&project, uris::DOAP_MAINTAINER)
            })?;
        inner.first(&author, predicate)
    }

    /// Serializes a compact Turtle description for this plugin from loaded metadata.
    #[must_use]
    pub fn description_turtle(&self) -> String {
        let mut out = String::new();
        out.push_str("@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n");
        out.push_str("@prefix doap: <http://usefulinc.com/ns/doap#> .\n");
        out.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n");
        out.push_str(&format!("{}\n", self.uri().turtle_token()));
        out.push_str("    a lv2:Plugin");
        if let Some(class) = self
            .class()
            .uri()
            .and_then(|node| node.as_uri().map(str::to_string))
            && class != uris::LV2_PLUGIN
        {
            out.push_str(&format!(" , <{class}>"));
        }
        out.push_str(" ;\n");
        if let Some(name) = self.name().as_str() {
            out.push_str(&format!(
                "    doap:name {} ;\n",
                Node::string(name).turtle_token()
            ));
        }
        for data_uri in self.data_uris() {
            out.push_str(&format!("    rdfs:seeAlso {} ;\n", data_uri.turtle_token()));
        }
        if let Some(binary) = self.library_uri() {
            out.push_str(&format!("    lv2:binary {} ;\n", binary.turtle_token()));
        }
        out.push_str("    .\n");
        out
    }

    /// Serializes a Turtle manifest entry for this plugin.
    #[must_use]
    pub fn manifest_entry_turtle(&self) -> String {
        let mut out = String::new();
        out.push_str("@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n");
        out.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n");
        out.push_str(&format!("{}\n", self.uri().turtle_token()));
        out.push_str("    a lv2:Plugin ;\n");
        if let Some(binary) = self.library_uri() {
            out.push_str(&format!("    lv2:binary {} ;\n", binary.turtle_token()));
        }
        for data_uri in self.data_uris() {
            out.push_str(&format!("    rdfs:seeAlso {} ;\n", data_uri.turtle_token()));
        }
        out.push_str("    .\n");
        out
    }

    /// Instantiate a plugin.
    ///
    /// This resolves the plugin's `lv2:binary`, loads it with `dlopen`, finds
    /// the descriptor for this plugin's URI, and calls its `instantiate`
    /// callback with the given features.
    ///
    /// # Safety
    /// Instantiating a plugin calls the plugin's code which itself may be
    /// unsafe.
    #[must_use]
    pub unsafe fn instantiate<'a, FS>(&self, sample_rate: f64, features: FS) -> Option<Instance>
    where
        FS: IntoIterator<Item = &'a LV2Feature>,
    {
        let (library_uri, bundle_uri, num_ports) = {
            let mut inner = self.world.lock();
            let data = inner.plugin_data(&self.uri)?;
            (
                data.binary_uri.clone()?,
                data.bundle_uri.clone(),
                load_ports(&mut inner, &self.uri).len() as u32,
            )
        };
        let features_vec: Vec<*const LV2Feature> = features
            .into_iter()
            .map(|f| f as *const LV2Feature)
            .chain(std::iter::once(std::ptr::null()))
            .collect();
        // SAFETY: the caller guarantees instantiating plugin code is
        // acceptable; the features array outlives the call.
        unsafe {
            Instance::new(
                &self.uri,
                &library_uri,
                &bundle_uri,
                sample_rate,
                features_vec.as_ptr(),
                num_ports,
            )
        }
    }
}

/// Picks the "best" human-readable string from a set of literal nodes: the
/// first plain (language-less) string literal, falling back to the first
/// string literal at all.
fn plain_string(nodes: Vec<Node>) -> Option<Node> {
    let strings: Vec<Node> = nodes.into_iter().filter(Node::is_string).collect();
    strings
        .iter()
        .find(|n| {
            n.as_literal_parts()
                .is_some_and(|(_, _, lang)| lang.is_none())
        })
        .cloned()
        .or_else(|| strings.first().cloned())
}

/// Walks the `rdfs:subClassOf` hierarchy from `class_uri` looking for
/// `lv2:Plugin` (with a cycle guard).
fn is_plugin_class(inner: &crate::world::WorldInner, class_uri: &str) -> bool {
    let mut work = vec![class_uri.to_string()];
    let mut visited = std::collections::HashSet::new();
    while let Some(uri) = work.pop() {
        if uri == uris::LV2_PLUGIN {
            return true;
        }
        if !visited.insert(uri.clone()) {
            continue;
        }
        for parent in inner
            .model
            .objects(&Node::uri(&uri), uris::RDFS_SUB_CLASS_OF)
        {
            if let Some(parent) = parent.as_uri() {
                work.push(parent.to_string());
            }
        }
    }
    false
}

/// Loads the port table of a plugin, mirroring
/// `lilv_plugin_load_ports_if_necessary`.
///
/// The table is built once per plugin and cached in the world (cleared
/// whenever a bundle is loaded); the returned `Arc` makes repeated access
/// and port iteration cheap.
///
/// The returned vector has one slot per port index. It is empty when the
/// port data is invalid (bad symbols, non-integer indices, missing
/// indices), like lilv freeing the ports on error.
pub(crate) fn load_ports(
    inner: &mut crate::world::WorldInner,
    plugin_uri: &str,
) -> Arc<Vec<Option<PortData>>> {
    if let Some(cached) = inner.ports_cache.get(plugin_uri) {
        return cached.clone();
    }
    let ports = Arc::new(build_port_table(inner, plugin_uri));
    inner
        .ports_cache
        .insert(plugin_uri.to_string(), ports.clone());
    ports
}

/// Builds the port table of a plugin from the model (see [`load_ports`]).
fn build_port_table(inner: &crate::world::WorldInner, plugin_uri: &str) -> Vec<Option<PortData>> {
    let subject = Node::uri(plugin_uri);
    let mut ports: Vec<Option<PortData>> = Vec::new();
    for node in inner.model.objects_iter(&subject, uris::LV2_PORT) {
        let symbol = inner
            .model
            .first(node, uris::LV2_SYMBOL)
            .and_then(|s| s.as_literal_parts().map(|(v, _, _)| v.to_string()));
        let symbol = match symbol {
            Some(symbol) if uris::is_symbol(&symbol) => symbol,
            other => {
                let shown = other.unwrap_or_default();
                lv2_log!(
                    "maolan_lv2_plugin_load_ports(): error: Plugin <{plugin_uri}> port symbol \"{shown}\" is invalid"
                );
                return Vec::new();
            }
        };
        let index = inner
            .model
            .first(node, uris::LV2_INDEX)
            .and_then(|i| i.as_int());
        let Some(index) = index else {
            lv2_log!(
                "maolan_lv2_plugin_load_ports(): error: Plugin <{plugin_uri}> port index is not an integer"
            );
            return Vec::new();
        };
        let Ok(index) = usize::try_from(index) else {
            lv2_log!(
                "maolan_lv2_plugin_load_ports(): error: Plugin <{plugin_uri}> port index is negative"
            );
            return Vec::new();
        };
        if ports.len() <= index {
            ports.resize_with(index + 1, || None);
        }
        if ports[index].is_none() {
            ports[index] = Some(PortData {
                node: node.clone(),
                index,
                symbol,
            });
        }
    }

    // Check sanity: indices must be contiguous.
    for (index, port) in ports.iter().enumerate() {
        if port.is_none() {
            lv2_log!(
                "maolan_lv2_plugin_load_ports(): error: Plugin <{plugin_uri}> is missing port {index}"
            );
            return Vec::new();
        }
    }
    ports
}

impl Debug for Plugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plugin")
            .field("uri", &self.uri())
            .field("bundle_uri", &self.bundle_uri())
            .field("library_uri", &self.library_uri())
            .field("class", &self.class())
            .field("required_features", &self.required_features())
            .field("optional_features", &self.optional_features())
            .field("ports_count", &self.ports_count())
            .field("has_latency", &self.has_latency())
            .finish()
    }
}

/// A collection of plugins.
pub struct Plugins {
    pub(crate) world: World,
}

impl Plugins {
    /// An iterable over all the plugins in the world.
    pub fn iter(&self) -> impl '_ + Iterator<Item = Plugin> {
        let uris: Vec<String> = {
            let inner = self.world.lock();
            inner.plugins.iter().map(|p| p.uri.clone()).collect()
        };
        uris.into_iter().map(|uri| Plugin {
            world: self.world.clone(),
            uri,
        })
    }

    /// Get a plugin by its unique identifier.
    #[must_use]
    pub fn plugin(&self, uri: &Node) -> Option<Plugin> {
        let uri = uri.as_uri()?;
        let inner = self.world.lock();
        inner.plugin_data(uri)?;
        Some(Plugin {
            world: self.world.clone(),
            uri: uri.to_string(),
        })
    }

    /// The number of plugins loaded.
    #[must_use]
    pub fn count(&self) -> usize {
        let inner = self.world.lock();
        inner.plugins.len()
    }
}

impl IntoIterator for Plugins {
    type Item = Plugin;
    type IntoIter = std::vec::IntoIter<Plugin>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter().collect::<Vec<_>>().into_iter()
    }
}

/// An iterator over the ports of a plugin.
struct PortsIter {
    plugin: Plugin,
    ports: Arc<Vec<Option<PortData>>>,
    index: usize,
}

impl Iterator for PortsIter {
    type Item = Port;

    fn next(&mut self) -> Option<Port> {
        let index = self.index;
        self.index += 1;
        let data = self.ports.get(index)?.clone()?;
        Some(Port {
            plugin: self.plugin.clone(),
            node: data.node,
            index: data.index,
            symbol: data.symbol,
        })
    }
}

/// A plugin class.
///
/// Examples of this include "Reverb Plugin" and "Instrument Plugin".
#[derive(Clone)]
pub struct Class {
    world: World,
    uri: Option<String>,
    label: String,
}

impl Class {
    pub(crate) fn new(world: World, uri: String, label: String) -> Self {
        Self {
            world,
            uri: Some(uri),
            label,
        }
    }

    /// The label of this plugin class, ie "Amplifier Plugin".
    #[must_use]
    pub fn label(&self) -> Node {
        Node::string(&self.label)
    }

    /// The URI for the plugin class.
    #[must_use]
    pub fn uri(&self) -> Option<Node> {
        self.uri.as_ref().map(Node::uri)
    }

    /// The URI of the this class' superclass.
    ///
    /// For example "Instrument Plugin" belongs to "Generator Plugin".
    #[must_use]
    pub fn parent_uri(&self) -> Option<Node> {
        let uri = self.uri.as_ref()?;
        let inner = self.world.lock();
        inner.model.first(&Node::uri(uri), uris::RDFS_SUB_CLASS_OF)
    }

    /// Child classes of this class.
    #[must_use]
    pub fn children(&self) -> PluginClasses {
        let Some(uri) = self.uri.as_ref() else {
            return PluginClasses {
                world: self.world.clone(),
                uris: Vec::new(),
            };
        };
        let inner = self.world.lock();
        let parent = Node::uri(uri);
        let uris = inner
            .model
            .find(None, Some(uris::RDFS_SUB_CLASS_OF), Some(&parent))
            .into_iter()
            .filter_map(|triple| triple.subject.as_uri().map(str::to_string))
            .collect();
        PluginClasses {
            world: self.world.clone(),
            uris,
        }
    }
}

/// A collection of plugin classes.
pub struct PluginClasses {
    world: World,
    uris: Vec<String>,
}

impl PluginClasses {
    pub(crate) fn new(world: World, uris: Vec<String>) -> Self {
        Self { world, uris }
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.uris.len()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.uris.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.uris.is_empty()
    }

    pub fn iter(&self) -> impl '_ + Iterator<Item = Class> {
        self.uris
            .iter()
            .filter_map(|uri| class_from_uri(&self.world, uri))
    }

    #[must_use]
    pub fn get_by_uri(&self, uri: &Node) -> Option<Class> {
        let uri = uri.as_uri()?;
        if self.uris.iter().any(|candidate| candidate == uri) {
            class_from_uri(&self.world, uri)
        } else {
            None
        }
    }
}

impl IntoIterator for PluginClasses {
    type Item = Class;
    type IntoIter = std::vec::IntoIter<Class>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter().collect::<Vec<_>>().into_iter()
    }
}

fn class_from_uri(world: &World, uri: &str) -> Option<Class> {
    let inner = world.lock();
    let node = Node::uri(uri);
    let label = plain_string(inner.objects(&node, uris::RDFS_LABEL))
        .and_then(|node| node.as_str().map(str::to_string))
        .unwrap_or_else(|| uri.to_string());
    Some(Class {
        world: world.clone(),
        uri: Some(uri.to_string()),
        label,
    })
}

impl Debug for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginClass")
            .field("label", &self.label())
            .field("uri", &self.uri())
            .finish()
    }
}
