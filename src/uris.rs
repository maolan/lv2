//! Well-known URIs used by the LV2 data model, plus file-URI helpers.

pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub(crate) const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
pub(crate) const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
pub(crate) const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
pub(crate) const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
pub(crate) const RDFS_SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

pub(crate) const DOAP_NAME: &str = "http://usefulinc.com/ns/doap#name";
pub(crate) const DOAP_MAINTAINER: &str = "http://usefulinc.com/ns/doap#maintainer";
pub(crate) const DOAP_HOMEPAGE: &str = "http://usefulinc.com/ns/doap#homepage";
pub(crate) const DCT_REPLACES: &str = "http://purl.org/dc/terms/replaces";
pub(crate) const FOAF_NAME: &str = "http://xmlns.com/foaf/0.1/name";
pub(crate) const FOAF_MBOX: &str = "http://xmlns.com/foaf/0.1/mbox";
pub(crate) const FOAF_HOMEPAGE: &str = "http://xmlns.com/foaf/0.1/homepage";

pub(crate) const LV2_PLUGIN: &str = "http://lv2plug.in/ns/lv2core#Plugin";
pub(crate) const LV2_SPECIFICATION: &str = "http://lv2plug.in/ns/lv2core#Specification";
pub(crate) const LV2_APPLIES_TO: &str = "http://lv2plug.in/ns/lv2core#appliesTo";
pub(crate) const LV2_PROJECT: &str = "http://lv2plug.in/ns/lv2core#project";
pub(crate) const LV2_BINARY: &str = "http://lv2plug.in/ns/lv2core#binary";
pub(crate) const LV2_PORT: &str = "http://lv2plug.in/ns/lv2core#port";
pub(crate) const LV2_INDEX: &str = "http://lv2plug.in/ns/lv2core#index";
pub(crate) const LV2_SYMBOL: &str = "http://lv2plug.in/ns/lv2core#symbol";
pub(crate) const LV2_NAME: &str = "http://lv2plug.in/ns/lv2core#name";
pub(crate) const LV2_MINIMUM: &str = "http://lv2plug.in/ns/lv2core#minimum";
pub(crate) const LV2_MAXIMUM: &str = "http://lv2plug.in/ns/lv2core#maximum";
pub(crate) const LV2_DEFAULT: &str = "http://lv2plug.in/ns/lv2core#default";
pub(crate) const LV2_DESIGNATION: &str = "http://lv2plug.in/ns/lv2core#designation";
pub(crate) const LV2_LATENCY: &str = "http://lv2plug.in/ns/lv2core#latency";
pub(crate) const LV2_PORT_PROPERTY: &str = "http://lv2plug.in/ns/lv2core#portProperty";
pub(crate) const LV2_REPORTS_LATENCY: &str = "http://lv2plug.in/ns/lv2core#reportsLatency";
pub(crate) const LV2_CONNECTION_OPTIONAL: &str = "http://lv2plug.in/ns/lv2core#connectionOptional";
pub(crate) const LV2_SAMPLE_RATE: &str = "http://lv2plug.in/ns/lv2core#sampleRate";
pub(crate) const LV2_INTEGER: &str = "http://lv2plug.in/ns/lv2core#integer";
pub(crate) const LV2_ENUMERATION: &str = "http://lv2plug.in/ns/lv2core#enumeration";
pub(crate) const LV2_INPUT_PORT: &str = "http://lv2plug.in/ns/lv2core#InputPort";
pub(crate) const LV2_OUTPUT_PORT: &str = "http://lv2plug.in/ns/lv2core#OutputPort";
pub(crate) const LV2_AUDIO_PORT: &str = "http://lv2plug.in/ns/lv2core#AudioPort";
pub(crate) const LV2_CONTROL_PORT: &str = "http://lv2plug.in/ns/lv2core#ControlPort";
pub(crate) const LV2_CV_PORT: &str = "http://lv2plug.in/ns/lv2core#CVPort";
pub(crate) const LV2_REQUIRED_FEATURE: &str = "http://lv2plug.in/ns/lv2core#requiredFeature";
pub(crate) const LV2_OPTIONAL_FEATURE: &str = "http://lv2plug.in/ns/lv2core#optionalFeature";
pub(crate) const LV2_EXTENSION_DATA: &str = "http://lv2plug.in/ns/lv2core#extensionData";
pub(crate) const LV2_MINOR_VERSION: &str = "http://lv2plug.in/ns/lv2core#minorVersion";
pub(crate) const LV2_MICRO_VERSION: &str = "http://lv2plug.in/ns/lv2core#microVersion";

pub(crate) const LV2_SCALE_POINT: &str = "http://lv2plug.in/ns/lv2core#scalePoint";
pub(crate) const RDF_VALUE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#value";

pub(crate) const UI_UI: &str = "http://lv2plug.in/ns/extensions/ui#ui";
pub(crate) const UI_BINARY: &str = "http://lv2plug.in/ns/extensions/ui#binary";
pub(crate) const UI_X11_UI: &str = "http://lv2plug.in/ns/extensions/ui#X11UI";
pub(crate) const UI_GTK_UI: &str = "http://lv2plug.in/ns/extensions/ui#GtkUI";
pub(crate) const UI_GTK3_UI: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";
pub(crate) const UI_QT4_UI: &str = "http://lv2plug.in/ns/extensions/ui#Qt4UI";
pub(crate) const UI_QT5_UI: &str = "http://lv2plug.in/ns/extensions/ui#Qt5UI";

pub(crate) const PRESETS_PRESET: &str = "http://lv2plug.in/ns/ext/presets#Preset";
pub(crate) const PRESETS_VALUE: &str = "http://lv2plug.in/ns/ext/presets#value";
pub(crate) const PRESETS_BANK: &str = "http://lv2plug.in/ns/ext/presets#bank";

pub(crate) const PORT_GROUPS_GROUP: &str = "http://lv2plug.in/ns/ext/port-groups#group";
pub(crate) const PORT_PROPS_NOT_ON_GUI: &str = "http://lv2plug.in/ns/ext/port-props#notOnGUI";
pub(crate) const PORT_PROPS_LOGARITHMIC: &str = "http://lv2plug.in/ns/ext/port-props#logarithmic";

pub(crate) const ATOM_SUPPORTS: &str = "http://lv2plug.in/ns/ext/atom#supports";
pub(crate) const ATOM_ATOM_PORT: &str = "http://lv2plug.in/ns/ext/atom#AtomPort";
pub(crate) const EVENT_SUPPORTS_EVENT: &str = "http://lv2plug.in/ns/ext/event#supportsEvent";
pub(crate) const EVENT_EVENT_PORT: &str = "http://lv2plug.in/ns/ext/event#EventPort";
pub(crate) const MIDI_MIDI_EVENT: &str = "http://lv2plug.in/ns/ext/midi#MidiEvent";
pub(crate) const DYNMANIFEST_DYN_MANIFEST: &str =
    "http://lv2plug.in/ns/ext/dynmanifest#DynManifest";

pub(crate) const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
pub(crate) const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
pub(crate) const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
pub(crate) const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
pub(crate) const XSD_FLOAT: &str = "http://www.w3.org/2001/XMLSchema#float";
pub(crate) const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// Returns `true` if `s` is a valid LV2 symbol (see `is_symbol` in lilv's
/// plugin.c): ASCII letters and underscores anywhere, digits except first.
pub(crate) fn is_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.char_indices()
            .all(|(i, c)| c.is_ascii_alphabetic() || c == '_' || (i > 0 && c.is_ascii_digit()))
}

/// Builds a `file://` URI for an absolute path, percent-encoding everything
/// outside the RFC 3986 unreserved set plus `/`.
pub(crate) fn path_to_file_uri(path: &str) -> String {
    let mut uri = String::with_capacity(path.len() + 8);
    uri.push_str("file://");
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                uri.push(char::from(byte));
            }
            _ => {
                uri.push('%');
                uri.push_str(&format!("{byte:02X}"));
            }
        }
    }
    uri
}

/// Parses a `file://` URI back into a filesystem path, percent-decoding it.
/// Returns `None` for non-file URIs.
pub(crate) fn file_uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    // Skip an (empty or "localhost") authority component.
    let path_start = rest.find('/')?;
    let authority = &rest[..path_start];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let path = &rest[path_start..];
    let mut out = Vec::with_capacity(path.len());
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hex = std::str::from_utf8(&bytes[i + 1..=i + 2]).ok()?;
            let value = u8::from_str_radix(hex, 16).ok()?;
            out.push(value);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uri_roundtrip() {
        let path = "/usr/local/lib/lv2/Surge XT.lv2/";
        let uri = path_to_file_uri(path);
        assert_eq!(uri, "file:///usr/local/lib/lv2/Surge%20XT.lv2/");
        assert_eq!(file_uri_to_path(&uri).as_deref(), Some(path));
    }

    #[test]
    fn file_uri_authority() {
        assert_eq!(
            file_uri_to_path("file://localhost/tmp/x").as_deref(),
            Some("/tmp/x")
        );
        assert!(file_uri_to_path("file://example.com/tmp/x").is_none());
        assert!(file_uri_to_path("http://example.com/x").is_none());
    }

    #[test]
    fn symbols() {
        assert!(is_symbol("gain"));
        assert!(is_symbol("_in1"));
        assert!(!is_symbol("1in"));
        assert!(!is_symbol("has space"));
        assert!(!is_symbol(""));
    }
}
