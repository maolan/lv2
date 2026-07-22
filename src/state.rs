//! LV2 state and preset metadata.

use crate::model::{Model, Triple};
use crate::node::Node;
use crate::uris;
use crate::world::World;
use oxttl::TurtleParser;
use std::path::{Path, PathBuf};

/// A saved LV2 state or preset, including metadata and scalar port values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    plugin_uri: Option<Node>,
    uri: Option<Node>,
    label: Option<String>,
    bundle_path: Option<PathBuf>,
    properties: Vec<StateProperty>,
    port_values: Vec<StatePortValue>,
}

/// One port value in an LV2 state/preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatePortValue {
    symbol: String,
    value: Node,
}

/// One arbitrary RDF metadata property attached to a state or preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateProperty {
    predicate: Node,
    value: Node,
}

impl StateProperty {
    #[must_use]
    pub fn predicate(&self) -> Node {
        self.predicate.clone()
    }

    #[must_use]
    pub fn value(&self) -> Node {
        self.value.clone()
    }
}

impl StatePortValue {
    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    #[must_use]
    pub fn value(&self) -> Node {
        self.value.clone()
    }
}

impl State {
    /// Reads a state or preset already loaded in `world`.
    #[must_use]
    pub fn new_from_world(world: &World, node: &Node) -> Option<Self> {
        let inner = world.lock();
        state_from_model(&inner.model, node)
    }

    /// Reads a state or preset from a Turtle file.
    #[must_use]
    pub fn new_from_file(
        path: impl AsRef<std::path::Path>,
        subject: Option<&Node>,
    ) -> Option<Self> {
        let path = path.as_ref();
        let base = uris::path_to_file_uri(&path.to_string_lossy());
        let data = std::fs::read(path).ok()?;
        let model = parse_turtle(&data, &base)?;
        let subject = subject.cloned().or_else(|| infer_state_subject(&model))?;
        let mut state = state_from_model(&model, &subject)?;
        state.bundle_path = path.parent().map(Path::to_path_buf);
        Some(state)
    }

    /// Reads a state or preset from a Turtle string.
    #[must_use]
    pub fn new_from_string(data: &str, subject: Option<&Node>) -> Option<Self> {
        let model = parse_turtle(data.as_bytes(), "file:///maolan-lv2-state.ttl")?;
        let subject = subject.cloned().or_else(|| infer_state_subject(&model))?;
        state_from_model(&model, &subject)
    }

    #[must_use]
    pub fn plugin_uri(&self) -> Option<Node> {
        self.plugin_uri.clone()
    }

    #[must_use]
    pub fn uri(&self) -> Option<Node> {
        self.uri.clone()
    }

    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    #[must_use]
    pub fn bank(&self) -> Option<Node> {
        self.property_values(&Node::uri(uris::PRESETS_BANK))
            .into_iter()
            .next()
    }

    #[must_use]
    pub fn bundle_path(&self) -> Option<&Path> {
        self.bundle_path.as_deref()
    }

    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = Some(label.into());
    }

    pub fn clear_label(&mut self) {
        self.label = None;
    }

    pub fn set_uri(&mut self, uri: Option<Node>) {
        self.uri = uri;
    }

    pub fn set_plugin_uri(&mut self, uri: Node) {
        self.plugin_uri = Some(uri);
    }

    #[must_use]
    pub fn properties(&self) -> &[StateProperty] {
        &self.properties
    }

    #[must_use]
    pub fn num_properties(&self) -> usize {
        self.properties.len()
    }

    pub fn set_metadata(&mut self, predicate: Node, value: Node) {
        self.properties.push(StateProperty { predicate, value });
    }

    #[must_use]
    pub fn property_values(&self, predicate: &Node) -> Vec<Node> {
        self.properties
            .iter()
            .filter(|property| &property.predicate == predicate)
            .map(StateProperty::value)
            .collect()
    }

    #[must_use]
    pub fn port_values(&self) -> &[StatePortValue] {
        &self.port_values
    }

    pub fn add_port_value(&mut self, symbol: impl Into<String>, value: Node) {
        let symbol = symbol.into();
        if let Some(existing) = self
            .port_values
            .iter_mut()
            .find(|value| value.symbol == symbol)
        {
            existing.value = value;
        } else {
            self.port_values.push(StatePortValue { symbol, value });
        }
    }

    pub fn emit_port_values(&self, mut emit: impl FnMut(&str, &Node)) {
        for value in &self.port_values {
            emit(&value.symbol, &value.value);
        }
    }

    /// Serializes this state as LV2 preset-compatible Turtle.
    #[must_use]
    pub fn to_turtle_string(&self, uri: Option<&str>) -> String {
        let subject = uri
            .map(Node::uri)
            .or_else(|| self.uri.clone())
            .unwrap_or_else(|| Node::uri(""));
        let mut out = String::new();
        out.push_str("@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n");
        out.push_str("@prefix pset: <http://lv2plug.in/ns/ext/presets#> .\n");
        out.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n");
        out.push_str(&format!("{}\n", subject.turtle_token()));
        out.push_str("    a pset:Preset");
        if let Some(plugin_uri) = &self.plugin_uri {
            out.push_str(&format!(
                " ;\n    lv2:appliesTo {}",
                plugin_uri.turtle_token()
            ));
        }
        if let Some(label) = &self.label {
            out.push_str(&format!(
                " ;\n    rdfs:label {}",
                Node::string(label).turtle_token()
            ));
        }
        for property in &self.properties {
            if property.predicate.as_uri() == Some(uris::RDF_TYPE)
                || property.predicate.as_uri() == Some(uris::LV2_APPLIES_TO)
                || property.predicate.as_uri() == Some(uris::RDFS_LABEL)
            {
                continue;
            }
            out.push_str(&format!(
                " ;\n    {} {}",
                property.predicate.turtle_token(),
                property.value.turtle_token()
            ));
        }
        for value in &self.port_values {
            out.push_str(&format!(
                " ;\n    lv2:port [ lv2:symbol {} ; pset:value {} ]",
                Node::string(&value.symbol).turtle_token(),
                value.value.turtle_token()
            ));
        }
        out.push_str(" .\n");
        out
    }

    /// Saves this state to `dir/filename` and writes/updates the bundle manifest.
    pub fn save(
        &mut self,
        dir: impl AsRef<Path>,
        filename: &str,
        uri: Option<&str>,
    ) -> Result<(), String> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|error| format!("create state dir: {error}"))?;
        let state_uri = uri
            .map(str::to_string)
            .or_else(|| {
                self.uri
                    .as_ref()
                    .and_then(|node| node.as_uri().map(str::to_string))
            })
            .unwrap_or_else(|| uris::path_to_file_uri(&dir.join(filename).to_string_lossy()));
        let data = self.to_turtle_string(Some(&state_uri));
        std::fs::write(dir.join(filename), data)
            .map_err(|error| format!("write state: {error}"))?;
        let manifest_path = dir.join("manifest.ttl");
        let mut manifest = std::fs::read_to_string(&manifest_path).unwrap_or_default();
        let see_also = Node::uri(filename);
        let entry = format!(
            "{}\n    a <{}> ;\n    <{}> {} .\n",
            Node::uri(&state_uri).turtle_token(),
            uris::PRESETS_PRESET,
            uris::RDFS_SEE_ALSO,
            see_also.turtle_token()
        );
        if !manifest.contains(&state_uri) {
            if !manifest.is_empty() && !manifest.ends_with('\n') {
                manifest.push('\n');
            }
            manifest.push_str(&entry);
            std::fs::write(&manifest_path, manifest)
                .map_err(|error| format!("write manifest: {error}"))?;
        }
        self.uri = Some(Node::uri(state_uri));
        self.bundle_path = Some(dir.to_path_buf());
        Ok(())
    }

    /// Removes this state from its saved bundle directory.
    pub fn delete(&self, world: Option<&World>) -> Result<(), String> {
        let Some(dir) = &self.bundle_path else {
            return Err("State has no bundle path".to_string());
        };
        let Some(uri) = self.uri.as_ref() else {
            return Err("State has no URI".to_string());
        };
        if let Some(world) = world {
            let _ = world.unload_resource(uri);
        }
        let manifest_path = dir.join("manifest.ttl");
        let manifest = std::fs::read_to_string(&manifest_path).unwrap_or_default();
        let uri_string = uri.as_uri().unwrap_or_default();
        let data_files = manifest_see_also_files(&manifest, uri_string);
        for file in data_files {
            let path = dir.join(file);
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|error| format!("delete state file: {error}"))?;
            }
        }
        if manifest_path.exists() {
            let filtered = remove_manifest_subject(&manifest, uri_string);
            if filtered.trim().is_empty() {
                std::fs::remove_file(&manifest_path)
                    .map_err(|error| format!("delete manifest: {error}"))?;
            } else {
                std::fs::write(&manifest_path, filtered)
                    .map_err(|error| format!("rewrite manifest: {error}"))?;
            }
        }
        if std::fs::read_dir(dir)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
        {
            std::fs::remove_dir(dir).map_err(|error| format!("delete state dir: {error}"))?;
        }
        Ok(())
    }

    #[must_use]
    pub fn port_value(&self, symbol: &str) -> Option<Node> {
        self.port_values
            .iter()
            .find(|value| value.symbol == symbol)
            .map(StatePortValue::value)
    }
}

fn infer_state_subject(model: &Model) -> Option<Node> {
    let preset = Node::uri(uris::PRESETS_PRESET);
    model
        .find(None, Some(uris::RDF_TYPE), Some(&preset))
        .into_iter()
        .next()
        .map(|triple| triple.subject)
        .or_else(|| {
            model
                .find(None, Some(uris::LV2_APPLIES_TO), None)
                .into_iter()
                .next()
                .map(|triple| triple.subject)
        })
}

fn state_from_model(model: &Model, node: &Node) -> Option<State> {
    if model.ask(node, uris::RDF_TYPE, &Node::uri(uris::LV2_PLUGIN)) {
        let label = model
            .first(node, uris::RDFS_LABEL)
            .and_then(|node| node.as_str().map(str::to_string));
        return Some(State {
            plugin_uri: Some(node.clone()),
            uri: node.as_uri().map(Node::uri),
            label,
            bundle_path: None,
            properties: Vec::new(),
            port_values: Vec::new(),
        });
    }
    let plugin_uri = model.first(node, uris::LV2_APPLIES_TO);
    let label = model
        .first(node, uris::RDFS_LABEL)
        .and_then(|node| node.as_str().map(str::to_string));
    let properties = model
        .find(Some(node), None, None)
        .into_iter()
        .filter(|triple| triple.predicate != uris::LV2_PORT)
        .map(|triple| StateProperty {
            predicate: Node::uri(triple.predicate),
            value: triple.object,
        })
        .collect();
    let port_values = model
        .objects(node, uris::LV2_PORT)
        .into_iter()
        .filter_map(|port| {
            let symbol = model
                .first(&port, uris::LV2_SYMBOL)?
                .as_str()
                .map(str::to_string)?;
            let value = model
                .first(&port, uris::PRESETS_VALUE)
                .or_else(|| model.first(&port, uris::LV2_DEFAULT))?;
            Some(StatePortValue { symbol, value })
        })
        .collect();
    Some(State {
        plugin_uri,
        uri: node.as_uri().map(Node::uri),
        label,
        bundle_path: None,
        properties,
        port_values,
    })
}

fn manifest_see_also_files(manifest: &str, uri: &str) -> Vec<String> {
    let subject = Node::uri(uri).turtle_token();
    let mut files = Vec::new();
    let mut in_subject = false;
    for line in manifest.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&subject) {
            in_subject = true;
        }
        if in_subject && trimmed.contains("seeAlso") {
            let mut rest = trimmed;
            while let Some(start) = rest.find('<') {
                rest = &rest[start + 1..];
                let Some(end) = rest.find('>') else {
                    break;
                };
                let value = &rest[..end];
                if value != uris::RDFS_SEE_ALSO {
                    files.push(value.to_string());
                }
                rest = &rest[end + 1..];
            }
        }
        if in_subject && trimmed.ends_with('.') {
            in_subject = false;
        }
    }
    files
}

fn remove_manifest_subject(manifest: &str, uri: &str) -> String {
    let subject = Node::uri(uri).turtle_token();
    let mut out = String::new();
    let mut skipping = false;
    for line in manifest.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&subject) {
            skipping = true;
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
        if skipping && trimmed.ends_with('.') {
            skipping = false;
        }
    }
    out
}

fn parse_turtle(data: &[u8], base: &str) -> Option<Model> {
    let parser = TurtleParser::new().with_base_iri(base).ok()?;
    let mut model = Model::default();
    for result in parser.for_slice(data) {
        let triple = result.ok()?;
        let subject = match triple.subject {
            oxrdf::NamedOrBlankNode::NamedNode(node) => Node::uri(node.into_string()),
            oxrdf::NamedOrBlankNode::BlankNode(node) => {
                Node::blank(format!("state:{}", node.as_str()))
            }
        };
        let object = match triple.object {
            oxrdf::Term::NamedNode(node) => Node::uri(node.into_string()),
            oxrdf::Term::BlankNode(node) => Node::blank(format!("state:{}", node.as_str())),
            oxrdf::Term::Literal(literal) => {
                let (value, datatype, language) = literal.destruct();
                Node::literal(value, datatype.map(|d| d.into_string()), language)
            }
        };
        model.insert(
            base,
            &Triple {
                subject,
                predicate: triple.predicate.into_string(),
                object,
            },
        );
    }
    Some(model)
}
