pub mod klv;

pub use klv::Klv;

use byteorder::{BigEndian, ReadBytesExt as _};

#[cfg(feature = "time")]
use time::{OffsetDateTime, Duration, Date, Month, Time};

use klv::Value;

#[derive(Debug, Clone)]
pub struct GpmfSample {
    klvs: Vec<Klv>,
    gps9: Gps9,
    accl: Vec<Vec3>,
    gyro: Vec<Vec3>,
    grav: Vec<Vec3>,
    cori: Vec<Quat>,
    iori: Vec<Quat>,
}

/// `GPS9` value, introduced in _GoPro HERO11_.
#[derive(Debug, Clone, Copy)]
pub struct Gps9 {
    /// GPS fix (0, 2D or 3D).
    ///
    /// If `0``, other values should be considered invalid and disregarded.
    pub fix: u32,
    /// [DOP(dilution of precision)](https://en.wikipedia.org/wiki/Dilution_of_precision_(navigation)).
    pub dop: f32,
    pub latitude: f32,
    pub longitude: f32,
    /// Altitude in _m_.
    pub altitude: f32,
    /// 2D speed in _m/s_.
    pub speed_2d: f32,
    /// 3D speed in _m/s_.
    pub speed_3d: f32,
    pub days_since_2000: f32,
    pub seconds_since_midnight: f32,
}

/// 3-component vector used for IMU and gravity streams.
///
/// Axis order is taken verbatim from the source KLV — GoPro's published axis
/// conventions (e.g. `ACCL` reading `z, x, y`) are *not* re-mapped here.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Quaternion used for `CORI` (camera orientation) and `IORI` (image
/// orientation) streams. Components are in raw KLV order: `w, x, y, z`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Quat {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl GpmfSample {
    pub fn klvs(&self) -> &[Klv] {
        &self.klvs
    }

    pub fn gps9(&self) -> &Gps9 {
        &self.gps9
    }

    /// Accelerometer samples (typically ~200 Hz, in _m/s²_ after SCAL is applied).
    /// Empty slice when the source has no `ACCL` stream.
    pub fn accl(&self) -> &[Vec3] {
        &self.accl
    }

    /// Gyroscope samples (typically ~400 Hz, in _rad/s_ after SCAL is applied).
    pub fn gyro(&self) -> &[Vec3] {
        &self.gyro
    }

    /// Gravity vector samples.
    pub fn grav(&self) -> &[Vec3] {
        &self.grav
    }

    /// Camera orientation quaternion samples.
    pub fn cori(&self) -> &[Quat] {
        &self.cori
    }

    /// Image orientation quaternion samples.
    pub fn iori(&self) -> &[Quat] {
        &self.iori
    }
}

impl Gps9 {
    /// Converts the GPS timestamp to [`time::OffsetDateTime`].
    ///
    /// Returns `None` if the conversion fails due to invalid values.
    #[cfg(feature = "time")]
    pub fn to_datetime(&self) -> Option<OffsetDateTime> {
        // GPS epoch is January 1, 2000
        let gps_epoch = Date::from_calendar_date(2000, Month::January, 1).ok()?;

        // Add days since 2000
        let date = gps_epoch + Duration::days(self.days_since_2000 as i64);

        // Convert seconds since midnight to time
        let total_seconds = self.seconds_since_midnight as u64;
        let hours = (total_seconds / 3600) as u8;
        let minutes = ((total_seconds % 3600) / 60) as u8;
        let seconds = (total_seconds % 60) as u8;
        let nanoseconds = ((self.seconds_since_midnight.fract()) * 1_000_000_000.0) as u32;

        let time = Time::from_hms_nano(hours, minutes, seconds, nanoseconds).ok()?;

        // Combine date and time (assuming UTC offset)
        date.with_time(time).assume_utc().into()
    }
}

impl GpmfSample {
    /// ## Panics
    /// - If the given KLV is not a nested `DEVC` one.
    /// - If the `DEVC` KLV does not contain a `STRM` KLV with a valid `GPS9` KLV.
    pub fn new(devc_klv: &Klv) -> Self {
        assert_eq!(devc_klv.header().fourcc().as_str(), "DEVC");

        let Value::Nested(child_klvs) = devc_klv.value() else {
            panic!("DEVC KLV with Nested value is expected.")
        };

        let gps9 = {
            let strm_klv = child_klvs
                .iter()
                .filter(|klv| klv.header().fourcc().as_str() == "STRM")
                .find(|klv| {
                    let Value::Nested(strm_klvs) = klv.value() else {
                        panic!("STRM KLV with Nested value is expected.")
                    };

                    strm_klvs
                        .iter()
                        .any(|klv| klv.header().fourcc().as_str() == "GPS9")
                });

            let Value::Nested(strm_child_klvs) = strm_klv.unwrap().value() else {
                panic!("STRM KLV with Nested value is expected.")
            };

            let gps9_klv = strm_child_klvs
                .iter()
                .find(|klv| klv.header().fourcc().as_str() == "GPS9")
                .unwrap();

            let Value::Complex(complex_value) = gps9_klv.value() else {
                panic!("GPS9 KLV with Complex value is expected.")
            };

            {
                let type_klv = strm_child_klvs
                    .iter()
                    .find(|klv| klv.header().fourcc().as_str() == "TYPE")
                    .unwrap();
                let Value::Ascii(type_str) = type_klv.value() else {
                    panic!("TYPE KLV with Ascii value is expected.")
                };

                assert_eq!(type_str, "lllllllSS");
            }

            let scal_values = {
                let scal_klv = strm_child_klvs
                    .iter()
                    .find(|klv| klv.header().fourcc().as_str() == "SCAL")
                    .unwrap();
                let Value::S32(scal_values) = scal_klv.value() else {
                    panic!("SCAL KLV with S32 values is expected.")
                };

                assert_eq!(scal_values.len(), 9);

                scal_values
            };

            let mut reader = std::io::Cursor::new(complex_value.raw_data());

            let latitude = reader.read_i32::<BigEndian>().unwrap();
            let longitude = reader.read_i32::<BigEndian>().unwrap();
            let altitude = reader.read_i32::<BigEndian>().unwrap();
            let speed_2d = reader.read_i32::<BigEndian>().unwrap();
            let speed_3d = reader.read_i32::<BigEndian>().unwrap();
            let days_since_2000 = reader.read_i32::<BigEndian>().unwrap();
            let seconds_since_midnight = reader.read_i32::<BigEndian>().unwrap();
            let dop = reader.read_u16::<BigEndian>().unwrap();
            let fix = reader.read_u16::<BigEndian>().unwrap();

            let latitude = latitude as f32 / scal_values[0] as f32;
            let longitude = longitude as f32 / scal_values[1] as f32;
            let altitude = altitude as f32 / scal_values[2] as f32;
            let speed_2d = speed_2d as f32 / scal_values[3] as f32;
            let speed_3d = speed_3d as f32 / scal_values[4] as f32;
            let days_since_2000 = days_since_2000 as f32 / scal_values[5] as f32;
            let seconds_since_midnight = seconds_since_midnight as f32 / scal_values[6] as f32;
            let dop = dop as f32 / scal_values[7] as f32;
            let fix = (fix as f32 / scal_values[8] as f32) as u32;

            Gps9 {
                fix,
                dop,
                latitude,
                longitude,
                altitude,
                speed_2d,
                speed_3d,
                days_since_2000,
                seconds_since_midnight,
            }
        };

        let accl = extract_vec3(child_klvs, "ACCL").unwrap_or_default();
        let gyro = extract_vec3(child_klvs, "GYRO").unwrap_or_default();
        let grav = extract_vec3(child_klvs, "GRAV").unwrap_or_default();
        let cori = extract_quat(child_klvs, "CORI").unwrap_or_default();
        let iori = extract_quat(child_klvs, "IORI").unwrap_or_default();

        GpmfSample {
            klvs: child_klvs.clone(),
            gps9,
            accl,
            gyro,
            grav,
            cori,
            iori,
        }
    }
}

/// Reads SCAL values as `f32` regardless of underlying numeric type. Returns
/// `None` if the SCAL value isn't one of the numeric types GPMF actually uses.
fn scal_as_f32(value: &Value) -> Option<Vec<f32>> {
    match value {
        Value::S16(v) => Some(v.iter().map(|&x| x as f32).collect()),
        Value::U16(v) => Some(v.iter().map(|&x| x as f32).collect()),
        Value::S32(v) => Some(v.iter().map(|&x| x as f32).collect()),
        Value::U32(v) => Some(v.iter().map(|&x| x as f32).collect()),
        Value::F32(v) => Some(v.clone()),
        _ => None,
    }
}

/// Returns the scale divisor for axis `i`, taking into account that a SCAL
/// with a single entry broadcasts to all axes.
fn scal_for_axis(scal: &[f32], i: usize) -> f32 {
    if scal.len() == 1 { scal[0] } else { scal[i] }
}

/// Finds a STRM block in `child_klvs` whose nested children contain a KLV
/// with the given `fourcc`. Returns the STRM's child KLV list.
fn find_strm_for<'a>(child_klvs: &'a [Klv], fourcc: &str) -> Option<&'a [Klv]> {
    for klv in child_klvs {
        if klv.header().fourcc().as_str() != "STRM" {
            continue;
        }
        let Value::Nested(strm_children) = klv.value() else { continue };
        if strm_children.iter().any(|k| k.header().fourcc().as_str() == fourcc) {
            return Some(strm_children);
        }
    }
    None
}

/// Generic extractor for STRM blocks whose data KLV is an `S16` array of
/// `axis_count`-tuples, scaled per-axis by the sibling SCAL. Returns `None`
/// when the stream isn't present or doesn't match the expected shape.
fn extract_s16_axes(
    child_klvs: &[Klv],
    fourcc: &str,
    axis_count: usize,
) -> Option<Vec<Vec<f32>>> {
    let strm_children = find_strm_for(child_klvs, fourcc)?;

    let data_klv = strm_children.iter().find(|k| k.header().fourcc().as_str() == fourcc)?;
    let Value::S16(raw) = data_klv.value() else { return None };
    if raw.len() % axis_count != 0 {
        return None;
    }

    let scal = strm_children
        .iter()
        .find(|k| k.header().fourcc().as_str() == "SCAL")
        .and_then(|k| scal_as_f32(k.value()))?;
    if scal.len() != 1 && scal.len() != axis_count {
        return None;
    }

    let sample_count = raw.len() / axis_count;
    let mut out: Vec<Vec<f32>> = Vec::with_capacity(sample_count);
    for s in 0..sample_count {
        let mut tuple: Vec<f32> = Vec::with_capacity(axis_count);
        for a in 0..axis_count {
            let v = raw[s * axis_count + a] as f32 / scal_for_axis(&scal, a);
            tuple.push(v);
        }
        out.push(tuple);
    }
    Some(out)
}

fn extract_vec3(child_klvs: &[Klv], fourcc: &str) -> Option<Vec<Vec3>> {
    let tuples = extract_s16_axes(child_klvs, fourcc, 3)?;
    Some(tuples.into_iter().map(|t| Vec3 { x: t[0], y: t[1], z: t[2] }).collect())
}

fn extract_quat(child_klvs: &[Klv], fourcc: &str) -> Option<Vec<Quat>> {
    let tuples = extract_s16_axes(child_klvs, fourcc, 4)?;
    Some(tuples.into_iter().map(|t| Quat { w: t[0], x: t[1], y: t[2], z: t[3] }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse_sample(bytes: &[u8]) -> GpmfSample {
        let klvs = Klv::from_reader(&mut Cursor::new(bytes)).unwrap();
        let devc = klvs.iter().find(|k| k.header().fourcc().as_str() == "DEVC").unwrap();
        GpmfSample::new(devc)
    }

    #[test]
    fn imu_extraction_from_sample_60() {
        let bytes = include_bytes!("../test_files/sample_60.bin");
        let sample = parse_sample(bytes);

        assert!(!sample.accl().is_empty(), "ACCL should be present in sample_60.bin");
        assert!(!sample.gyro().is_empty(), "GYRO should be present in sample_60.bin");

        let grav_mag_plausible = sample.grav().iter().any(|v| {
            let m = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
            m > 0.5 && m < 1.5
        });
        if !sample.grav().is_empty() {
            assert!(grav_mag_plausible, "GRAV magnitude should be ~1 (unit gravity vector)");
        }

        for q in sample.cori() {
            let n = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
            assert!((n - 1.0).abs() < 0.1, "CORI quaternion should be near-unit, got {n}");
        }
    }
}
