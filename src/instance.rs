//! Plugin instances: loaded shared libraries and live LV2 handles.

use crate::raw::{LV2Descriptor, LV2Feature, LV2Handle, LV2LibDescriptor, LV2Urid};
use crate::runtime_state::{
    LV2_STATE_INTERFACE_URI, LV2_STATE_STATUS_SUCCESS, Lv2StateInterface, Lv2StateProperty,
    StateSaveContext, decode_properties, encode_properties, feature_array,
    lv2_state_retrieve_callback, lv2_state_store_callback,
};
use crate::uris;
use libloading::Library;
use std::ffi::{CStr, CString};
use std::ptr::NonNull;

/// The prototype of the `lv2_descriptor` symbol every LV2 plugin library
/// exports.
type LV2DescriptorFn = unsafe extern "C" fn(index: u32) -> *const LV2Descriptor;
type LV2LibDescriptorFn = unsafe extern "C" fn(
    bundle_path: *const std::ffi::c_char,
    features: *const *const LV2Feature,
) -> *const LV2LibDescriptor;

/// An LV2 plugin instance.
///
/// Owns the loaded shared library and the plugin handle. The library stays
/// mapped for as long as the instance lives; on drop the descriptor's
/// `cleanup` callback is invoked before the library is unloaded.
pub struct Instance {
    // `_lib` must be declared first so it is dropped last: `Drop::drop`
    // calls the plugin's `cleanup` (code inside `_lib`) before field drop
    // unloads it.
    _lib: Library,
    descriptor: *const LV2Descriptor,
    lib_descriptor: Option<NonNull<LV2LibDescriptor>>,
    handle: LV2Handle,
    uri: String,
}

/// An LV2 plugin instance that has been activated and is ready to process
/// data.
pub struct ActiveInstance {
    inner: Instance,
}

// SAFETY: the LV2 ABI allows instances to be moved between threads as long as
// the host does not call instance methods concurrently (LV2 plugins are not
// required to be thread-safe; the host must serialize calls, which `&mut
// self` receivers and the consumer's locking discipline provide).
unsafe impl Send for Instance {}

impl Instance {
    /// Loads `library_uri`, finds the descriptor for `plugin_uri`, and
    /// instantiates it. Mirrors `lilv_plugin_instantiate`.
    ///
    /// # Safety
    /// Calls arbitrary plugin code (library constructors and `instantiate`).
    pub(crate) unsafe fn new(
        plugin_uri: &str,
        library_uri: &str,
        bundle_uri: &str,
        sample_rate: f64,
        features: *const *const LV2Feature,
        num_ports: u32,
    ) -> Option<Instance> {
        let library_path = uris::file_uri_to_path(library_uri)?;
        // The bundle path is passed to the plugin with a trailing slash, as
        // required by the LV2 spec (plugins append filenames to it).
        let mut bundle_path = uris::file_uri_to_path(bundle_uri)?;
        if !bundle_path.ends_with(std::path::MAIN_SEPARATOR) {
            bundle_path.push(std::path::MAIN_SEPARATOR);
        }
        let bundle_path = CString::new(bundle_path).ok()?;

        // SAFETY: loading a shared library runs its initializers; the path
        // comes from the plugin's own manifest data. lilv dlopens with
        // RTLD_NOW (resolve all symbols eagerly).
        #[cfg(unix)]
        let lib = unsafe {
            match libloading::os::unix::Library::open(
                Some(&library_path),
                libloading::os::unix::RTLD_NOW,
            ) {
                Ok(lib) => Library::from(lib),
                Err(error) => {
                    lv2_log!(
                        "maolan_lv2_lib_open(): error: Failed to open library {library_path} ({error})"
                    );
                    return None;
                }
            }
        };
        #[cfg(windows)]
        let lib = unsafe {
            match Library::new(&library_path) {
                Ok(lib) => lib,
                Err(error) => {
                    lv2_log!(
                        "maolan_lv2_lib_open(): error: Failed to open library {library_path} ({error})"
                    );
                    return None;
                }
            }
        };

        // SAFETY: LV2 libraries export either `lv2_descriptor` or
        // `lv2_lib_descriptor`. lilv opens both when present, uses the direct
        // descriptor function for plugin lookup, and keeps the library
        // descriptor alive until all instances from the library are cleaned up.
        let descriptor_fn = unsafe { lib.get::<LV2DescriptorFn>(b"lv2_descriptor\0").ok() };
        let lib_descriptor_fn =
            unsafe { lib.get::<LV2LibDescriptorFn>(b"lv2_lib_descriptor\0").ok() };
        if descriptor_fn.is_none() && lib_descriptor_fn.is_none() {
            lv2_log!(
                "maolan_lv2_lib_open(): error: No lv2_descriptor or lv2_lib_descriptor in {library_path}"
            );
            return None;
        }

        let lib_descriptor = if let Some(lib_descriptor_fn) = lib_descriptor_fn.as_ref() {
            let raw = unsafe { lib_descriptor_fn(bundle_path.as_ptr(), features) };
            let Some(ptr) = NonNull::new(raw.cast_mut()) else {
                lv2_log!(
                    "maolan_lv2_lib_open(): error: Call to {library_path}:lv2_lib_descriptor failed"
                );
                return None;
            };
            Some(ptr)
        } else {
            None
        };

        // Search for the plugin by URI.
        let mut index = 0_u32;
        let descriptor = loop {
            let descriptor = if let Some(descriptor_fn) = descriptor_fn.as_ref() {
                // SAFETY: `lv2_descriptor(index)` must return a valid
                // descriptor pointer or NULL per the LV2 ABI.
                unsafe { descriptor_fn(index) }
            } else {
                let lib_descriptor = lib_descriptor?;
                let lib_descriptor_ref = unsafe { lib_descriptor.as_ref() };
                let Some(get_plugin) = lib_descriptor_ref.get_plugin else {
                    cleanup_lib_descriptor(lib_descriptor);
                    return None;
                };
                get_plugin(lib_descriptor_ref.handle, index)
            };
            if descriptor.is_null() {
                lv2_log!(
                    "maolan_lv2_plugin_instantiate(): error: No plugin <{plugin_uri}> in <{library_uri}>"
                );
                if let Some(lib_descriptor) = lib_descriptor {
                    cleanup_lib_descriptor(lib_descriptor);
                }
                return None;
            }
            // SAFETY: the descriptor is valid per the LV2 ABI, and `uri` is a
            // NUL-terminated string owned by the library.
            let uri = unsafe { CStr::from_ptr((*descriptor).uri) };
            if uri.to_bytes() == plugin_uri.as_bytes() {
                break descriptor;
            }
            index += 1;
        };

        // SAFETY: `descriptor` is valid, `bundle_path` is NUL-terminated and
        // lives until the end of the call, and `features` is a NULL-terminated
        // array as the ABI requires. The plugin may refuse to instantiate by
        // returning NULL.
        let handle = unsafe {
            ((*descriptor).instantiate)(descriptor, sample_rate, bundle_path.as_ptr(), features)
        };
        if handle.is_null() {
            lv2_log!(
                "maolan_lv2_plugin_instantiate(): error: Plugin <{plugin_uri}> failed to instantiate"
            );
            if let Some(lib_descriptor) = lib_descriptor {
                cleanup_lib_descriptor(lib_descriptor);
            }
            return None;
        }

        // "Connect" all ports to NULL, like lilv does, to catch plugins that
        // read port buffers before connect_port is called.
        // The raw LV2 ABI types declare these callbacks as non-nullable function
        // pointers, but real-world plugins leave them null anyway, so check
        // via the same address comparison the consumer uses.
        for port in 0..num_ports {
            // SAFETY: the descriptor is valid for the lifetime of the library.
            let connect_port = unsafe { &*descriptor }.connect_port;
            if (connect_port as *const ()) as usize != 0 {
                // The handle is live and NULL is a valid data pointer for a
                // port the host has not connected yet.
                connect_port(handle, port, std::ptr::null_mut());
            }
        }

        Some(Instance {
            _lib: lib,
            descriptor,
            lib_descriptor,
            handle,
            uri: plugin_uri.to_string(),
        })
    }

    /// Returns the URI of the plugin for the instance.
    /// This is a globally unique string for the plugin.
    #[must_use]
    pub fn uri(&self) -> Option<&str> {
        Some(&self.uri)
    }

    /// Connect a port on a plugin instance to a memory location.
    ///
    /// `data` should point to data of the type defined by the port type in
    /// the plugin's RDF data (e.g. an array of float for an lv2:AudioPort).
    ///
    /// # Safety
    /// Connecting a port calls the plugin's code, which itself may be unsafe.
    pub unsafe fn connect_port_mut<T>(&mut self, port_index: usize, data: *mut T) {
        let Ok(port_index) = u32::try_from(port_index) else {
            debug_assert!(false, "port_index is too large");
            return;
        };
        // SAFETY: the descriptor is valid for the lifetime of the instance.
        let connect_port = unsafe { &*self.descriptor }.connect_port;
        if (connect_port as *const ()) as usize == 0 {
            return;
        }
        // The handle is live; the caller guarantees `data` points to a
        // suitable buffer.
        connect_port(self.handle, port_index, data.cast());
    }

    /// Connect data pointer to a port on a plugin instance. Similar to
    /// `connect_port_mut` but takes a const pointer instead.
    ///
    /// # Safety
    /// Connecting a port calls the plugin's code, which itself may be unsafe.
    pub unsafe fn connect_port<T>(&mut self, port_index: usize, data: *const T) {
        // SAFETY: same contract as connect_port_mut; the plugin may write
        // through the pointer for output ports.
        unsafe { self.connect_port_mut(port_index, data as *mut T) };
    }

    /// Activate a plugin instance.
    ///
    /// This resets all state information in the plugin except for port
    /// connections.
    ///
    /// # Safety
    /// Calling external code may be unsafe.
    #[must_use]
    pub unsafe fn activate(self) -> ActiveInstance {
        // SAFETY: the descriptor is valid for the lifetime of the instance.
        let activate = unsafe { &*self.descriptor }.activate;
        if let Some(activate) = activate {
            // The handle is live and not concurrently used.
            activate(self.handle);
        }
        ActiveInstance { inner: self }
    }

    /// Get the extension data for a plugin instance.
    ///
    /// The type and semantics of the data returned is specific to the
    /// particular extension, though in all cases it is shared and must not be
    /// deleted.
    ///
    /// # Safety
    /// Gathering extension data calls the plugin's code, which itself may be
    /// unsafe.
    #[must_use]
    pub unsafe fn extension_data<T>(&self, uri: &str) -> Option<NonNull<T>> {
        let uri = CString::new(uri).ok()?;
        // SAFETY: the descriptor is valid for the lifetime of the instance.
        let descriptor = unsafe { &*self.descriptor };
        if (descriptor.extension_data as *const ()) as usize == 0 {
            return None;
        }
        // `uri` is NUL-terminated and lives until the end of the call; the
        // returned pointer is owned by the plugin.
        NonNull::new((descriptor.extension_data)(uri.as_ptr().cast()) as *mut T)
    }

    /// Saves plugin-defined LV2 state properties using the state extension.
    ///
    /// `unmap_urid` must convert URIDs emitted by the plugin back to URI strings.
    /// `features` are passed to `LV2_State_Interface.save` and may include
    /// `state:mapPath`, `state:makePath`, and related host features.
    pub fn save_state_properties(
        &self,
        mut unmap_urid: impl FnMut(LV2Urid) -> Option<String>,
        features: &[*const LV2Feature],
    ) -> Result<Vec<Lv2StateProperty>, String> {
        let interface =
            unsafe { self.extension_data::<Lv2StateInterface>(LV2_STATE_INTERFACE_URI) }
                .map(|ptr| unsafe { ptr.as_ref() })
                .ok_or_else(|| format!("LV2 plugin '{}' does not support state", self.uri))?;
        let save = interface
            .save
            .ok_or_else(|| format!("LV2 plugin '{}' has no state save callback", self.uri))?;
        let mut ctx = StateSaveContext {
            properties: Vec::new(),
        };
        let features = feature_array(features);
        let status = unsafe {
            save(
                self.handle,
                Some(lv2_state_store_callback),
                (&mut ctx as *mut StateSaveContext).cast(),
                0,
                features.as_ptr(),
            )
        };
        if status != LV2_STATE_STATUS_SUCCESS {
            return Err(format!(
                "LV2 state save failed for '{}': status {}",
                self.uri, status
            ));
        }
        Ok(decode_properties(ctx.properties, &mut unmap_urid))
    }

    /// Restores plugin-defined LV2 state properties using the state extension.
    ///
    /// `map_uri` must map URI strings to the same URID space used for instance features.
    pub fn restore_state_properties(
        &mut self,
        properties: &[Lv2StateProperty],
        mut map_uri: impl FnMut(&str) -> LV2Urid,
        features: &[*const LV2Feature],
    ) -> Result<(), String> {
        if properties.is_empty() {
            return Ok(());
        }
        let interface =
            unsafe { self.extension_data::<Lv2StateInterface>(LV2_STATE_INTERFACE_URI) }
                .map(|ptr| unsafe { ptr.as_ref() })
                .ok_or_else(|| format!("LV2 plugin '{}' does not support state", self.uri))?;
        let restore = interface
            .restore
            .ok_or_else(|| format!("LV2 plugin '{}' has no state restore callback", self.uri))?;
        let mut ctx = encode_properties(properties, &mut map_uri);
        let features = feature_array(features);
        let status = unsafe {
            restore(
                self.handle,
                Some(lv2_state_retrieve_callback),
                (&mut ctx as *mut crate::runtime_state::StateRestoreContext).cast(),
                0,
                features.as_ptr(),
            )
        };
        if status == LV2_STATE_STATUS_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "LV2 state restore failed for '{}': status {}",
                self.uri, status
            ))
        }
    }

    /// Get the raw descriptor for the plugin.
    #[must_use]
    pub fn descriptor(&self) -> Option<&LV2Descriptor> {
        // SAFETY: the descriptor pointer is valid for the lifetime of the
        // instance because the library is kept loaded in `self.lib`.
        unsafe { self.descriptor.as_ref() }
    }

    /// Get the raw handle for the plugin instance.
    #[must_use]
    pub fn handle(&self) -> LV2Handle {
        self.handle
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: the descriptor is valid for the lifetime of the instance.
        let cleanup = unsafe { &*self.descriptor }.cleanup;
        if (cleanup as *const ()) as usize != 0 && !self.handle.is_null() {
            // Pairs with the successful `instantiate` call; the handle is
            // live and not concurrently used.
            cleanup(self.handle);
        }
        if let Some(lib_descriptor) = self.lib_descriptor {
            cleanup_lib_descriptor(lib_descriptor);
        }
    }
}

fn cleanup_lib_descriptor(lib_descriptor: NonNull<LV2LibDescriptor>) {
    let lib_descriptor = unsafe { lib_descriptor.as_ref() };
    if let Some(cleanup) = lib_descriptor.cleanup {
        cleanup(lib_descriptor.handle);
    }
}

impl ActiveInstance {
    /// Run the plugin instance for `sample_count` frames.
    ///
    /// # Safety
    /// Calling external code may be unsafe.
    pub unsafe fn run(&mut self, sample_count: usize) {
        let sample_count = u32::try_from(sample_count).unwrap_or(u32::MAX);
        // SAFETY: the descriptor is valid for the lifetime of the instance.
        let run = unsafe { &*self.inner.descriptor }.run;
        if (run as *const ()) as usize == 0 {
            return;
        }
        // The handle is live; the caller guarantees all ports are connected
        // to suitable buffers.
        run(self.inner.handle, sample_count);
    }

    /// Saves plugin-defined LV2 state properties using the state extension.
    pub fn save_state_properties(
        &self,
        unmap_urid: impl FnMut(LV2Urid) -> Option<String>,
        features: &[*const LV2Feature],
    ) -> Result<Vec<Lv2StateProperty>, String> {
        self.inner.save_state_properties(unmap_urid, features)
    }

    /// Restores plugin-defined LV2 state properties using the state extension.
    pub fn restore_state_properties(
        &mut self,
        properties: &[Lv2StateProperty],
        map_uri: impl FnMut(&str) -> LV2Urid,
        features: &[*const LV2Feature],
    ) -> Result<(), String> {
        self.inner
            .restore_state_properties(properties, map_uri, features)
    }

    /// Deactivate the plugin instance.
    ///
    /// Note: This will reset all state information except for port
    /// connections.
    ///
    /// # Safety
    /// Calling external code may be unsafe.
    #[must_use]
    pub unsafe fn deactivate(self) -> Option<Instance> {
        // Wrap in ManuallyDrop so the Drop impl (which would call the
        // plugin's deactivate a second time) does not run when the inner
        // Instance is moved out.
        let mut this = std::mem::ManuallyDrop::new(self);
        this.deactivate_impl();
        // SAFETY: `this` is never dropped, so the Instance is moved out
        // exactly once and no double-deactivate can occur.
        Some(unsafe { std::ptr::read(&raw const this.inner) })
    }

    /// Get the underlying instance.
    #[must_use]
    pub fn instance(&self) -> &Instance {
        &self.inner
    }

    /// Get the underlying instance.
    ///
    /// This is useful to call `connect_port` if the data locations have
    /// changed.
    pub fn instance_mut(&mut self) -> &mut Instance {
        &mut self.inner
    }

    fn deactivate_impl(&mut self) {
        // SAFETY: the descriptor is valid for the lifetime of the instance.
        let deactivate = unsafe { &*self.inner.descriptor }.deactivate;
        if let Some(deactivate) = deactivate {
            // The handle is live, was activated, and is not concurrently
            // used.
            deactivate(self.inner.handle);
        }
    }
}

impl Drop for ActiveInstance {
    fn drop(&mut self) {
        self.deactivate_impl();
    }
}
