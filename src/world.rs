//! The world: discovery, loading, and storage of all LV2 data.

use crate::model::{Model, Triple};
use crate::node::{Node, Nodes};
use crate::plugin::{Class, PluginClasses, Plugins};
use crate::port::PortData;
use crate::raw::LV2Feature;
use crate::uris;
use oxttl::TurtleParser;
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// Metadata about one discovered plugin, recorded at bundle-load time.
pub(crate) struct PluginData {
    pub uri: String,
    pub bundle_uri: String,
    pub binary_uri: Option<String>,
    pub minor_version: u32,
    pub micro_version: u32,
}

pub(crate) struct WorldInner {
    pub model: Model,
    /// URIs of Turtle files that have already been loaded (cycle guard).
    loaded_files: HashSet<String>,
    /// All known plugins, sorted by URI.
    pub plugins: Vec<PluginData>,
    /// Bundles whose data files had parse errors (fails `verify()`).
    parse_error_bundles: HashSet<String>,
    /// Data file graphs loaded from each bundle, used for replacement and unload.
    bundle_files: HashMap<String, HashSet<String>>,
    /// URIs marked as replaced by each loaded graph via dct:replaces.
    replaced_by_graph: HashMap<String, HashSet<String>>,
    /// Port tables by plugin URI, built lazily and cleared whenever the
    /// model changes (mirrors lilv's `plugin->ports` caching).
    pub(crate) ports_cache: HashMap<String, Arc<Vec<Option<PortData>>>>,
    /// Parsed Turtle files by URI, so files consulted for version skimming
    /// are not parsed again when their data is loaded into the model.
    parse_cache: HashMap<String, Arc<ParsedFile>>,
    /// Counter used to namespace blank nodes per parsed file, like
    /// `lilv_world_blank_node_prefix`.
    file_counter: usize,
    /// Whether language-tagged string results are filtered to the active language.
    filter_lang: bool,
    /// Active BCP47-ish language tag, normalized from LANG when not overridden.
    lang: Option<String>,
    /// Optional LV2 bundle search path override.
    lv2_path: Option<Vec<PathBuf>>,
    /// Whether discovery may execute LV2 dynmanifest libraries.
    dyn_manifest_enabled: bool,
}

/// The world represents all LV2 state. It is used to discover, load, and
/// query LV2 data (plugins and specifications).
///
/// Cloning a `World` is cheap: all clones share the same underlying state.
#[derive(Clone)]
pub struct World {
    pub(crate) inner: Arc<Mutex<WorldInner>>,
}

impl World {
    /// Initializes a new, empty world.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(WorldInner {
                model: Model::default(),
                loaded_files: HashSet::new(),
                plugins: Vec::new(),
                parse_error_bundles: HashSet::new(),
                bundle_files: HashMap::new(),
                replaced_by_graph: HashMap::new(),
                ports_cache: HashMap::new(),
                parse_cache: HashMap::new(),
                file_counter: 0,
                filter_lang: true,
                lang: default_language(),
                lv2_path: None,
                dyn_manifest_enabled: false,
            })),
        }
    }

    /// Loads a new world with all the installed LV2 bundles on the system.
    #[must_use]
    pub fn with_load_all() -> World {
        let world = World::new();
        world.load_all();
        world
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, WorldInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// An iterable over all the plugins in the world.
    #[must_use]
    pub fn plugins(&self) -> Plugins {
        Plugins {
            world: self.clone(),
        }
    }

    /// The root LV2 plugin class.
    #[must_use]
    pub fn plugin_class(&self) -> Class {
        Class::new(
            self.clone(),
            uris::LV2_PLUGIN.to_string(),
            "Plugin".to_string(),
        )
    }

    /// All known plugin classes.
    #[must_use]
    pub fn plugin_classes(&self) -> PluginClasses {
        let inner = self.lock();
        let root = Node::uri(uris::LV2_PLUGIN);
        let mut uris = vec![uris::LV2_PLUGIN.to_string()];
        for triple in inner.model.find(None, Some(uris::RDFS_SUB_CLASS_OF), None) {
            let Some(uri) = triple.subject.as_uri() else {
                continue;
            };
            if is_subclass_of(&inner.model, &Node::uri(uri), &root)
                && !uris.iter().any(|u| u == uri)
            {
                uris.push(uri.to_string());
            }
        }
        uris.sort();
        PluginClasses::new(self.clone(), uris)
    }

    /// Creates a new URI value.
    #[must_use]
    pub fn new_uri(&self, uri: &str) -> Node {
        Node::uri(uri)
    }

    /// Creates a new string value (with no language).
    #[must_use]
    pub fn new_string(&self, string: &str) -> Node {
        Node::string(string)
    }

    /// Creates a new string value with a language tag.
    #[must_use]
    pub fn new_string_lang(&self, string: &str, lang: &str) -> Node {
        Node::string_lang(string, lang)
    }

    /// Enables or disables Lilv-style language filtering for returned string values.
    pub fn set_filter_lang(&self, enabled: bool) {
        self.lock().filter_lang = enabled;
    }

    /// Sets the preferred language for filtered string values, for example `de_DE.UTF-8`.
    pub fn set_lang(&self, lang: impl AsRef<str>) {
        self.lock().lang = normalize_language(lang.as_ref());
    }

    /// Returns the current normalized language used for filtering string values.
    #[must_use]
    pub fn lang(&self) -> Option<String> {
        self.lock().lang.clone()
    }

    /// Sets the LV2 bundle search path, overriding the `LV2_PATH` environment variable.
    pub fn set_lv2_path(&self, paths: impl IntoIterator<Item = impl Into<PathBuf>>) {
        self.lock().lv2_path = Some(paths.into_iter().map(Into::into).collect());
    }

    /// Clears the explicit LV2 path override so discovery uses `LV2_PATH` or defaults.
    pub fn clear_lv2_path(&self) {
        self.lock().lv2_path = None;
    }

    /// Enables or disables LV2 dynamic manifests.
    ///
    /// Dynamic manifests execute code from the bundle during discovery, so they
    /// are disabled by default. Enable this only for trusted plugin paths.
    pub fn set_dyn_manifest_enabled(&self, enabled: bool) {
        self.lock().dyn_manifest_enabled = enabled;
    }

    /// Returns whether LV2 dynamic manifests are enabled for discovery.
    #[must_use]
    pub fn dyn_manifest_enabled(&self) -> bool {
        self.lock().dyn_manifest_enabled
    }

    fn search_path(&self) -> Vec<PathBuf> {
        self.lock().lv2_path.clone().unwrap_or_else(lv2_search_path)
    }

    /// Returns true if `subject` declares `rdfs:range` equal to `range`.
    #[must_use]
    pub fn has_range(&self, subject: &Node, range: &Node) -> bool {
        let inner = self.lock();
        inner.model.ask(subject, uris::RDFS_RANGE, range)
    }

    /// Returns an LV2 symbol for a resource, using explicit `lv2:symbol` or the URI suffix.
    #[must_use]
    pub fn symbol(&self, subject: &Node) -> Option<Node> {
        let inner = self.lock();
        if let Some(symbol) = inner.model.first(subject, uris::LV2_SYMBOL) {
            return Some(symbol);
        }
        let uri = subject.as_uri()?;
        let suffix = uri.rsplit(['#', '/', ':']).find(|part| !part.is_empty())?;
        if uris::is_symbol(suffix) {
            Some(Node::string(suffix))
        } else {
            None
        }
    }

    /// Loads all installed LV2 bundles on the system.
    ///
    /// Bundles are discovered in the directories listed in the `LV2_PATH`
    /// environment variable, or in the default per-OS search path when it is
    /// not set.
    pub fn load_all(&self) {
        for dir in self.search_path() {
            self.load_directory(&dir);
        }
        self.load_specifications();
        self.load_plugin_classes();
    }

    /// Loads data files for specification resources found in loaded manifests.
    pub fn load_specifications(&self) {
        let specs = {
            let inner = self.lock();
            inner
                .model
                .find(
                    None,
                    Some(uris::RDF_TYPE),
                    Some(&Node::uri(uris::LV2_SPECIFICATION)),
                )
                .into_iter()
                .map(|triple| triple.subject)
                .collect::<Vec<_>>()
        };
        for spec in specs {
            let _ = self.load_resource(&spec);
        }
    }

    /// Loads LV2 plugin class metadata from loaded specification data.
    pub fn load_plugin_classes(&self) {
        self.load_specifications();
    }

    /// Finds all objects matching `(subject, predicate, object)`. `None` is a wildcard.
    #[must_use]
    pub fn find_nodes(
        &self,
        subject: Option<&Node>,
        predicate: Option<&Node>,
        object: Option<&Node>,
    ) -> Nodes {
        let predicate = predicate.and_then(Node::as_uri);
        let inner = self.lock();
        let mut nodes = Vec::new();
        for triple in inner.model.find(subject, predicate, object) {
            if !nodes.contains(&triple.object) {
                nodes.push(triple.object);
            }
        }
        Nodes::new(inner.filter_nodes(nodes))
    }

    /// Returns the first object matching `(subject, predicate, object)`.
    #[must_use]
    pub fn get(
        &self,
        subject: Option<&Node>,
        predicate: Option<&Node>,
        object: Option<&Node>,
    ) -> Option<Node> {
        self.find_nodes(subject, predicate, object).first()
    }

    /// Returns true if any triple matches `(subject, predicate, object)`.
    #[must_use]
    pub fn ask(
        &self,
        subject: Option<&Node>,
        predicate: Option<&Node>,
        object: Option<&Node>,
    ) -> bool {
        !self.find_nodes(subject, predicate, object).is_empty()
    }

    /// Loads all bundles found directly inside `dir`.
    fn load_directory(&self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(mut path_str) = path.to_str().map(str::to_string) else {
                continue;
            };
            if !path_str.ends_with('/') {
                path_str.push('/');
            }
            let bundle_uri = uris::path_to_file_uri(&path_str);
            self.load_bundle(&Node::uri(bundle_uri));
        }
    }

    /// Loads a specific bundle. `bundle_uri` must be a fully qualified URI to
    /// the bundle directory, with the trailing slash, eg
    /// `file:///usr/lib/lv2/foo.lv2/`.
    pub fn load_bundle(&self, bundle_uri: &Node) {
        let Some(bundle_uri) = bundle_uri.as_uri() else {
            return;
        };
        let mut bundle_uri = bundle_uri.to_string();
        if !bundle_uri.ends_with('/') {
            bundle_uri.push('/');
        }
        let mut inner = self.lock();
        inner.load_bundle(&bundle_uri);
    }

    /// Unloads triples and plugin records loaded from a bundle.
    pub fn unload_bundle(&self, bundle_uri: &Node) -> Result<(), String> {
        let Some(bundle_uri) = bundle_uri.as_uri() else {
            return Err("Bundle URI must be a URI node".to_string());
        };
        let mut bundle_uri = bundle_uri.to_string();
        if !bundle_uri.ends_with('/') {
            bundle_uri.push('/');
        }
        let mut inner = self.lock();
        inner.unload_bundle(&bundle_uri);
        Ok(())
    }

    /// Loads the `rdfs:seeAlso` files for a subject already present in the model.
    pub fn load_resource(&self, resource: &Node) -> Result<usize, String> {
        if !resource.is_uri() && !resource.is_blank() {
            return Err("Resource must be a URI or blank node".to_string());
        }
        let files = {
            let inner = self.lock();
            inner
                .model
                .objects(resource, uris::RDFS_SEE_ALSO)
                .into_iter()
                .map(|node| {
                    node.as_uri()
                        .map(str::to_string)
                        .ok_or_else(|| "rdfs:seeAlso value must be a URI".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut inner = self.lock();
        let mut loaded = 0;
        for file in files {
            if inner.load_resource_file(&file)? {
                loaded += 1;
            }
        }
        Ok(loaded)
    }

    /// Unloads files loaded by `load_resource`.
    pub fn unload_resource(&self, resource: &Node) -> Result<usize, String> {
        if !resource.is_uri() && !resource.is_blank() {
            return Err("Resource must be a URI or blank node".to_string());
        }
        let files = {
            let inner = self.lock();
            inner
                .model
                .objects(resource, uris::RDFS_SEE_ALSO)
                .into_iter()
                .map(|node| {
                    node.as_uri()
                        .map(str::to_string)
                        .ok_or_else(|| "rdfs:seeAlso value must be a URI".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut inner = self.lock();
        let mut dropped = 0;
        for file in files {
            if inner.loaded_files.remove(&file) {
                inner.model.remove_graph(&file);
                inner.remove_replacement_graph(&file);
                dropped += 1;
            }
        }
        if dropped > 0 {
            inner.ports_cache.clear();
        }
        Ok(dropped)
    }
}

fn default_language() -> Option<String> {
    std::env::var("LANG")
        .ok()
        .and_then(|lang| normalize_language(&lang))
}

fn normalize_language(lang: &str) -> Option<String> {
    let lang = lang.split('.').next().unwrap_or(lang);
    let lang = lang.split('@').next().unwrap_or(lang);
    if lang.eq_ignore_ascii_case("c") || lang.eq_ignore_ascii_case("posix") {
        return None;
    }
    let lang = lang.replace('_', "-").to_ascii_lowercase();
    if lang
        .split('-')
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphabetic()))
    {
        Some(lang)
    } else {
        None
    }
}

impl Default for World {
    /// Return a new empty world.
    fn default() -> World {
        World::new()
    }
}

/// The result of parsing one Turtle file: the triples read so far and
/// whether a syntax error was encountered (parsing stops at the first
/// error, like lilv's reader, keeping whatever was read before it).
struct ParsedFile {
    triples: Vec<Triple>,
    had_error: bool,
}

#[repr(C)]
struct CFile {
    _private: [u8; 0],
}

type Lv2DynManifestHandle = *mut c_void;
type Lv2DynManifestOpen = unsafe extern "C" fn(
    handle: *mut Lv2DynManifestHandle,
    features: *const *const LV2Feature,
) -> c_int;
type Lv2DynManifestGetSubjects =
    unsafe extern "C" fn(handle: Lv2DynManifestHandle, fp: *mut CFile) -> c_int;
type Lv2DynManifestGetData =
    unsafe extern "C" fn(handle: Lv2DynManifestHandle, fp: *mut CFile, uri: *const c_char) -> c_int;
type Lv2DynManifestClose = unsafe extern "C" fn(handle: Lv2DynManifestHandle);

unsafe extern "C" {
    fn fopen(path: *const c_char, mode: *const c_char) -> *mut CFile;
    fn fflush(stream: *mut CFile) -> c_int;
    fn fclose(stream: *mut CFile) -> c_int;
}

/// Reads `(minorVersion, microVersion)` of `resource` from `triples` in a
/// single pass.
fn read_version(triples: &[Triple], resource: &Node) -> (u32, u32) {
    let mut minor = None;
    let mut micro = None;
    for triple in triples {
        if triple.subject != *resource {
            continue;
        }
        let target = if triple.predicate == uris::LV2_MINOR_VERSION {
            &mut minor
        } else if triple.predicate == uris::LV2_MICRO_VERSION {
            &mut micro
        } else {
            continue;
        };
        *target = triple
            .object
            .as_literal_parts()
            .and_then(|(value, _, _)| value.parse::<u32>().ok());
        if minor.is_some() && micro.is_some() {
            break;
        }
    }
    (minor.unwrap_or(0), micro.unwrap_or(0))
}

fn parse_turtle_uncached(uri: &str, file_id: usize) -> Option<ParsedFile> {
    let path = uris::file_uri_to_path(uri)?;
    let data = std::fs::read(&path).ok()?;
    parse_turtle_bytes(uri, &data, file_id)
}

fn parse_turtle_bytes(uri: &str, data: &[u8], file_id: usize) -> Option<ParsedFile> {
    let parser = match TurtleParser::new().with_base_iri(uri) {
        Ok(parser) => parser,
        Err(_) => return None,
    };

    let blank_prefix = format!("{file_id}:");

    let mut triples = Vec::new();
    let mut had_error = false;
    for result in parser.for_slice(data) {
        match result {
            Ok(triple) => {
                let subject = match triple.subject {
                    oxrdf::NamedOrBlankNode::NamedNode(node) => Node::uri(node.into_string()),
                    oxrdf::NamedOrBlankNode::BlankNode(node) => {
                        Node::blank(format!("{}{}", blank_prefix, node.as_str()))
                    }
                };
                let object = match triple.object {
                    oxrdf::Term::NamedNode(node) => Node::uri(node.into_string()),
                    oxrdf::Term::BlankNode(node) => {
                        Node::blank(format!("{}{}", blank_prefix, node.as_str()))
                    }
                    oxrdf::Term::Literal(literal) => {
                        let (value, datatype, language) = literal.destruct();
                        Node::literal(value, datatype.map(|d| d.into_string()), language)
                    }
                };
                triples.push(Triple {
                    subject,
                    predicate: triple.predicate.into_string(),
                    object,
                });
            }
            Err(error) => {
                lv2_log!("maolan_lv2_world_load_file(): error: Error loading <{uri}> ({error})");
                had_error = true;
                break;
            }
        }
    }
    Some(ParsedFile { triples, had_error })
}

impl WorldInner {
    pub(crate) fn objects(&self, subject: &Node, predicate: &str) -> Vec<Node> {
        self.filter_nodes(self.model.objects(subject, predicate))
    }

    pub(crate) fn first(&self, subject: &Node, predicate: &str) -> Option<Node> {
        self.objects(subject, predicate).into_iter().next()
    }

    pub(crate) fn filter_nodes(&self, nodes: Vec<Node>) -> Vec<Node> {
        if !self.filter_lang || nodes.iter().all(|node| node.language().is_none()) {
            return nodes;
        }

        let Some(lang) = self.lang.as_deref() else {
            return nodes
                .into_iter()
                .filter(|node| node.language().is_none())
                .collect();
        };
        let primary = lang.split('-').next().unwrap_or(lang);
        let mut plain = Vec::new();
        let mut exact = Vec::new();
        let mut partial = Vec::new();
        let mut other = Vec::new();

        for node in nodes {
            let Some(node_lang) = node.language().and_then(normalize_language) else {
                if node.is_string() {
                    plain.push(node);
                } else {
                    other.push(node);
                }
                continue;
            };
            if node_lang == lang {
                exact.push(node);
            } else if node_lang.split('-').next() == Some(primary) {
                partial.push(node);
            }
        }

        if !exact.is_empty() {
            other.extend(exact);
        } else if !partial.is_empty() {
            other.extend(partial);
        } else {
            other.extend(plain);
        }
        other
    }

    /// Parses the Turtle file at `uri` (a `file://` URI). Relative IRIs are
    /// resolved against the file's own URI. Blank node identifiers are
    /// namespaced with a per-file prefix so identical labels from different
    /// files do not collide. Results are cached: a file is parsed at most
    /// once per world.
    fn parse_turtle(&mut self, uri: &str) -> Option<Arc<ParsedFile>> {
        if let Some(cached) = self.parse_cache.get(uri) {
            return Some(cached.clone());
        }
        self.file_counter += 1;
        let parsed = Arc::new(parse_turtle_uncached(uri, self.file_counter)?);
        self.parse_cache.insert(uri.to_string(), parsed.clone());
        Some(parsed)
    }

    fn parse_turtle_batch_parallel(
        &mut self,
        files: Vec<String>,
    ) -> Vec<(String, Option<Arc<ParsedFile>>)> {
        let mut results = Vec::with_capacity(files.len());
        let mut to_parse = Vec::new();

        for file in files {
            if let Some(cached) = self.parse_cache.get(&file) {
                results.push((file, Some(cached.clone())));
            } else {
                self.file_counter += 1;
                to_parse.push((file, self.file_counter));
            }
        }

        if !to_parse.is_empty() {
            let workers = std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .min(to_parse.len());
            let next = AtomicUsize::new(0);
            let parsed_results = std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(workers);
                for _ in 0..workers {
                    let to_parse = &to_parse;
                    let next = &next;
                    handles.push(scope.spawn(move || {
                        let mut local = Vec::new();
                        loop {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            let Some((file, file_id)) = to_parse.get(index) else {
                                break;
                            };
                            let parsed = parse_turtle_uncached(file, *file_id).map(Arc::new);
                            local.push((file.clone(), parsed));
                        }
                        local
                    }));
                }

                let mut parsed_results = Vec::with_capacity(to_parse.len());
                for handle in handles {
                    parsed_results.extend(handle.join().unwrap_or_default());
                }
                parsed_results
            });

            for (file, parsed) in parsed_results {
                if let Some(parsed) = &parsed {
                    self.parse_cache.insert(file.clone(), parsed.clone());
                }
                results.push((file, parsed));
            }
        }

        results.sort_by(|(left, _), (right, _)| left.cmp(right));
        results
    }

    fn load_files_recursive_parallel(
        &mut self,
        files: Vec<String>,
        bundle_uri: &str,
        error_bundles: &mut Vec<String>,
    ) {
        let mut pending = files;
        while !pending.is_empty() {
            let mut batch = Vec::new();
            for file in pending {
                if self.loaded_files.contains(&file) {
                    continue;
                }
                if !file.starts_with("file:") || !file.ends_with(".ttl") {
                    continue;
                }
                self.loaded_files.insert(file.clone());
                batch.push(file);
            }

            if batch.is_empty() {
                break;
            }

            let mut next = Vec::new();
            for (file, parsed) in self.parse_turtle_batch_parallel(batch) {
                let Some(parsed) = parsed else {
                    lv2_log!("maolan_lv2_world_load_file(): error: Error loading file <{file}>");
                    error_bundles.push(bundle_uri.to_string());
                    continue;
                };
                self.bundle_files
                    .entry(bundle_uri.to_string())
                    .or_default()
                    .insert(file.clone());
                if parsed.had_error {
                    error_bundles.push(bundle_uri.to_string());
                }
                for triple in &parsed.triples {
                    if triple.predicate == uris::RDFS_SEE_ALSO
                        && let Some(file) = triple.object.as_uri()
                    {
                        next.push(file.to_string());
                    }
                    self.insert_triple(&file, triple);
                }
            }
            pending = next;
        }
    }

    /// Reads `(minorVersion, microVersion)` of `resource`, parsing the
    /// resource's `rdfs:seeAlso` files when `triples` (the manifest) carries
    /// no version (mirrors `load_version` in lilv's world.c).
    fn skim_version(&mut self, triples: &[Triple], resource: &Node) -> (u32, u32) {
        let version = read_version(triples, resource);
        if version != (0, 0) {
            return version;
        }
        let see_also: Vec<String> = triples
            .iter()
            .filter(|t| t.subject == *resource && t.predicate == uris::RDFS_SEE_ALSO)
            .filter_map(|t| t.object.as_uri().map(str::to_string))
            .collect();
        for file in see_also {
            if let Some(parsed) = self.parse_turtle(&file) {
                let version = read_version(&parsed.triples, resource);
                if version != (0, 0) {
                    return version;
                }
            }
        }
        (0, 0)
    }

    fn load_dynamic_manifest(
        &mut self,
        bundle_uri: &str,
        manifest_uri: &str,
        manifest_triples: &[Triple],
    ) -> Vec<Triple> {
        let generators: Vec<Node> = manifest_triples
            .iter()
            .filter(|t| {
                t.predicate == uris::RDF_TYPE
                    && t.object.as_uri() == Some(uris::DYNMANIFEST_DYN_MANIFEST)
            })
            .map(|t| t.subject.clone())
            .collect();
        let mut triples = Vec::new();
        for generator in generators {
            let Some(binary_uri) = manifest_triples
                .iter()
                .find(|t| t.subject == generator && t.predicate == uris::LV2_BINARY)
                .and_then(|t| t.object.as_uri())
            else {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Dynamic manifest in <{bundle_uri}> has no lv2:binary"
                );
                continue;
            };
            let Some(binary_path) = uris::file_uri_to_path(binary_uri) else {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Dynamic manifest binary <{binary_uri}> is not a file URI"
                );
                continue;
            };
            let Ok(library) = (unsafe { libloading::Library::new(&binary_path) }) else {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Failed to open dynamic manifest binary <{binary_uri}>"
                );
                continue;
            };

            let Ok(open) = (unsafe { library.get::<Lv2DynManifestOpen>(b"lv2_dyn_manifest_open") })
            else {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Dynamic manifest binary <{binary_uri}> has no lv2_dyn_manifest_open"
                );
                continue;
            };
            let Ok(get_subjects) = (unsafe {
                library.get::<Lv2DynManifestGetSubjects>(b"lv2_dyn_manifest_get_subjects")
            }) else {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Dynamic manifest binary <{binary_uri}> has no lv2_dyn_manifest_get_subjects"
                );
                continue;
            };
            let Ok(get_data) =
                (unsafe { library.get::<Lv2DynManifestGetData>(b"lv2_dyn_manifest_get_data") })
            else {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Dynamic manifest binary <{binary_uri}> has no lv2_dyn_manifest_get_data"
                );
                continue;
            };
            let Ok(close) =
                (unsafe { library.get::<Lv2DynManifestClose>(b"lv2_dyn_manifest_close") })
            else {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Dynamic manifest binary <{binary_uri}> has no lv2_dyn_manifest_close"
                );
                continue;
            };

            let mut handle = std::ptr::null_mut();
            let features = [std::ptr::null()];
            let open_status = unsafe { open(&mut handle, features.as_ptr()) };
            if open_status != 0 {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Opening dynamic manifest <{binary_uri}> failed with status {open_status}"
                );
                continue;
            }

            let subject_data = self.dynamic_manifest_output(bundle_uri, "subjects", |fp| unsafe {
                get_subjects(handle, fp)
            });
            let Some(subject_data) = subject_data else {
                unsafe { close(handle) };
                continue;
            };
            self.file_counter += 1;
            let Some(subjects) = parse_turtle_bytes(manifest_uri, &subject_data, self.file_counter)
            else {
                unsafe { close(handle) };
                continue;
            };
            if subjects.had_error {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Error parsing subjects for <{binary_uri}>"
                );
            }

            let data_subjects: Vec<String> = subjects
                .triples
                .iter()
                .filter(|t| t.predicate == uris::RDF_TYPE)
                .filter_map(|t| t.subject.as_uri().map(str::to_string))
                .collect();
            triples.extend(subjects.triples);

            for subject in data_subjects {
                let Ok(subject_c) = CString::new(subject.as_str()) else {
                    continue;
                };
                let Some(data) = self.dynamic_manifest_output(bundle_uri, "data", |fp| unsafe {
                    get_data(handle, fp, subject_c.as_ptr())
                }) else {
                    continue;
                };
                self.file_counter += 1;
                let Some(parsed) = parse_turtle_bytes(manifest_uri, &data, self.file_counter)
                else {
                    continue;
                };
                if parsed.had_error {
                    lv2_log!(
                        "maolan_lv2_world_load_dyn_manifest(): warning: Error parsing data for <{subject}>"
                    );
                }
                triples.extend(parsed.triples);
            }

            unsafe { close(handle) };
        }
        triples
    }

    fn dynamic_manifest_output<F>(
        &mut self,
        bundle_uri: &str,
        name: &str,
        write: F,
    ) -> Option<Vec<u8>>
    where
        F: FnOnce(*mut CFile) -> c_int,
    {
        let path = std::env::temp_dir().join(format!(
            "maolan-lv2-dynmanifest-{}-{}-{name}.ttl",
            std::process::id(),
            self.file_counter
        ));
        let path_c = CString::new(path.to_string_lossy().as_bytes()).ok()?;
        let mode = CString::new("w+").ok()?;
        let fp = unsafe { fopen(path_c.as_ptr(), mode.as_ptr()) };
        if fp.is_null() {
            lv2_log!(
                "maolan_lv2_world_load_dyn_manifest(): warning: Could not create temporary output for <{bundle_uri}>"
            );
            return None;
        }
        let status = write(fp);
        unsafe {
            fflush(fp);
            fclose(fp);
        }
        if status != 0 {
            let _ = std::fs::remove_file(&path);
            lv2_log!(
                "maolan_lv2_world_load_dyn_manifest(): warning: Dynamic manifest write for <{bundle_uri}> failed with status {status}"
            );
            return None;
        }
        let data = std::fs::read(&path).ok();
        let _ = std::fs::remove_file(path);
        data
    }

    /// Loads the bundle at `bundle_uri` (a `file://` URI with a trailing
    /// slash). Mirrors `lilv_world_load_bundle`.
    fn load_bundle(&mut self, bundle_uri: &str) {
        let manifest_uri = format!("{bundle_uri}manifest.ttl");
        if self.loaded_files.contains(&manifest_uri) {
            return; // Bundle already loaded
        }
        let Some(parsed) = self.parse_turtle(&manifest_uri) else {
            return; // No readable manifest: not a bundle, skip silently
        };
        self.loaded_files.insert(manifest_uri.clone());
        // Loading a bundle mutates the model: cached port tables may be stale.
        self.ports_cache.clear();
        let mut local = parsed.triples.clone();
        let mut error_bundles = Vec::new();
        if parsed.had_error {
            error_bundles.push(bundle_uri.to_string());
        }

        let has_dynmanifest = local.iter().any(|t| {
            t.predicate == uris::RDF_TYPE
                && t.object.as_uri() == Some(uris::DYNMANIFEST_DYN_MANIFEST)
        });
        if has_dynmanifest {
            if self.dyn_manifest_enabled {
                let dynamic_triples = self.load_dynamic_manifest(bundle_uri, &manifest_uri, &local);
                local.extend(dynamic_triples);
            } else {
                lv2_log!(
                    "maolan_lv2_world_load_dyn_manifest(): warning: Skipping dynamic manifest in <{bundle_uri}> (disabled)"
                );
            }
        }

        // Plugin subjects: typed lv2:Plugin with an lv2:binary.
        let plugin_type = Node::uri(uris::LV2_PLUGIN);
        let candidates: Vec<Node> = local
            .iter()
            .filter(|t| t.predicate == uris::RDF_TYPE && t.object == plugin_type)
            .map(|t| t.subject.clone())
            .filter(|subject| {
                local
                    .iter()
                    .any(|t| t.subject == *subject && t.predicate == uris::LV2_BINARY)
            })
            .collect();

        let candidate_see_also: Vec<String> = local
            .iter()
            .filter(|t| t.predicate == uris::RDFS_SEE_ALSO && candidates.contains(&t.subject))
            .filter_map(|t| t.object.as_uri().map(str::to_string))
            .collect();
        let _ = self.parse_turtle_batch_parallel(candidate_see_also);

        // Versions of all candidates, computed once: used for duplicate
        // resolution below and for the plugin records at the end.
        let mut versions: HashMap<Node, (u32, u32)> = HashMap::new();
        for candidate in &candidates {
            let version = self.skim_version(&local, candidate);
            versions.insert(candidate.clone(), version);
        }

        // Resolve duplicate plugins: keep the newest version
        // (lilv_world_compare_versions).
        let mut replacements: Vec<(Node, String, u32, u32)> = Vec::new();
        for candidate in &candidates {
            let Some(uri) = candidate.as_uri() else {
                continue;
            };
            let existing = self
                .plugins
                .iter()
                .find(|p| p.uri == uri)
                .map(|p| (p.bundle_uri.clone(), p.minor_version, p.micro_version));
            let Some((existing_bundle, old_minor, old_micro)) = existing else {
                continue;
            };
            if existing_bundle == bundle_uri {
                continue;
            }
            let (new_minor, new_micro) = versions.get(candidate).copied().unwrap_or((0, 0));
            // lilv_version_cmp: lexicographic (minor, micro) comparison.
            if (new_minor, new_micro) > (old_minor, old_micro) {
                lv2_log!(
                    "maolan_lv2_world_compare_versions(): warning: Loading new version {new_minor}.{new_micro} of <{uri}> from <{bundle_uri}>"
                );
                lv2_log!(
                    "maolan_lv2_world_compare_versions(): note: Replaces version {old_minor}.{old_micro} from <{existing_bundle}>"
                );
                replacements.push((candidate.clone(), existing_bundle, new_minor, new_micro));
            } else if (new_minor, new_micro) < (old_minor, old_micro) {
                // Older version: ignore this bundle entirely.
                lv2_log!(
                    "maolan_lv2_world_compare_versions(): warning: Ignoring version {new_minor}.{new_micro} of <{uri}> from <{bundle_uri}>"
                );
                lv2_log!(
                    "maolan_lv2_world_compare_versions(): note: Newer version {old_minor}.{old_micro} loaded from <{existing_bundle}>"
                );
                return;
            } else {
                // Duplicate version: first loaded wins.
                lv2_log!(
                    "maolan_lv2_world_compare_versions(): warning: Ignoring duplicate version {new_minor}.{new_micro} of <{uri}> from <{bundle_uri}>"
                );
                lv2_log!(
                    "maolan_lv2_world_compare_versions(): note: Previously loaded from <{existing_bundle}>"
                );
                return;
            }
        }

        // Drop the replaced bundles' data and plugin records.
        for (candidate, old_bundle, _, _) in &replacements {
            self.unload_bundle(old_bundle);
            if let Some(uri) = candidate.as_uri() {
                self.plugins.retain(|p| p.uri != uri);
            }
        }

        // Merge the manifest and all seeAlso data into the model.
        for triple in &local {
            self.insert_triple(bundle_uri, triple);
        }
        let see_also: Vec<String> = local
            .iter()
            .filter(|t| t.predicate == uris::RDFS_SEE_ALSO)
            .filter_map(|t| t.object.as_uri().map(str::to_string))
            .collect();
        self.load_files_recursive_parallel(see_also, bundle_uri, &mut error_bundles);
        for bundle in error_bundles {
            self.parse_error_bundles.insert(bundle);
        }

        // Register the discovered plugins.
        for candidate in candidates {
            let Some(uri) = candidate.as_uri().map(str::to_string) else {
                continue;
            };
            if self.plugins.iter().any(|p| p.uri == uri) {
                continue;
            }
            let binary_uri = local
                .iter()
                .find(|t| t.subject == candidate && t.predicate == uris::LV2_BINARY)
                .and_then(|t| t.object.as_uri().map(str::to_string));
            let (minor_version, micro_version) =
                versions.get(&candidate).copied().unwrap_or((0, 0));
            self.plugins.push(PluginData {
                uri,
                bundle_uri: bundle_uri.to_string(),
                binary_uri,
                minor_version,
                micro_version,
            });
        }
        self.plugins.sort_by(|a, b| a.uri.cmp(&b.uri));
    }

    fn load_resource_file(&mut self, file: &str) -> Result<bool, String> {
        if self.loaded_files.contains(file) {
            return Ok(false);
        }
        if !file.starts_with("file:") || !file.ends_with(".ttl") {
            return Err(format!("Resource is not a Turtle file URI: {file}"));
        }
        let Some(parsed) = self.parse_turtle(file) else {
            return Err(format!("Error loading file <{file}>"));
        };
        self.loaded_files.insert(file.to_string());
        self.ports_cache.clear();
        for triple in &parsed.triples {
            self.insert_triple(file, triple);
        }
        if parsed.had_error {
            Err(format!("Error loading file <{file}>"))
        } else {
            Ok(true)
        }
    }

    fn insert_triple(&mut self, graph: &str, triple: &Triple) {
        if triple.predicate == uris::DCT_REPLACES
            && let Some(replaced) = triple.object.as_uri()
        {
            self.replaced_by_graph
                .entry(graph.to_string())
                .or_default()
                .insert(replaced.to_string());
        }
        self.model.insert(graph, triple);
    }

    fn remove_replacement_graph(&mut self, graph: &str) {
        self.replaced_by_graph.remove(graph);
    }

    pub(crate) fn is_replaced(&self, uri: &str) -> bool {
        self.replaced_by_graph
            .values()
            .any(|replaced| replaced.contains(uri))
    }

    pub(crate) fn plugin_data(&self, uri: &str) -> Option<&PluginData> {
        self.plugins.iter().find(|p| p.uri == uri)
    }

    pub(crate) fn bundle_has_parse_errors(&self, bundle_uri: &str) -> bool {
        self.parse_error_bundles.contains(bundle_uri)
    }

    fn unload_bundle(&mut self, bundle_uri: &str) {
        self.model.remove_graph(bundle_uri);
        self.remove_replacement_graph(bundle_uri);
        let manifest_uri = format!("{bundle_uri}manifest.ttl");
        self.loaded_files.remove(&manifest_uri);
        if let Some(files) = self.bundle_files.remove(bundle_uri) {
            for file in files {
                self.model.remove_graph(&file);
                self.remove_replacement_graph(&file);
                self.loaded_files.remove(&file);
            }
        }
        self.plugins
            .retain(|plugin| plugin.bundle_uri != bundle_uri);
        self.parse_error_bundles.remove(bundle_uri);
        self.ports_cache.clear();
    }
}

fn is_subclass_of(model: &Model, child: &Node, parent: &Node) -> bool {
    if child == parent {
        return true;
    }
    let mut stack = model.objects(child, uris::RDFS_SUB_CLASS_OF);
    let mut seen = HashSet::new();
    while let Some(node) = stack.pop() {
        if &node == parent {
            return true;
        }
        if !seen.insert(node.clone()) {
            continue;
        }
        stack.extend(model.objects(&node, uris::RDFS_SUB_CLASS_OF));
    }
    false
}

/// Returns the LV2 bundle search path: the directories of `LV2_PATH`, or the
/// default per-OS path when the variable is unset (see lilv's meson.build).
fn lv2_search_path() -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os("LV2_PATH") {
        // std::env::split_paths uses ':' on unix and ';' on Windows, which
        // matches lilv's LILV_PATH_SEP.
        return std::env::split_paths(&path)
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
    }

    let home = dirs::home_dir();
    #[cfg(target_os = "macos")]
    {
        let mut dirs = Vec::new();
        if let Some(home) = &home {
            dirs.push(home.join(".lv2"));
            dirs.push(home.join("Library/Audio/Plug-Ins/LV2"));
        }
        dirs.push(PathBuf::from("/usr/local/lib/lv2"));
        dirs.push(PathBuf::from("/usr/lib/lv2"));
        dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/LV2"));
        dirs
    }
    #[cfg(target_os = "windows")]
    {
        let mut dirs = Vec::new();
        if let Some(appdata) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("LV2"));
        }
        if let Some(common) = std::env::var_os("COMMONPROGRAMFILES") {
            dirs.push(PathBuf::from(common).join("LV2"));
        }
        let _ = home;
        dirs
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut dirs = Vec::new();
        if let Some(home) = &home {
            dirs.push(home.join(".lv2"));
        }
        dirs.push(PathBuf::from("/usr/local/lib/lv2"));
        dirs.push(PathBuf::from("/usr/lib/lv2"));
        dirs
    }
}
