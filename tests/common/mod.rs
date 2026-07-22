//! Shared helpers for integration tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A temporary directory that is removed on drop.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(name: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "maolan-lv2-test-{}-{id}-{name}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    /// Creates a bundle directory `<name>.lv2` and returns its path.
    pub fn bundle(&self, name: &str) -> PathBuf {
        let path = self.path.join(format!("{name}.lv2"));
        std::fs::create_dir_all(&path).expect("create bundle dir");
        path
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Writes `contents` to `dir/file_name`.
pub fn write_file(dir: &Path, file_name: &str, contents: &str) {
    std::fs::write(dir.join(file_name), contents).expect("write bundle file");
}

/// Builds the `file://` URI (with trailing slash) for a bundle directory.
pub fn bundle_uri(dir: &Path) -> String {
    let mut path = dir.to_str().expect("utf-8 path").to_string();
    if !path.ends_with('/') {
        path.push('/');
    }
    let mut uri = String::from("file://");
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                uri.push(char::from(byte));
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}
