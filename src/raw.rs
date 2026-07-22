//! Minimal LV2 C ABI definitions used by Maolan.
//!
//! These are hand-written `#[repr(C)]` bindings for the stable LV2 headers.
//! Keep this module restricted to ABI items Maolan actually passes to or
//! receives from LV2 plugin binaries.

use std::ffi::{c_char, c_int, c_uint, c_void};
use std::mem::size_of;
use std::ptr;

pub type LV2Handle = *mut c_void;
pub type LV2LibHandle = *mut c_void;

#[repr(C)]
pub struct LV2Feature {
    pub uri: *const c_char,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct LV2Descriptor {
    pub uri: *const c_char,
    pub instantiate: extern "C" fn(
        descriptor: *const LV2Descriptor,
        rate: f64,
        bundle_path: *const c_char,
        features: *const *const LV2Feature,
    ) -> LV2Handle,
    pub connect_port: extern "C" fn(handle: LV2Handle, port: u32, data: *mut c_void),
    pub activate: Option<extern "C" fn(instance: LV2Handle)>,
    pub run: extern "C" fn(instance: LV2Handle, n_samples: u32),
    pub deactivate: Option<extern "C" fn(instance: LV2Handle)>,
    pub cleanup: extern "C" fn(instance: LV2Handle),
    pub extension_data: extern "C" fn(uri: *const u8) -> *const c_void,
}

#[repr(C)]
pub struct LV2LibDescriptor {
    pub handle: LV2LibHandle,
    pub size: u32,
    pub cleanup: Option<extern "C" fn(handle: LV2LibHandle)>,
    pub get_plugin: Option<extern "C" fn(handle: LV2LibHandle, index: u32) -> *const LV2Descriptor>,
}

pub type LV2Urid = u32;
pub type LV2UridMapHandle = *mut c_void;

pub const LV2_URID__MAP: &str = "http://lv2plug.in/ns/ext/urid#map";
pub const LV2_URID__UNMAP: &str = "http://lv2plug.in/ns/ext/urid#unmap";

#[repr(C)]
pub struct LV2UridMap {
    pub handle: LV2UridMapHandle,
    pub map: extern "C" fn(handle: LV2UridMapHandle, uri: *const c_char) -> LV2Urid,
}

pub const LV2_ATOM__DOUBLE: &[u8] = b"http://lv2plug.in/ns/ext/atom#Double\0";
pub const LV2_ATOM__FLOAT: &[u8] = b"http://lv2plug.in/ns/ext/atom#Float\0";
pub const LV2_ATOM__FRAMETIME: &[u8] = b"http://lv2plug.in/ns/ext/atom#frameTime\0";
pub const LV2_ATOM__INT: &[u8] = b"http://lv2plug.in/ns/ext/atom#Int\0";
pub const LV2_ATOM__LONG: &[u8] = b"http://lv2plug.in/ns/ext/atom#Long\0";
pub const LV2_ATOM__CHUNK: &[u8] = b"http://lv2plug.in/ns/ext/atom#Chunk\0";
pub const LV2_ATOM__OBJECT: &[u8] = b"http://lv2plug.in/ns/ext/atom#Object\0";
pub const LV2_ATOM__SEQUENCE: &[u8] = b"http://lv2plug.in/ns/ext/atom#Sequence\0";

pub const LV2_MIDI__MIDIEVENT: &[u8] = b"http://lv2plug.in/ns/ext/midi#MidiEvent\0";

pub const LV2_TIME__POSITION: &[u8] = b"http://lv2plug.in/ns/ext/time#Position\0";
pub const LV2_TIME__FRAME: &[u8] = b"http://lv2plug.in/ns/ext/time#frame\0";
pub const LV2_TIME__SPEED: &[u8] = b"http://lv2plug.in/ns/ext/time#speed\0";
pub const LV2_TIME__BEATSPERMINUTE: &[u8] = b"http://lv2plug.in/ns/ext/time#beatsPerMinute\0";
pub const LV2_TIME__BAR: &[u8] = b"http://lv2plug.in/ns/ext/time#bar\0";
pub const LV2_TIME__BARBEAT: &[u8] = b"http://lv2plug.in/ns/ext/time#barBeat\0";
pub const LV2_TIME__BEATSPERBAR: &[u8] = b"http://lv2plug.in/ns/ext/time#beatsPerBar\0";
pub const LV2_TIME__BEATUNIT: &[u8] = b"http://lv2plug.in/ns/ext/time#beatUnit\0";

#[repr(C)]
pub struct LV2Atom {
    pub size: u32,
    pub mytype: u32,
}

#[repr(C)]
pub struct LV2AtomLong {
    pub atom: LV2Atom,
    pub body: i64,
}

#[repr(C)]
pub struct LV2AtomDouble {
    pub atom: LV2Atom,
    pub body: f64,
}

#[repr(C)]
pub struct LV2AtomPropertyBody {
    pub key: u32,
    pub context: u32,
    pub value: LV2Atom,
}

#[repr(C)]
pub struct LV2AtomObjectBody {
    pub id: u32,
    pub otype: u32,
}

#[repr(C)]
pub struct LV2AtomEvent {
    pub time_in_frames: i64,
    pub body: LV2Atom,
}

#[repr(C)]
pub struct LV2AtomSequenceBody {
    pub unit: u32,
    pub pad: u32,
}

#[repr(C)]
pub struct LV2AtomSequence {
    pub atom: LV2Atom,
    pub body: LV2AtomSequenceBody,
}

#[must_use]
pub fn lv2_atom_pad_size(size: u32) -> u32 {
    (size + 7) & !7
}

/// # Safety
///
/// `body` must point to a valid `LV2AtomSequenceBody`.
#[must_use]
pub unsafe fn lv2_atom_sequence_begin(body: *const LV2AtomSequenceBody) -> *mut LV2AtomEvent {
    unsafe { body.add(1).cast_mut().cast::<LV2AtomEvent>() }
}

/// # Safety
///
/// `body` must point to a valid sequence body whose backing allocation is at
/// least `size` bytes after the body header.
#[must_use]
pub unsafe fn lv2_atom_sequence_is_end(
    body: *const LV2AtomSequenceBody,
    size: u32,
    i: *const LV2AtomEvent,
) -> bool {
    unsafe { i.cast::<u8>() >= body.cast::<u8>().add(size as usize) }
}

/// # Safety
///
/// `i` must point to a valid event in an LV2 atom sequence.
#[must_use]
pub unsafe fn lv2_atom_sequence_next(i: *const LV2AtomEvent) -> *mut LV2AtomEvent {
    let offset = size_of::<LV2AtomEvent>() + unsafe { lv2_atom_pad_size((*i).body.size) as usize };
    unsafe { i.cast::<u8>().add(offset).cast_mut().cast::<LV2AtomEvent>() }
}

/// # Safety
///
/// `body` must point to a valid sequence body, and `size` must be the current
/// sequence atom body size.
#[must_use]
pub unsafe fn lv2_atom_sequence_end(
    body: *const LV2AtomSequenceBody,
    size: u32,
) -> *const LV2AtomEvent {
    unsafe {
        body.cast::<u8>()
            .add(lv2_atom_pad_size(size) as usize)
            .cast::<LV2AtomEvent>()
    }
}

/// # Safety
///
/// `seq` must point to writable storage for an LV2 sequence atom with total
/// atom body capacity `capacity`. `event` must point to a valid event.
pub unsafe fn lv2_atom_sequence_append_event(
    seq: *mut LV2AtomSequence,
    capacity: u32,
    event: *const LV2AtomEvent,
) -> *const LV2AtomEvent {
    let total_size = size_of::<LV2AtomEvent>() as u32 + unsafe { (*event).body.size };
    let used = unsafe { (*seq).atom.size };
    if capacity < used || capacity - used < total_size {
        return ptr::null();
    }

    let dst = unsafe { lv2_atom_sequence_end(&raw const (*seq).body, used) };
    unsafe {
        ptr::copy_nonoverlapping(
            event.cast::<u8>(),
            dst.cast_mut().cast::<u8>(),
            total_size as usize,
        );
        (*seq).atom.size += lv2_atom_pad_size(total_size);
    }
    dst
}

pub type LV2UIHandle = *mut c_void;
pub type LV2UIWidget = *mut c_void;
pub type LV2UIControllerRaw = *const c_void;
pub type LV2UIWriteFunctionRaw = Option<
    extern "C" fn(
        controller: LV2UIControllerRaw,
        port_index: c_uint,
        buffer_size: c_uint,
        port_protocol: c_uint,
        buffer: *const c_void,
    ),
>;

#[repr(C)]
pub struct LV2UIDescriptorRaw {
    pub uri: *const c_char,
    pub instantiate_raw: extern "C" fn(
        descriptor: *const LV2UIDescriptorRaw,
        plugin_uri: *const c_char,
        bundle_path: *const c_char,
        write_function: LV2UIWriteFunctionRaw,
        controller: LV2UIControllerRaw,
        widget: *mut LV2UIWidget,
        features: *const *const LV2Feature,
    ) -> LV2UIHandle,
    pub cleanup: extern "C" fn(LV2UIHandle),
    pub port_event: extern "C" fn(
        ui: LV2UIHandle,
        port_index: c_uint,
        buffer_size: c_uint,
        format: c_uint,
        buffer: *const c_void,
    ),
    pub extension_data: Option<extern "C" fn(*const c_char) -> *const c_void>,
}

#[repr(C)]
pub struct LV2UIIdleInterface {
    pub idle: extern "C" fn(ui: LV2UIHandle) -> c_int,
}

#[repr(C)]
pub struct LV2UIShowInterface {
    pub show: extern "C" fn(ui: LV2UIHandle) -> c_int,
    pub hide: extern "C" fn(ui: LV2UIHandle) -> c_int,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atom_layouts_match_lv2_abi() {
        assert_eq!(size_of::<LV2Atom>(), 8);
        assert_eq!(size_of::<LV2AtomLong>(), 16);
        assert_eq!(size_of::<LV2AtomDouble>(), 16);
        assert_eq!(size_of::<LV2AtomEvent>(), 16);
        assert_eq!(size_of::<LV2AtomSequenceBody>(), 8);
        assert_eq!(size_of::<LV2AtomSequence>(), 16);
        assert_eq!(size_of::<LV2AtomPropertyBody>(), 16);
        assert_eq!(size_of::<LV2AtomObjectBody>(), 8);
    }

    #[test]
    fn atom_padding_is_64_bit_aligned() {
        assert_eq!(lv2_atom_pad_size(0), 0);
        assert_eq!(lv2_atom_pad_size(1), 8);
        assert_eq!(lv2_atom_pad_size(8), 8);
        assert_eq!(lv2_atom_pad_size(9), 16);
    }
}
