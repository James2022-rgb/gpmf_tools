//! Exercises the `extern "C"` entry points through raw pointer arguments,
//! the same way a C caller would. Uses the bundled GPMF sample binary from
//! `gpmf_parser`.
//!
//! Gated behind the `with-fixtures` feature because the referenced
//! `test_files/sample_60.bin` is intentionally not tracked in git.

#![cfg(feature = "with-fixtures")]

use std::ptr;

use jgpmf_capi::*;

const SAMPLE_BYTES: &[u8] = include_bytes!("../../gpmf_parser/test_files/sample_60.bin");

#[test]
fn parse_and_read_back_full_telemetry() {
    let mut handle: *mut JgpmfSample = ptr::null_mut();
    let status = unsafe {
        jgpmf_sample_parse(SAMPLE_BYTES.as_ptr(), SAMPLE_BYTES.len(), &mut handle)
    };
    assert!(matches!(status, JgpmfStatus::JGPMF_OK));
    assert!(!handle.is_null());

    let mut accl_ptr: *const JgpmfVec3 = ptr::null();
    let mut accl_count: usize = 0;
    let status = unsafe { jgpmf_sample_accl(handle, &mut accl_ptr, &mut accl_count) };
    assert!(matches!(status, JgpmfStatus::JGPMF_OK));
    assert!(accl_count > 0, "ACCL should be present in sample_60.bin");
    assert!(!accl_ptr.is_null());

    let mut gyro_ptr: *const JgpmfVec3 = ptr::null();
    let mut gyro_count: usize = 0;
    let status = unsafe { jgpmf_sample_gyro(handle, &mut gyro_ptr, &mut gyro_count) };
    assert!(matches!(status, JgpmfStatus::JGPMF_OK));
    assert!(gyro_count > 0, "GYRO should be present in sample_60.bin");

    // Grav / cori / iori may or may not be present, but the call must succeed.
    let mut tmp_ptr: *const JgpmfVec3 = ptr::null();
    let mut tmp_count: usize = 0;
    assert!(matches!(
        unsafe { jgpmf_sample_grav(handle, &mut tmp_ptr, &mut tmp_count) },
        JgpmfStatus::JGPMF_OK
    ));

    let mut q_ptr: *const JgpmfQuat = ptr::null();
    let mut q_count: usize = 0;
    assert!(matches!(
        unsafe { jgpmf_sample_cori(handle, &mut q_ptr, &mut q_count) },
        JgpmfStatus::JGPMF_OK
    ));
    assert!(matches!(
        unsafe { jgpmf_sample_iori(handle, &mut q_ptr, &mut q_count) },
        JgpmfStatus::JGPMF_OK
    ));

    // GPS9 may legitimately have no fix in test fixtures; accept either OK or NO_GPS9.
    let mut gps = JgpmfGps9 {
        fix: 0, dop: 0.0, latitude: 0.0, longitude: 0.0, altitude: 0.0,
        speed_2d: 0.0, speed_3d: 0.0, days_since_2000: 0.0, seconds_since_midnight: 0.0,
    };
    let status = unsafe { jgpmf_sample_get_gps9(handle, &mut gps) };
    assert!(matches!(
        status,
        JgpmfStatus::JGPMF_OK | JgpmfStatus::JGPMF_ERR_NO_GPS9
    ));

    unsafe { jgpmf_sample_free(handle) };
}

#[test]
fn null_args_return_null_arg_status() {
    let mut handle: *mut JgpmfSample = ptr::null_mut();
    assert!(matches!(
        unsafe { jgpmf_sample_parse(ptr::null(), 0, &mut handle) },
        JgpmfStatus::JGPMF_ERR_NULL_ARG
    ));
    assert!(matches!(
        unsafe { jgpmf_sample_parse(SAMPLE_BYTES.as_ptr(), SAMPLE_BYTES.len(), ptr::null_mut()) },
        JgpmfStatus::JGPMF_ERR_NULL_ARG
    ));

    // Null sample handle.
    let mut ptr_out: *const JgpmfVec3 = ptr::null();
    let mut count_out: usize = 0;
    assert!(matches!(
        unsafe { jgpmf_sample_accl(ptr::null(), &mut ptr_out, &mut count_out) },
        JgpmfStatus::JGPMF_ERR_NULL_ARG
    ));
}

#[test]
fn parse_rejects_garbage_bytes() {
    let garbage = [0u8; 16];
    let mut handle: *mut JgpmfSample = ptr::null_mut();
    let status = unsafe {
        jgpmf_sample_parse(garbage.as_ptr(), garbage.len(), &mut handle)
    };
    // Either NO_DEVC (parsed-but-no-devc) or PARSE (early termination) is acceptable;
    // crucially, must not return OK and must not crash.
    assert!(matches!(
        status,
        JgpmfStatus::JGPMF_ERR_NO_DEVC | JgpmfStatus::JGPMF_ERR_PARSE
    ));
    assert!(handle.is_null());
}

#[test]
fn version_returns_sensible_values() {
    let mut major: u32 = u32::MAX;
    let mut minor: u32 = u32::MAX;
    let mut patch: u32 = u32::MAX;
    unsafe { jgpmf_version(&mut major, &mut minor, &mut patch) };
    // Crate is at 0.1.0; verify ABI plumbed correctly rather than pinning values.
    assert!(major < 100 && minor < 100 && patch < 100);

    // Null pointers must be silently ignored.
    unsafe { jgpmf_version(ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) };
}

#[test]
fn free_is_null_safe() {
    unsafe { jgpmf_sample_free(ptr::null_mut()) };
}
