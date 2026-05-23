//! C ABI for [`gpmf_parser`].
//!
//! The host application demuxes the GoPro `gpmd` track itself (e.g. via
//! FFmpeg) and passes one GPMF sample payload at a time to
//! `jgpmf_sample_parse`. Per-sample telemetry (`GPS9`, `ACCL`, `GYRO`,
//! `GRAV`, `CORI`, `IORI`) is then queried through dedicated getters.
//!
//! All `extern "C"` entry points are panic-safe — Rust panics are caught and
//! reported as `JGPMF_ERR_PARSE` rather than unwinding across the FFI
//! boundary.

#![allow(non_camel_case_types)] // variants like JGPMF_OK are intentional for the C-facing header

use std::panic::{catch_unwind, AssertUnwindSafe};

use gpmf_parser::{GpmfSample, Klv, Quat as ParserQuat, Vec3 as ParserVec3, klv::Value};

// --- Layout-compatibility assertions ---------------------------------------
//
// We expose dedicated `Jgpmf*` structs in the C header but alias their
// pointers into the `gpmf_parser`-owned storage. The casts are sound only if
// the structs have identical layouts.
const _: () = {
    assert!(core::mem::size_of::<JgpmfVec3>() == core::mem::size_of::<ParserVec3>());
    assert!(core::mem::align_of::<JgpmfVec3>() == core::mem::align_of::<ParserVec3>());
    assert!(core::mem::size_of::<JgpmfQuat>() == core::mem::size_of::<ParserQuat>());
    assert!(core::mem::align_of::<JgpmfQuat>() == core::mem::align_of::<ParserQuat>());
};

// --- Status codes ----------------------------------------------------------

/// Return status for every `jgpmf_*` entry point. Zero is success.
#[repr(C)]
pub enum JgpmfStatus {
    JGPMF_OK = 0,
    /// One of the required pointer arguments was null.
    JGPMF_ERR_NULL_ARG = 1,
    /// The byte payload could not be parsed as GPMF.
    JGPMF_ERR_PARSE = 2,
    /// The payload parsed but contained no `DEVC` root container.
    JGPMF_ERR_NO_DEVC = 3,
    /// The sample has no usable `GPS9` data (no fix, missing stream, etc.).
    JGPMF_ERR_NO_GPS9 = 4,
    /// An index argument was out of range.
    JGPMF_ERR_OUT_OF_RANGE = 5,
}

// --- Value types -----------------------------------------------------------

/// Mirror of `gpmf_parser::Gps9`, exposed to C as a plain value type.
#[repr(C)]
pub struct JgpmfGps9 {
    pub fix: u32,
    pub dop: f32,
    pub latitude: f32,
    pub longitude: f32,
    /// Altitude in metres.
    pub altitude: f32,
    /// 2D speed in m/s.
    pub speed_2d: f32,
    /// 3D speed in m/s.
    pub speed_3d: f32,
    pub days_since_2000: f32,
    pub seconds_since_midnight: f32,
}

/// 3-component vector in raw KLV axis order.
#[repr(C)]
pub struct JgpmfVec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Quaternion in raw KLV component order: (w, x, y, z).
#[repr(C)]
pub struct JgpmfQuat {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

// --- Opaque handle ---------------------------------------------------------

/// Opaque parsed-sample handle. Allocated by `jgpmf_sample_parse`, freed by
/// `jgpmf_sample_free`.
/// cbindgen:opaque
#[repr(C)]
pub struct JgpmfSample {
    _opaque: [u8; 0],
}

// --- Helpers ---------------------------------------------------------------

#[inline]
fn from_handle<'a>(sample: *const JgpmfSample) -> Option<&'a GpmfSample> {
    if sample.is_null() {
        return None;
    }
    // Safety: callers must only pass pointers returned by jgpmf_sample_parse;
    // those were created from `Box<GpmfSample>::into_raw` cast to JgpmfSample.
    Some(unsafe { &*(sample as *const GpmfSample) })
}

fn status_of_panic() -> JgpmfStatus {
    JgpmfStatus::JGPMF_ERR_PARSE
}

// --- Lifecycle -------------------------------------------------------------

/// Parses one GPMF sample payload.
///
/// On success, writes an owned handle to `*out_sample` (must be released with
/// `jgpmf_sample_free`) and returns `JGPMF_OK`.
///
/// # Safety
/// `bytes` must point to at least `len` valid bytes for the duration of the
/// call. `out_sample` must point to a writable `JgpmfSample*` slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jgpmf_sample_parse(
    bytes: *const u8,
    len: usize,
    out_sample: *mut *mut JgpmfSample,
) -> JgpmfStatus {
    if bytes.is_null() || out_sample.is_null() {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
        let mut cursor = std::io::Cursor::new(slice);
        let klvs = match Klv::from_reader(&mut cursor) {
            Ok(v) => v,
            Err(_) => return JgpmfStatus::JGPMF_ERR_PARSE,
        };
        let Some(devc) = klvs.iter().find(|k| k.header().fourcc().as_str() == "DEVC") else {
            return JgpmfStatus::JGPMF_ERR_NO_DEVC;
        };
        if !matches!(devc.value(), Value::Nested(_)) {
            return JgpmfStatus::JGPMF_ERR_NO_DEVC;
        }
        let sample = GpmfSample::new(devc);
        let boxed = Box::new(sample);
        unsafe { *out_sample = Box::into_raw(boxed) as *mut JgpmfSample };
        JgpmfStatus::JGPMF_OK
    }));
    match result {
        Ok(s) => s,
        Err(_) => status_of_panic(),
    }
}

/// Releases a handle returned by `jgpmf_sample_parse`. Passing null is a no-op.
///
/// # Safety
/// `sample` must be a pointer previously returned by `jgpmf_sample_parse` and
/// not yet freed. After this call the pointer is invalid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jgpmf_sample_free(sample: *mut JgpmfSample) {
    if sample.is_null() {
        return;
    }
    // Discard a possible panic in Drop.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let _ = unsafe { Box::from_raw(sample as *mut GpmfSample) };
    }));
}

// --- Getters ---------------------------------------------------------------

/// Copies the parsed `GPS9` value into `*out`. Returns `JGPMF_OK` on success,
/// `JGPMF_ERR_NO_GPS9` if the sample has `fix == 0` (no GPS fix).
///
/// # Safety
/// `out` must point to a writable `JgpmfGps9` slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jgpmf_sample_get_gps9(
    sample: *const JgpmfSample,
    out: *mut JgpmfGps9,
) -> JgpmfStatus {
    if out.is_null() {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    }
    let Some(s) = from_handle(sample) else {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let g = s.gps9();
        if g.fix == 0 {
            return JgpmfStatus::JGPMF_ERR_NO_GPS9;
        }
        unsafe {
            *out = JgpmfGps9 {
                fix: g.fix,
                dop: g.dop,
                latitude: g.latitude,
                longitude: g.longitude,
                altitude: g.altitude,
                speed_2d: g.speed_2d,
                speed_3d: g.speed_3d,
                days_since_2000: g.days_since_2000,
                seconds_since_midnight: g.seconds_since_midnight,
            };
        }
        JgpmfStatus::JGPMF_OK
    }));
    match result {
        Ok(s) => s,
        Err(_) => status_of_panic(),
    }
}

/// Common body for vector-stream getters. Aliases the parser-owned slice
/// into the C-side pointer types via guaranteed-identical repr(C) layout.
unsafe fn export_vec3(
    slice: &[ParserVec3],
    out_ptr: *mut *const JgpmfVec3,
    out_count: *mut usize,
) -> JgpmfStatus {
    if out_ptr.is_null() || out_count.is_null() {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    }
    unsafe {
        *out_ptr = slice.as_ptr() as *const JgpmfVec3;
        *out_count = slice.len();
    }
    JgpmfStatus::JGPMF_OK
}

unsafe fn export_quat(
    slice: &[ParserQuat],
    out_ptr: *mut *const JgpmfQuat,
    out_count: *mut usize,
) -> JgpmfStatus {
    if out_ptr.is_null() || out_count.is_null() {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    }
    unsafe {
        *out_ptr = slice.as_ptr() as *const JgpmfQuat;
        *out_count = slice.len();
    }
    JgpmfStatus::JGPMF_OK
}

/// Borrows the sample's accelerometer array (m/s² after SCAL). The pointer
/// remains valid until `jgpmf_sample_free` is called on the same handle.
/// An empty stream produces `*out_count == 0` and `*out_ptr` unspecified.
///
/// # Safety
/// `out_ptr` / `out_count` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jgpmf_sample_accl(
    sample: *const JgpmfSample,
    out_ptr: *mut *const JgpmfVec3,
    out_count: *mut usize,
) -> JgpmfStatus {
    let Some(s) = from_handle(sample) else {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    };
    unsafe { export_vec3(s.accl(), out_ptr, out_count) }
}

/// Borrows the sample's gyroscope array (rad/s after SCAL).
///
/// # Safety
/// See [`jgpmf_sample_accl`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jgpmf_sample_gyro(
    sample: *const JgpmfSample,
    out_ptr: *mut *const JgpmfVec3,
    out_count: *mut usize,
) -> JgpmfStatus {
    let Some(s) = from_handle(sample) else {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    };
    unsafe { export_vec3(s.gyro(), out_ptr, out_count) }
}

/// Borrows the sample's gravity-vector array.
///
/// # Safety
/// See [`jgpmf_sample_accl`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jgpmf_sample_grav(
    sample: *const JgpmfSample,
    out_ptr: *mut *const JgpmfVec3,
    out_count: *mut usize,
) -> JgpmfStatus {
    let Some(s) = from_handle(sample) else {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    };
    unsafe { export_vec3(s.grav(), out_ptr, out_count) }
}

/// Borrows the sample's camera-orientation quaternion array.
///
/// # Safety
/// See [`jgpmf_sample_accl`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jgpmf_sample_cori(
    sample: *const JgpmfSample,
    out_ptr: *mut *const JgpmfQuat,
    out_count: *mut usize,
) -> JgpmfStatus {
    let Some(s) = from_handle(sample) else {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    };
    unsafe { export_quat(s.cori(), out_ptr, out_count) }
}

/// Borrows the sample's image-orientation quaternion array.
///
/// # Safety
/// See [`jgpmf_sample_accl`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jgpmf_sample_iori(
    sample: *const JgpmfSample,
    out_ptr: *mut *const JgpmfQuat,
    out_count: *mut usize,
) -> JgpmfStatus {
    let Some(s) = from_handle(sample) else {
        return JgpmfStatus::JGPMF_ERR_NULL_ARG;
    };
    unsafe { export_quat(s.iori(), out_ptr, out_count) }
}

/// Library semantic version. Any out-parameter may be null.
///
/// # Safety
/// Each non-null pointer must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jgpmf_version(
    major: *mut u32,
    minor: *mut u32,
    patch: *mut u32,
) {
    let v_major: u32 = env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap_or(0);
    let v_minor: u32 = env!("CARGO_PKG_VERSION_MINOR").parse().unwrap_or(0);
    let v_patch: u32 = env!("CARGO_PKG_VERSION_PATCH").parse().unwrap_or(0);
    if !major.is_null() {
        unsafe { *major = v_major };
    }
    if !minor.is_null() {
        unsafe { *minor = v_minor };
    }
    if !patch.is_null() {
        unsafe { *patch = v_patch };
    }
}
