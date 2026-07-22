//! Integration test with a synthetic bundle built in a temporary directory.

mod common;

use common::{TempDir, bundle_uri, write_file};
use maolan_lv2::World;
use maolan_lv2::node::Node;
use std::path::Path;
use std::process::Command;

const PLUGIN_URI: &str = "http://example.org/plugins/synth";
const CLASS_URI: &str = "http://example.org/classes/Synth";

fn manifest(version_minor: u32, version_micro: u32) -> String {
    format!(
        r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<{PLUGIN_URI}>
    a lv2:Plugin ;
    lv2:binary <synth.so> ;
    lv2:minorVersion {version_minor} ;
    lv2:microVersion {version_micro} ;
    rdfs:seeAlso <plugin.ttl> .
"#
    )
}

fn plugin_ttl(port_symbol: &str) -> String {
    format!(
        r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix atom: <http://lv2plug.in/ns/ext/atom#> .
@prefix midi: <http://lv2plug.in/ns/ext/midi#> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .
@prefix pset: <http://lv2plug.in/ns/ext/presets#> .

<{CLASS_URI}> a rdfs:Class ;
    rdfs:subClassOf lv2:Plugin ;
    rdfs:label "Synthetic Plugin" .

<http://example.org/project>
    doap:maintainer <http://example.org/developer> .

<http://example.org/developer>
    foaf:name "Example Author" ;
    foaf:mbox <mailto:author@example.org> ;
    foaf:homepage <http://example.org/> .

<http://example.org/ui>
    a <http://lv2plug.in/ns/extensions/ui#X11UI> ;
    lv2:binary <ui.so> ;
    lv2:extensionData <http://example.org/ui-ext> .

<http://example.org/presets/default>
    a pset:Preset ;
    rdfs:label "Default" ;
    lv2:appliesTo <{PLUGIN_URI}> ;
    lv2:port [
        lv2:symbol "gain" ;
        pset:value 0.25
    ] .

<{PLUGIN_URI}>
    a lv2:Plugin , <{CLASS_URI}> ;
    doap:name "Synthetic Test Plugin" , "Deutscher Synth"@de-de , "Sintetizador"@es ;
    lv2:project <http://example.org/project> ;
    lv2:requiredFeature <http://lv2plug.in/ns/ext/urid#map> ;
    lv2:optionalFeature lv2:hardRTCapable ;
    lv2:extensionData <http://lv2plug.in/ns/ext/state#interface> ;
    <http://lv2plug.in/ns/extensions/ui#ui> <http://example.org/ui> ;
    lv2:port [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 0 ;
        lv2:symbol "{port_symbol}" ;
        lv2:name "Gain" , "Verstaerkung"@de-de , "Ganancia"@es ;
        rdfs:comment "Gain comment" , "Commentaire gain"@fr ;
        <http://lv2plug.in/ns/ext/port-groups#group> <http://example.org/groups/main> ;
        lv2:default 0.5 ;
        lv2:minimum 0.0 ;
        lv2:maximum 1.0 ;
        lv2:portProperty lv2:toggled , lv2:integer , lv2:enumeration , lv2:sampleRate , <http://lv2plug.in/ns/ext/port-props#notOnGUI> , <http://lv2plug.in/ns/ext/port-props#logarithmic> ;
        lv2:scalePoint [
            rdfs:label "half" ;
            <http://www.w3.org/1999/02/22-rdf-syntax-ns#value> 0.5
        ]
    ] , [
        a lv2:InputPort , atom:AtomPort ;
        lv2:index 1 ;
        lv2:symbol "events_in" ;
        lv2:name "Events In" ;
        atom:supports midi:MidiEvent
    ] , [
        a lv2:OutputPort , lv2:AudioPort ;
        lv2:index 2 ;
        lv2:symbol "out" ;
        lv2:name "Out"
    ] , [
        a lv2:OutputPort , lv2:ControlPort ;
        lv2:index 3 ;
        lv2:symbol "latency" ;
        lv2:name "Latency" ;
        lv2:designation lv2:latency
    ] .
"#
    )
}

fn make_bundle(temp: &TempDir, name: &str, minor: u32, micro: u32, symbol: &str) -> String {
    let dir = temp.bundle(name);
    write_file(&dir, "manifest.ttl", &manifest(minor, micro));
    write_file(&dir, "plugin.ttl", &plugin_ttl(symbol));
    bundle_uri(&dir)
}

fn load_world_with_bundle(temp: &TempDir) -> (World, String) {
    let world = World::new();
    let uri = make_bundle(temp, "synth", 2, 0, "gain");
    world.load_bundle(&world.new_uri(&uri));
    (world, uri)
}

#[test]
fn discovery_and_metadata() {
    let temp = TempDir::new("discovery");
    let (world, bundle) = load_world_with_bundle(&temp);

    let plugins = world.plugins();
    assert_eq!(plugins.count(), 1);

    let uri = world.new_uri(PLUGIN_URI);
    let plugin = plugins.plugin(&uri).expect("plugin not found");
    assert!(plugin.verify());
    assert_eq!(plugin.uri().as_uri(), Some(PLUGIN_URI));
    assert_eq!(plugin.bundle_uri().as_uri(), Some(bundle.as_str()));
    assert!(
        plugin
            .library_uri()
            .and_then(|n| n.as_uri().map(str::to_string))
            .is_some_and(|u| u.ends_with("/synth.so"))
    );
    assert_eq!(plugin.name().as_str(), Some("Synthetic Test Plugin"));
    let description = plugin.description_turtle();
    assert!(description.contains(PLUGIN_URI));
    assert!(description.contains("Synthetic Test Plugin"));
    let manifest_entry = plugin.manifest_entry_turtle();
    assert!(manifest_entry.contains(PLUGIN_URI));
    assert!(manifest_entry.contains("synth.so"));
    assert!(manifest_entry.contains("plugin.ttl"));

    let class = plugin.class();
    assert_eq!(class.label().as_str(), Some("Synthetic Plugin"));
    assert_eq!(
        class
            .uri()
            .and_then(|n| n.as_uri().map(str::to_string))
            .as_deref(),
        Some(CLASS_URI)
    );

    // Features and extension data.
    let urid_map = world.new_uri("http://lv2plug.in/ns/ext/urid#map");
    let hard_rt = world.new_uri("http://lv2plug.in/ns/lv2core#hardRTCapable");
    assert!(plugin.required_features().contains(&urid_map));
    assert!(!plugin.required_features().contains(&hard_rt));
    assert!(plugin.optional_features().contains(&hard_rt));
    assert!(plugin.supported_features().contains(&urid_map));
    assert!(plugin.supported_features().contains(&hard_rt));
    assert!(plugin.has_feature(&urid_map));
    let state_iface = world.new_uri("http://lv2plug.in/ns/ext/state#interface");
    let worker_iface = world.new_uri("http://lv2plug.in/ns/ext/worker#interface");
    assert!(plugin.has_extension_data(&state_iface));
    assert!(plugin.extension_data().contains(&state_iface));
    assert!(!plugin.has_extension_data(&worker_iface));
    let uis = plugin.uis();
    assert_eq!(uis.len(), 1);
    let x11 = world.new_uri("http://lv2plug.in/ns/extensions/ui#X11UI");
    assert!(uis[0].is_a(&x11));
    assert!(
        uis[0]
            .extension_data()
            .contains(&world.new_uri("http://example.org/ui-ext"))
    );
    assert_eq!(uis[0].supported_class([&x11]).as_ref(), Some(&x11));
    assert_eq!(
        uis[0].support_quality(&x11, |container, ui_type| if container == ui_type {
            5
        } else {
            0
        }),
        Some((5, x11.clone()))
    );
    assert_eq!(plugin.supported_uis().len(), 1);
    assert_eq!(
        plugin
            .project()
            .and_then(|n| n.as_uri().map(str::to_string))
            .as_deref(),
        Some("http://example.org/project")
    );
    assert_eq!(
        plugin
            .author_name()
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("Example Author")
    );
    assert_eq!(
        plugin
            .author_email()
            .and_then(|n| n.as_uri().map(str::to_string))
            .as_deref(),
        Some("mailto:author@example.org")
    );
    assert_eq!(
        plugin
            .author_homepage()
            .and_then(|n| n.as_uri().map(str::to_string))
            .as_deref(),
        Some("http://example.org/")
    );
    assert!(!plugin.is_replaced());
    assert!(
        plugin
            .data_uris()
            .contains(&world.new_uri(&format!("{}plugin.ttl", bundle)))
    );
    let preset = world.new_uri("http://example.org/presets/default");
    assert!(plugin.presets().contains(&preset));
    assert!(
        plugin
            .related(Some(
                &world.new_uri("http://lv2plug.in/ns/ext/presets#Preset")
            ))
            .contains(&preset)
    );

    // Arbitrary predicate values.
    let values: Vec<Node> = plugin
        .value(&world.new_uri("http://usefulinc.com/ns/doap#name"))
        .iter()
        .collect();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].as_str(), Some("Synthetic Test Plugin"));
}

#[test]
fn language_filtering_matches_lilv_value_queries() {
    let temp = TempDir::new("lang");
    let (world, _) = load_world_with_bundle(&temp);
    let plugin = world
        .plugins()
        .plugin(&world.new_uri(PLUGIN_URI))
        .expect("plugin");
    let gain = plugin.port_by_index(0).expect("gain");

    assert_eq!(plugin.name().as_str(), Some("Synthetic Test Plugin"));
    assert_eq!(
        gain.name()
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("Gain")
    );

    world.set_lang("de_DE.UTF-8");
    assert_eq!(plugin.name().as_str(), Some("Deutscher Synth"));
    assert_eq!(
        gain.name()
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("Verstaerkung")
    );

    world.set_lang("de_CH");
    assert_eq!(plugin.name().as_str(), Some("Deutscher Synth"));

    world.set_lang("es_MX");
    assert_eq!(plugin.name().as_str(), Some("Sintetizador"));
    assert_eq!(
        gain.name()
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("Ganancia")
    );

    world.set_lang("fr");
    assert_eq!(
        gain.comment()
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("Commentaire gain")
    );

    world.set_lang("cn");
    assert_eq!(plugin.name().as_str(), Some("Synthetic Test Plugin"));
    assert_eq!(
        gain.comment()
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("Gain comment")
    );

    world.set_lang("1!");
    assert_eq!(world.lang(), None);
    assert_eq!(plugin.name().as_str(), Some("Synthetic Test Plugin"));

    world.set_filter_lang(false);
    let names = plugin.value(&world.new_uri("http://usefulinc.com/ns/doap#name"));
    assert_eq!(names.len(), 3);
}

#[test]
fn ports() {
    let temp = TempDir::new("ports");
    let (world, _) = load_world_with_bundle(&temp);

    let plugin = world
        .plugins()
        .plugin(&world.new_uri(PLUGIN_URI))
        .expect("plugin not found");
    assert_eq!(plugin.ports_count(), 4);
    assert_eq!(plugin.iter_ports().count(), 4);
    assert_eq!(plugin.latency_port_index(), Some(3));
    assert!(plugin.has_latency());

    let input_port = world.new_uri("http://lv2plug.in/ns/lv2core#InputPort");
    let output_port = world.new_uri("http://lv2plug.in/ns/lv2core#OutputPort");
    let control_port = world.new_uri("http://lv2plug.in/ns/lv2core#ControlPort");
    let audio_port = world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort");
    let toggled = world.new_uri("http://lv2plug.in/ns/lv2core#toggled");
    let midi_event = world.new_uri("http://lv2plug.in/ns/ext/midi#MidiEvent");
    let time_position = world.new_uri("http://lv2plug.in/ns/ext/time#Position");

    let gain = plugin.port_by_index(0).expect("port 0");
    assert_eq!(gain.index(), 0);
    assert_eq!(
        gain.symbol()
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("gain")
    );
    assert_eq!(
        gain.name()
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("Gain")
    );
    assert!(gain.is_a(&input_port));
    assert!(gain.is_a(&control_port));
    assert!(!gain.is_a(&output_port));
    assert!(!gain.is_a(&audio_port));
    assert!(gain.has_property(&toggled));
    assert!(gain.is_input());
    assert!(!gain.is_output());
    assert!(gain.is_control());
    assert!(!gain.is_audio());
    assert!(!gain.is_cv());
    assert!(gain.is_integer());
    assert!(gain.is_enumeration());
    assert!(gain.is_sample_rate());
    assert!(gain.is_hidden());
    assert!(gain.is_logarithmic());
    assert!(!gain.is_connection_optional());
    assert_eq!(
        gain.comment()
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("Gain comment")
    );
    assert_eq!(
        gain.group()
            .and_then(|n| n.as_uri().map(str::to_string))
            .as_deref(),
        Some("http://example.org/groups/main")
    );
    assert!(!gain.supports_event(&midi_event));
    let range = gain.range();
    assert_eq!(range.minimum.as_ref().and_then(Node::as_float), Some(0.0));
    assert_eq!(range.maximum.as_ref().and_then(Node::as_float), Some(1.0));
    assert_eq!(range.default.as_ref().and_then(Node::as_float), Some(0.5));

    let events = plugin
        .port_by_symbol(&world.new_string("events_in"))
        .expect("events_in");
    assert_eq!(events.index(), 1);
    assert!(events.supports_event(&midi_event));
    assert!(!events.supports_event(&time_position));

    let out = plugin
        .port_by_symbol(&world.new_string("out"))
        .expect("out");
    assert!(out.is_a(&output_port) && out.is_a(&audio_port));

    let latency = plugin.port_by_index(3).expect("port 3");
    assert!(latency.is_a(&output_port) && latency.is_a(&control_port));
    assert!(latency.range().default.is_none());

    assert!(plugin.port_by_index(4).is_none());
    let scale_points = gain.scale_points();
    assert_eq!(scale_points.count(), 1);
    let point = scale_points.iter().next().expect("scale point");
    assert_eq!(point.label().as_str(), Some("half"));
    assert_eq!(point.value().as_float(), Some(0.5));

    let latency_designation = world.new_uri("http://lv2plug.in/ns/lv2core#latency");
    assert_eq!(
        plugin
            .port_by_designation(&output_port, &latency_designation)
            .map(|port| port.index()),
        Some(3)
    );
    assert_eq!(
        plugin
            .port_by_designation_optional(None, &latency_designation)
            .map(|port| port.index()),
        Some(3)
    );
    assert_eq!(plugin.num_ports_of_class([&output_port]), 2);
    let ranges = plugin.port_ranges_float();
    assert_eq!(ranges[0], (Some(0.0), Some(1.0), Some(0.5)));
    assert_eq!(
        gain.range_float(48_000.0),
        (Some(0.0), Some(48_000.0), Some(0.5))
    );

    assert!(plugin.port_by_index(4).is_none());
    assert!(plugin.port_by_symbol(&world.new_string("nope")).is_none());
}

#[test]
fn world_queries_and_classes() {
    let temp = TempDir::new("world_queries");
    let (world, bundle) = load_world_with_bundle(&temp);

    let subject = world.new_uri(PLUGIN_URI);
    let plugin = world.plugins().plugin(&subject).expect("plugin");
    let name = world.new_uri("http://usefulinc.com/ns/doap#name");
    assert!(world.ask(Some(&subject), Some(&name), None));
    assert_eq!(
        world
            .get(Some(&subject), Some(&name), None)
            .and_then(|node| node.as_str().map(str::to_string))
            .as_deref(),
        Some("Synthetic Test Plugin")
    );
    assert_eq!(world.find_nodes(None, Some(&name), None).count(), 1);
    assert_eq!(
        world
            .symbol(&subject)
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("synth")
    );
    let explicit_symbol_subject = plugin.port_by_index(0).expect("port").node();
    assert_eq!(
        world
            .symbol(&explicit_symbol_subject)
            .and_then(|n| n.as_str().map(str::to_string))
            .as_deref(),
        Some("gain")
    );

    let file_uri = Node::file_uri("/tmp/Maolan LV2/test.ttl");
    assert_eq!(
        file_uri.path().as_deref(),
        Some(std::path::Path::new("/tmp/Maolan LV2/test.ttl"))
    );

    let classes = world.plugin_classes();
    let class = classes
        .get_by_uri(&world.new_uri(CLASS_URI))
        .expect("synthetic class");
    assert_eq!(
        class
            .parent_uri()
            .and_then(|n| n.as_uri().map(str::to_string))
            .as_deref(),
        Some("http://lv2plug.in/ns/lv2core#Plugin")
    );
    assert!(
        world
            .plugin_class()
            .children()
            .get_by_uri(&world.new_uri(CLASS_URI))
            .is_some()
    );

    world
        .unload_bundle(&world.new_uri(&bundle))
        .expect("unload bundle");
    assert_eq!(world.plugins().count(), 0);
    assert!(!world.ask(Some(&subject), Some(&name), None));
}

#[test]
fn load_all_uses_explicit_lv2_path() {
    let temp = TempDir::new("lv2_path");
    let search_root = temp.path().join("lv2");
    std::fs::create_dir_all(&search_root).expect("create search root");
    let bundle_dir = search_root.join("synth.lv2");
    std::fs::create_dir_all(&bundle_dir).expect("create bundle dir");
    write_file(&bundle_dir, "manifest.ttl", &manifest(2, 0));
    write_file(&bundle_dir, "plugin.ttl", &plugin_ttl("gain"));

    let world = World::new();
    world.set_lv2_path([search_root]);
    world.load_all();
    assert!(world.plugins().plugin(&world.new_uri(PLUGIN_URI)).is_some());

    world.clear_lv2_path();
}

#[test]
fn duplicate_versions_keep_newest() {
    let temp = TempDir::new("versions");
    let world = World::new();

    // Load version 2.0 first.
    let v2 = make_bundle(&temp, "v2", 2, 0, "gain");
    world.load_bundle(&world.new_uri(&v2));

    // An older version in a different bundle is ignored.
    let v1 = make_bundle(&temp, "v1", 1, 0, "old_gain");
    world.load_bundle(&world.new_uri(&v1));
    assert_eq!(world.plugins().count(), 1);
    let plugin = world
        .plugins()
        .plugin(&world.new_uri(PLUGIN_URI))
        .expect("plugin not found");
    assert_eq!(plugin.bundle_uri().as_uri(), Some(v2.as_str()));

    // A duplicate of the same version: first loaded wins.
    let v2dup = make_bundle(&temp, "v2dup", 2, 0, "dup_gain");
    world.load_bundle(&world.new_uri(&v2dup));
    assert_eq!(world.plugins().count(), 1);
    let plugin = world
        .plugins()
        .plugin(&world.new_uri(PLUGIN_URI))
        .expect("plugin not found");
    assert_eq!(plugin.bundle_uri().as_uri(), Some(v2.as_str()));

    // A newer version replaces the old one.
    let v3 = make_bundle(&temp, "v3", 3, 1, "new_gain");
    world.load_bundle(&world.new_uri(&v3));
    assert_eq!(world.plugins().count(), 1);
    let plugin = world
        .plugins()
        .plugin(&world.new_uri(PLUGIN_URI))
        .expect("plugin not found");
    assert_eq!(plugin.bundle_uri().as_uri(), Some(v3.as_str()));
    // The old bundle's triples are gone: only the new port symbol resolves.
    assert!(
        plugin
            .port_by_symbol(&world.new_string("new_gain"))
            .is_some()
    );
    assert!(plugin.port_by_symbol(&world.new_string("gain")).is_none());
    assert!(plugin.verify());
}

#[test]
fn replacement_metadata_follows_loaded_graphs() {
    let temp = TempDir::new("replacement");
    let (world, _) = load_world_with_bundle(&temp);
    let plugin = world
        .plugins()
        .plugin(&world.new_uri(PLUGIN_URI))
        .expect("plugin");
    assert!(!plugin.is_replaced());

    let dir = temp.bundle("replacement-meta");
    write_file(
        &dir,
        "manifest.ttl",
        &format!(
            r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
@prefix dct: <http://purl.org/dc/terms/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<http://example.org/plugins/replacement>
    a lv2:Plugin ;
    doap:name "Replacement" ;
    dct:replaces <{PLUGIN_URI}> ;
    lv2:binary <replacement.so> ;
    rdfs:seeAlso <plugin.ttl> .
"#
        ),
    );
    write_file(
        &dir,
        "plugin.ttl",
        r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<http://example.org/plugins/replacement>
    lv2:port [ a lv2:InputPort , lv2:ControlPort ; lv2:index 0 ; lv2:symbol "gain" ; lv2:name "Gain" ] .
"#,
    );
    let replacement_bundle = bundle_uri(&dir);
    world.load_bundle(&world.new_uri(&replacement_bundle));
    assert!(plugin.is_replaced());

    world
        .unload_bundle(&world.new_uri(&replacement_bundle))
        .expect("unload replacement bundle");
    assert!(!plugin.is_replaced());
}

#[test]
fn verify_rejects_bad_bundles() {
    let temp = TempDir::new("verify");

    // Missing doap:name.
    let dir = temp.bundle("noname");
    write_file(
        &dir,
        "manifest.ttl",
        r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<http://example.org/plugins/noname>
    a lv2:Plugin ;
    lv2:binary <x.so> ;
    rdfs:seeAlso <plugin.ttl> .
"#,
    );
    write_file(
        &dir,
        "plugin.ttl",
        r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<http://example.org/plugins/noname>
    a lv2:Plugin ;
    lv2:port [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:index 0 ;
        lv2:symbol "x" ;
        lv2:name "X"
    ] .
"#,
    );
    let world = World::new();
    world.load_bundle(&world.new_uri(&bundle_uri(&dir)));
    let plugin = world
        .plugins()
        .plugin(&world.new_uri("http://example.org/plugins/noname"))
        .expect("plugin not found");
    assert!(!plugin.verify(), "plugin without doap:name must not verify");
    assert_eq!(plugin.name().as_str(), Some(""));

    // Broken Turtle in a data file marks the plugin as not verifiable, but
    // does not panic and does not prevent discovery.
    let dir = temp.bundle("broken");
    write_file(
        &dir,
        "manifest.ttl",
        r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<http://example.org/plugins/broken>
    a lv2:Plugin ;
    lv2:binary <x.so> ;
    rdfs:seeAlso <plugin.ttl> .
"#,
    );
    write_file(
        &dir,
        "plugin.ttl",
        r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
<http://example.org/plugins/broken>
    a lv2:Plugin ;
    doap:name "Broken" ;
    lv2:port [ this is not valid turtle !!!
"#,
    );
    let world = World::new();
    world.load_bundle(&world.new_uri(&bundle_uri(&dir)));
    let plugin = world
        .plugins()
        .plugin(&world.new_uri("http://example.org/plugins/broken"))
        .expect("plugin not found");
    assert!(!plugin.verify(), "plugin with parse errors must not verify");

    // A bundle without a manifest is skipped silently.
    let dir = temp.bundle("empty");
    let world = World::new();
    world.load_bundle(&world.new_uri(&bundle_uri(&dir)));
    assert_eq!(world.plugins().count(), 0);
}

#[test]
fn state_metadata_from_world_and_string() {
    let temp = TempDir::new("state_metadata");
    let (world, _) = load_world_with_bundle(&temp);
    let state = maolan_lv2::state::State::new_from_world(
        &world,
        &world.new_uri("http://example.org/presets/default"),
    )
    .expect("state from world");
    assert_eq!(state.label(), Some("Default"));
    let default_state =
        maolan_lv2::state::State::new_from_world(&world, &world.new_uri(PLUGIN_URI))
            .expect("default state from plugin URI");
    assert_eq!(
        default_state
            .plugin_uri()
            .and_then(|n| n.as_uri().map(str::to_string))
            .as_deref(),
        Some(PLUGIN_URI)
    );
    assert_eq!(
        state
            .plugin_uri()
            .and_then(|n| n.as_uri().map(str::to_string))
            .as_deref(),
        Some(PLUGIN_URI)
    );
    assert_eq!(
        state.port_value("gain").and_then(|n| n.as_float()),
        Some(0.25)
    );

    let ttl = format!(
        r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix pset: <http://lv2plug.in/ns/ext/presets#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<http://example.org/presets/string>
    a pset:Preset ;
    rdfs:label "String State" ;
    lv2:appliesTo <{PLUGIN_URI}> ;
    lv2:port [ lv2:symbol "gain" ; pset:value 0.75 ] .
"#
    );
    let state = maolan_lv2::state::State::new_from_string(&ttl, None).expect("state from string");
    assert_eq!(state.label(), Some("String State"));
    assert_eq!(state.port_values().len(), 1);
    assert_eq!(
        state.port_value("gain").and_then(|n| n.as_float()),
        Some(0.75)
    );
    let label_predicate = world.new_uri("http://www.w3.org/2000/01/rdf-schema#label");
    assert_eq!(state.num_properties(), 3);
    assert_eq!(
        state.property_values(&label_predicate)[0].as_str(),
        Some("String State")
    );
    let mut emitted = Vec::new();
    state.emit_port_values(|symbol, value| {
        emitted.push((symbol.to_string(), value.as_float()));
    });
    assert_eq!(emitted, vec![("gain".to_string(), Some(0.75))]);

    let mut state = state.clone();
    state.set_label("Edited State");
    assert_eq!(state.label(), Some("Edited State"));
    state.clear_label();
    assert_eq!(state.label(), None);
    let comment = world.new_uri("http://www.w3.org/2000/01/rdf-schema#comment");
    state.set_metadata(comment.clone(), Node::string("host note"));
    assert_eq!(
        state.property_values(&comment)[0].as_str(),
        Some("host note")
    );
}

#[test]
fn state_metadata_from_file_records_bundle_path() {
    let temp = TempDir::new("state_file");
    let path = temp.path().join("state.ttl");
    write_file(
        temp.path(),
        "state.ttl",
        &format!(
            r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix pset: <http://lv2plug.in/ns/ext/presets#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<http://example.org/presets/file>
    a pset:Preset ;
    rdfs:label "File State" ;
    lv2:appliesTo <{PLUGIN_URI}> ;
    lv2:port [ lv2:symbol "gain" ; pset:value 1 ] .
"#
        ),
    );
    let state = maolan_lv2::state::State::new_from_file(&path, None).expect("state from file");
    assert_eq!(state.bundle_path(), Some(temp.path()));
    assert_eq!(
        state.port_value("gain").and_then(|n| n.as_float()),
        Some(1.0)
    );
}

#[test]
fn state_save_string_and_delete() {
    let temp = TempDir::new("state_save");
    let mut state = maolan_lv2::state::State::new_from_string(
        &format!(
            r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix pset: <http://lv2plug.in/ns/ext/presets#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<http://example.org/presets/save>
    a pset:Preset ;
    rdfs:label "Saved State" ;
    lv2:appliesTo <{PLUGIN_URI}> ;
    lv2:port [ lv2:symbol "gain" ; lv2:default 0.33 ] .
"#
        ),
        None,
    )
    .expect("state from string");
    state.add_port_value("mix", Node::float(0.66));
    let turtle = state.to_turtle_string(Some("http://example.org/presets/exported"));
    assert!(turtle.contains("http://example.org/presets/exported"));
    assert!(turtle.contains("pset:value"));
    assert_eq!(
        state.port_value("gain").and_then(|n| n.as_float()),
        Some(0.33)
    );

    state
        .save(
            temp.path(),
            "custom.ttl",
            Some("http://example.org/presets/exported"),
        )
        .expect("save state");
    assert!(temp.path().join("custom.ttl").exists());
    assert!(temp.path().join("manifest.ttl").exists());
    assert_eq!(state.bundle_path(), Some(temp.path()));

    state.delete(None).expect("delete state");
    assert!(!temp.path().join("custom.ttl").exists());
}

#[test]
fn load_and_unload_resource_uses_seealso_file_graphs() {
    let temp = TempDir::new("resource_load");
    let dir = temp.bundle("resource");
    write_file(
        &dir,
        "manifest.ttl",
        &format!(
            r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<{PLUGIN_URI}>
    a lv2:Plugin ;
    doap:name "Resource Test" ;
    lv2:binary <synth.so> ;
    rdfs:seeAlso <plugin.ttl> ;
    rdfs:seeAlso <presets.ttl> .
"#
        ),
    );
    write_file(
        &dir,
        "plugin.ttl",
        &format!(
            r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<{PLUGIN_URI}> lv2:port [ a lv2:InputPort , lv2:ControlPort ; lv2:index 0 ; lv2:symbol "gain" ; lv2:name "Gain" ] .
"#
        ),
    );
    write_file(
        &dir,
        "presets.ttl",
        &format!(
            r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix pset: <http://lv2plug.in/ns/ext/presets#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<http://example.org/presets/external> a pset:Preset ; lv2:appliesTo <{PLUGIN_URI}> ; rdfs:label "External" .
"#
        ),
    );

    let world = World::new();
    let bundle = bundle_uri(&dir);
    world.load_bundle(&world.new_uri(&bundle));
    let plugin = world
        .plugins()
        .plugin(&world.new_uri(PLUGIN_URI))
        .expect("plugin");
    let preset = world.new_uri("http://example.org/presets/external");
    assert!(plugin.presets().contains(&preset));

    assert_eq!(
        world
            .unload_resource(&world.new_uri(PLUGIN_URI))
            .expect("unload resource"),
        2
    );
    assert!(!plugin.presets().contains(&preset));

    assert_eq!(
        world
            .load_resource(&world.new_uri(PLUGIN_URI))
            .expect("load resource"),
        2
    );
    assert!(plugin.presets().contains(&preset));
}

#[test]
fn dynamic_manifest_is_disabled_by_default_and_loads_when_enabled() {
    let temp = TempDir::new("dynmanifest");
    let dir = temp.bundle("dynamic");
    build_dynmanifest_library(&dir);
    write_file(
        &dir,
        "manifest.ttl",
        r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix dynman: <http://lv2plug.in/ns/ext/dynmanifest#> .

<http://example.org/dynmanifest>
    a dynman:DynManifest ;
    lv2:binary <dynmanifest.so> .
"#,
    );

    let bundle = bundle_uri(&dir);
    let disabled_world = World::new();
    assert!(!disabled_world.dyn_manifest_enabled());
    disabled_world.load_bundle(&disabled_world.new_uri(&bundle));
    assert!(
        disabled_world
            .plugins()
            .plugin(&disabled_world.new_uri("http://example.org/plugins/dynamic"))
            .is_none()
    );

    let enabled_world = World::new();
    enabled_world.set_dyn_manifest_enabled(true);
    enabled_world.load_bundle(&enabled_world.new_uri(&bundle));
    let plugin = enabled_world
        .plugins()
        .plugin(&enabled_world.new_uri("http://example.org/plugins/dynamic"))
        .expect("generated plugin");
    assert_eq!(plugin.name().as_str(), Some("Dynamic Test Plugin"));
    assert!(
        plugin
            .library_uri()
            .and_then(|node| node.as_uri().map(str::to_string))
            .is_some_and(|uri| uri.ends_with("/dynamic.so"))
    );
}

fn build_dynmanifest_library(dir: &Path) {
    let source = dir.join("dynmanifest.c");
    let output = dir.join("dynmanifest.so");
    std::fs::write(
        &source,
        r#"#include <stdio.h>

int lv2_dyn_manifest_open(void **handle, const void * const *features) {
    (void)features;
    *handle = (void*)1;
    return 0;
}

int lv2_dyn_manifest_get_subjects(void *handle, FILE *fp) {
    (void)handle;
    return fprintf(fp, "<http://example.org/plugins/dynamic> a <http://lv2plug.in/ns/lv2core#Plugin> .\n") < 0;
}

int lv2_dyn_manifest_get_data(void *handle, FILE *fp, const char *uri) {
    (void)handle;
    (void)uri;
    return fprintf(fp,
        "<http://example.org/plugins/dynamic> "
        "a <http://lv2plug.in/ns/lv2core#Plugin> ; "
        "<http://usefulinc.com/ns/doap#name> \"Dynamic Test Plugin\" ; "
        "<http://lv2plug.in/ns/lv2core#binary> <dynamic.so> .\n") < 0;
}

void lv2_dyn_manifest_close(void *handle) {
    (void)handle;
}
"#,
    )
    .expect("write dynmanifest source");
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&output)
        .arg(&source)
        .status()
        .expect("run cc");
    assert!(status.success(), "compile dynmanifest fixture");
}
