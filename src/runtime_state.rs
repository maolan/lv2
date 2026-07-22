//! Runtime LV2 state save/restore support.

use crate::raw::{LV2Feature, LV2Handle, LV2Urid};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const LV2_STATE_INTERFACE_URI: &str = "http://lv2plug.in/ns/ext/state#interface";
pub const LV2_STATE_MAP_PATH_URI: &str = "http://lv2plug.in/ns/ext/state#mapPath";
pub const LV2_STATE_MAKE_PATH_URI: &str = "http://lv2plug.in/ns/ext/state#makePath";
pub const LV2_STATE_FREE_PATH_URI: &str = "http://lv2plug.in/ns/ext/state#freePath";
pub const LV2_STATE_LOAD_DEFAULT_STATE: &str = "http://lv2plug.in/ns/ext/state#loadDefaultState";

pub type Lv2StateHandle = *mut c_void;
pub type Lv2StateStatus = u32;
pub const LV2_STATE_STATUS_SUCCESS: Lv2StateStatus = 0;
pub const LV2_STATE_STATUS_ERR_NO_PROPERTY: Lv2StateStatus = 5;

pub type Lv2StateStoreFn = Option<
    unsafe extern "C" fn(
        handle: Lv2StateHandle,
        key: u32,
        value: *const c_void,
        size: usize,
        type_: u32,
        flags: u32,
    ) -> Lv2StateStatus,
>;
pub type Lv2StateRetrieveFn = Option<
    unsafe extern "C" fn(
        handle: Lv2StateHandle,
        key: u32,
        size: *mut usize,
        type_: *mut u32,
        flags: *mut u32,
    ) -> *const c_void,
>;

#[repr(C)]
pub struct Lv2StateInterface {
    pub save: Option<
        unsafe extern "C" fn(
            instance: LV2Handle,
            store: Lv2StateStoreFn,
            handle: Lv2StateHandle,
            flags: u32,
            features: *const *const LV2Feature,
        ) -> Lv2StateStatus,
    >,
    pub restore: Option<
        unsafe extern "C" fn(
            instance: LV2Handle,
            retrieve: Lv2StateRetrieveFn,
            handle: Lv2StateHandle,
            flags: u32,
            features: *const *const LV2Feature,
        ) -> Lv2StateStatus,
    >,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lv2StateProperty {
    pub key_uri: String,
    pub type_uri: String,
    pub flags: u32,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawStateProperty {
    pub key: LV2Urid,
    pub type_: LV2Urid,
    pub flags: u32,
    pub value: Vec<u8>,
}

pub(crate) struct StateSaveContext {
    pub properties: Vec<RawStateProperty>,
}

pub(crate) struct StateRestoreContext {
    pub properties: Vec<RawStateProperty>,
    pub by_key: HashMap<LV2Urid, usize>,
}

#[repr(C)]
pub struct Lv2StateMapPath {
    pub handle: *mut c_void,
    pub abstract_path: Option<extern "C" fn(*mut c_void, *const c_char) -> *mut c_char>,
    pub absolute_path: Option<extern "C" fn(*mut c_void, *const c_char) -> *mut c_char>,
}

#[repr(C)]
pub struct Lv2StateMakePath {
    pub handle: *mut c_void,
    pub path: Option<extern "C" fn(*mut c_void, *const c_char) -> *mut c_char>,
}

#[repr(C)]
pub struct Lv2StateFreePath {
    pub handle: *mut c_void,
    pub free_path: Option<extern "C" fn(*mut c_void, *mut c_char)>,
}

#[derive(Default)]
struct StatePathContext {
    base_dir: PathBuf,
    copy_counter: u64,
}

pub struct StatePathFeature {
    _map_uri: CString,
    _make_uri: CString,
    _free_uri: CString,
    _map: Box<Lv2StateMapPath>,
    _make: Box<Lv2StateMakePath>,
    _free: Box<Lv2StateFreePath>,
    map_feature: LV2Feature,
    make_feature: LV2Feature,
    free_feature: LV2Feature,
    context: Box<Mutex<StatePathContext>>,
}

unsafe impl Send for StatePathFeature {}

impl StatePathFeature {
    #[must_use]
    pub fn new(base_dir: PathBuf) -> Self {
        let context = Box::new(Mutex::new(StatePathContext {
            base_dir,
            copy_counter: 0,
        }));
        let handle = (&*context as *const Mutex<StatePathContext>) as *mut c_void;

        let map = Box::new(Lv2StateMapPath {
            handle,
            abstract_path: Some(lv2_state_abstract_path_callback),
            absolute_path: Some(lv2_state_absolute_path_callback),
        });
        let make = Box::new(Lv2StateMakePath {
            handle,
            path: Some(lv2_state_make_path_callback),
        });
        let free = Box::new(Lv2StateFreePath {
            handle,
            free_path: Some(lv2_state_free_path_callback),
        });

        let map_uri = CString::new(LV2_STATE_MAP_PATH_URI).expect("valid LV2 state mapPath URI");
        let make_uri = CString::new(LV2_STATE_MAKE_PATH_URI).expect("valid LV2 state makePath URI");
        let free_uri = CString::new(LV2_STATE_FREE_PATH_URI).expect("valid LV2 state freePath URI");

        let map_feature = LV2Feature {
            uri: map_uri.as_ptr(),
            data: (&*map as *const Lv2StateMapPath)
                .cast_mut()
                .cast::<c_void>(),
        };
        let make_feature = LV2Feature {
            uri: make_uri.as_ptr(),
            data: (&*make as *const Lv2StateMakePath)
                .cast_mut()
                .cast::<c_void>(),
        };
        let free_feature = LV2Feature {
            uri: free_uri.as_ptr(),
            data: (&*free as *const Lv2StateFreePath)
                .cast_mut()
                .cast::<c_void>(),
        };

        let instance = Self {
            _map_uri: map_uri,
            _make_uri: make_uri,
            _free_uri: free_uri,
            _map: map,
            _make: make,
            _free: free,
            map_feature,
            make_feature,
            free_feature,
            context,
        };
        instance.ensure_base_dir();
        instance
    }

    pub fn set_base_dir(&self, base_dir: PathBuf) {
        if let Ok(mut ctx) = self.context.lock() {
            ctx.base_dir = base_dir;
            let _ = std::fs::create_dir_all(&ctx.base_dir);
        }
    }

    #[must_use]
    pub fn feature_ptrs(&self) -> [*const LV2Feature; 3] {
        [
            &self.map_feature as *const LV2Feature,
            &self.make_feature as *const LV2Feature,
            &self.free_feature as *const LV2Feature,
        ]
    }

    fn ensure_base_dir(&self) {
        if let Ok(ctx) = self.context.lock() {
            let _ = std::fs::create_dir_all(&ctx.base_dir);
        }
    }
}

pub(crate) fn feature_array(features: &[*const LV2Feature]) -> Vec<*const LV2Feature> {
    let mut out: Vec<*const LV2Feature> = features
        .iter()
        .copied()
        .filter(|feature| !feature.is_null())
        .collect();
    if out.last().is_none_or(|feature| !feature.is_null()) {
        out.push(std::ptr::null());
    }
    out
}

pub(crate) fn decode_properties(
    raw: Vec<RawStateProperty>,
    mut unmap: impl FnMut(LV2Urid) -> Option<String>,
) -> Vec<Lv2StateProperty> {
    raw.into_iter()
        .filter_map(|property| {
            let key_uri = unmap(property.key)?;
            let type_uri = unmap(property.type_)?;
            Some(Lv2StateProperty {
                key_uri,
                type_uri,
                flags: property.flags,
                value: property.value,
            })
        })
        .collect()
}

pub(crate) fn encode_properties(
    properties: &[Lv2StateProperty],
    mut map: impl FnMut(&str) -> LV2Urid,
) -> StateRestoreContext {
    let mut raw = Vec::new();
    let mut by_key = HashMap::new();
    for property in properties {
        let key = map(&property.key_uri);
        let type_ = map(&property.type_uri);
        if key == 0 || type_ == 0 {
            continue;
        }
        let index = raw.len();
        raw.push(RawStateProperty {
            key,
            type_,
            flags: property.flags,
            value: property.value.clone(),
        });
        by_key.insert(key, index);
    }
    StateRestoreContext {
        properties: raw,
        by_key,
    }
}

pub(crate) unsafe extern "C" fn lv2_state_store_callback(
    handle: Lv2StateHandle,
    key: u32,
    value: *const c_void,
    size: usize,
    type_: u32,
    flags: u32,
) -> Lv2StateStatus {
    if handle.is_null() || value.is_null() || size == 0 {
        return LV2_STATE_STATUS_ERR_NO_PROPERTY;
    }
    let ctx = unsafe { &mut *(handle as *mut StateSaveContext) };
    let bytes = unsafe { std::slice::from_raw_parts(value.cast::<u8>(), size) };
    ctx.properties.push(RawStateProperty {
        key,
        type_,
        flags,
        value: bytes.to_vec(),
    });
    LV2_STATE_STATUS_SUCCESS
}

pub(crate) unsafe extern "C" fn lv2_state_retrieve_callback(
    handle: Lv2StateHandle,
    key: u32,
    size: *mut usize,
    type_: *mut u32,
    flags: *mut u32,
) -> *const c_void {
    if handle.is_null() {
        return std::ptr::null();
    }
    let ctx = unsafe { &mut *(handle as *mut StateRestoreContext) };
    let Some(index) = ctx.by_key.get(&key).copied() else {
        return std::ptr::null();
    };
    let Some(property) = ctx.properties.get(index) else {
        return std::ptr::null();
    };
    if !size.is_null() {
        unsafe {
            *size = property.value.len();
        }
    }
    if !type_.is_null() {
        unsafe {
            *type_ = property.type_;
        }
    }
    if !flags.is_null() {
        unsafe {
            *flags = property.flags;
        }
    }
    property.value.as_ptr().cast::<c_void>()
}

extern "C" fn lv2_state_free_path_callback(_handle: *mut c_void, path: *mut c_char) {
    if path.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(path);
    }
}

fn state_ctx_from_handle(handle: *mut c_void) -> Option<&'static Mutex<StatePathContext>> {
    if handle.is_null() {
        return None;
    }
    Some(unsafe { &*(handle as *const Mutex<StatePathContext>) })
}

fn copy_into_state_assets(ctx: &mut StatePathContext, src: &Path) -> Option<String> {
    let file_name = src.file_name()?.to_str()?.to_string();
    let assets_dir = ctx.base_dir.join("assets");
    let _ = std::fs::create_dir_all(&assets_dir);
    ctx.copy_counter = ctx.copy_counter.saturating_add(1);
    let dst_name = format!("{}-{file_name}", ctx.copy_counter);
    let dst = assets_dir.join(&dst_name);
    std::fs::copy(src, &dst).ok()?;
    Some(format!("assets/{dst_name}"))
}

extern "C" fn lv2_state_abstract_path_callback(
    handle: *mut c_void,
    absolute_path: *const c_char,
) -> *mut c_char {
    let Some(ctx_lock) = state_ctx_from_handle(handle) else {
        return std::ptr::null_mut();
    };
    if absolute_path.is_null() {
        return std::ptr::null_mut();
    }
    let Some(path_str) = (unsafe { CStr::from_ptr(absolute_path) }).to_str().ok() else {
        return std::ptr::null_mut();
    };
    let path = PathBuf::from(path_str);
    let mut mapped = None;
    if let Ok(mut ctx) = ctx_lock.lock() {
        if let Ok(rel) = path.strip_prefix(&ctx.base_dir) {
            mapped = Some(rel.to_string_lossy().to_string());
        } else if path.exists() {
            mapped = copy_into_state_assets(&mut ctx, &path);
        }
    }
    let out = mapped.unwrap_or_else(|| path_str.to_string());
    CString::new(out)
        .ok()
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

extern "C" fn lv2_state_absolute_path_callback(
    handle: *mut c_void,
    abstract_path: *const c_char,
) -> *mut c_char {
    let Some(ctx_lock) = state_ctx_from_handle(handle) else {
        return std::ptr::null_mut();
    };
    if abstract_path.is_null() {
        return std::ptr::null_mut();
    }
    let Some(path_str) = (unsafe { CStr::from_ptr(abstract_path) }).to_str().ok() else {
        return std::ptr::null_mut();
    };
    let output = if Path::new(path_str).is_absolute() {
        path_str.to_string()
    } else if let Ok(ctx) = ctx_lock.lock() {
        ctx.base_dir.join(path_str).to_string_lossy().to_string()
    } else {
        path_str.to_string()
    };
    CString::new(output)
        .ok()
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

extern "C" fn lv2_state_make_path_callback(
    handle: *mut c_void,
    requested: *const c_char,
) -> *mut c_char {
    let Some(ctx_lock) = state_ctx_from_handle(handle) else {
        return std::ptr::null_mut();
    };
    let requested_name = if requested.is_null() {
        "state.bin".to_string()
    } else {
        (unsafe { CStr::from_ptr(requested) })
            .to_str()
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.replace("..", "_"))
            .unwrap_or_else(|| "state.bin".to_string())
    };
    let output = if let Ok(mut ctx) = ctx_lock.lock() {
        ctx.copy_counter = ctx.copy_counter.saturating_add(1);
        let file_name = format!("generated-{}-{requested_name}", ctx.copy_counter);
        let path = ctx.base_dir.join("generated").join(file_name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        path.to_string_lossy().to_string()
    } else {
        requested_name
    };
    CString::new(output)
        .ok()
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_retrieve_properties_preserves_metadata_and_bytes() {
        let properties = vec![Lv2StateProperty {
            key_uri: "http://example.org/state/key".to_string(),
            type_uri: "http://lv2plug.in/ns/ext/atom#String".to_string(),
            flags: 7,
            value: b"saved-value".to_vec(),
        }];
        let mut ctx = encode_properties(&properties, |uri| match uri {
            "http://example.org/state/key" => 11,
            "http://lv2plug.in/ns/ext/atom#String" => 22,
            _ => 0,
        });

        let mut size = 0_usize;
        let mut type_ = 0_u32;
        let mut flags = 0_u32;
        let value = unsafe {
            lv2_state_retrieve_callback(
                (&mut ctx as *mut StateRestoreContext).cast(),
                11,
                &mut size,
                &mut type_,
                &mut flags,
            )
        };

        assert!(!value.is_null());
        assert_eq!(size, b"saved-value".len());
        assert_eq!(type_, 22);
        assert_eq!(flags, 7);
        let bytes = unsafe { std::slice::from_raw_parts(value.cast::<u8>(), size) };
        assert_eq!(bytes, b"saved-value");
    }

    #[test]
    fn store_and_decode_properties_drops_unmapped_urids() {
        let mut ctx = StateSaveContext { properties: vec![] };
        let value = [1_u8, 2, 3, 4];
        let status = unsafe {
            lv2_state_store_callback(
                (&mut ctx as *mut StateSaveContext).cast(),
                5,
                value.as_ptr().cast(),
                value.len(),
                6,
                9,
            )
        };
        assert_eq!(status, LV2_STATE_STATUS_SUCCESS);

        let decoded = decode_properties(ctx.properties, |urid| match urid {
            5 => Some("http://example.org/key".to_string()),
            6 => Some("http://example.org/type".to_string()),
            _ => None,
        });
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].key_uri, "http://example.org/key");
        assert_eq!(decoded[0].type_uri, "http://example.org/type");
        assert_eq!(decoded[0].flags, 9);
        assert_eq!(decoded[0].value, value);
    }
}
