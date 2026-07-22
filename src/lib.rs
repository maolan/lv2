//! Pure-Rust LV2 plugin discovery and instantiation.
//!
//! This crate is a C-dependency-free reimplementation of the parts of the
//! [lilv](https://drobilla.net/category/lilv) library that the Maolan plugin
//! host needs: scanning `LV2_PATH` for bundles, parsing their Turtle data,
//! querying plugin/port metadata, and instantiating plugins from their shared
//! libraries via `dlopen`.
//!
//! The public API mirrors the `lilv` crate (0.2.x) so consumers can switch
//! with minimal changes.

/// Logs a diagnostic to stderr, unconditionally, in the same style as C
/// lilv: `function_name(): warning: message` (or `error:` / `note:`).
///
/// The plugin host captures stderr during LV2 scans and classifies lines
/// containing these markers into structured diagnostics that are surfaced in
/// scan results, so staying quiet would silently drop that UX.
macro_rules! lv2_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

/// Contains functionality for plugin instances that process data.
pub mod instance;
/// Contains functionality for nodes. Nodes are used to represent metadata.
pub mod node;
/// Contains functionality to describe and instantiate plugins.
pub mod plugin;
/// Contains port to describe IO for plugins.
pub mod port;
/// Minimal raw LV2 ABI used for plugin binary calls.
pub mod raw;
/// Runtime LV2 state save/restore support.
pub mod runtime_state;
/// Contains LV2 state and preset metadata.
pub mod state;

mod model;
mod uris;
mod world;

pub use world::World;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_safety_bounds() {
        fn assert_send_sync<T: Send + Sync>() {}
        fn assert_send<T: Send>() {}

        assert_send_sync::<World>();
        assert_send_sync::<node::Node>();
        assert_send_sync::<node::Nodes>();
        assert_send_sync::<plugin::Plugin>();
        assert_send_sync::<plugin::Class>();
        assert_send_sync::<port::Port>();
        assert_send::<instance::Instance>();
        assert_send::<instance::ActiveInstance>();
    }
}
