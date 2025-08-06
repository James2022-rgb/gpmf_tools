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

impl GpmfSample {
    pub fn klvs(&self) -> &[Klv] {
        &self.klvs
    }

    pub fn gps9(&self) -> &Gps9 {
        &self.gps9
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

        GpmfSample {
            klvs: child_klvs.clone(),
            gps9,
        }
    }
}
