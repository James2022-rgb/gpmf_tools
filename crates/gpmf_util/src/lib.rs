
#[derive(Debug)]
pub struct GpmfTrack {
    gpmf_sample_infos: Vec<GpmfSampleInfo>,
}

#[derive(Debug)]
pub struct GpmfSampleInfo {
    sample: gpmf_parser::GpmfSample,
    #[cfg(feature = "mp4")]
    mp4_sample_info: Option<Mp4SampleInfo>,
}

impl GpmfTrack {
    pub fn gpmf_sample_infos(&self) -> &[GpmfSampleInfo] {
        &self.gpmf_sample_infos
    }

    #[cfg(feature = "mp4")]
    pub fn from_mp4_reader<R: std::io::Read + std::io::Seek>(
        mp4_reader: &mut mp4::Mp4Reader<R>,
        track_id: u32,
    ) -> Result<Self, String> {
        let sample_count = mp4_reader.sample_count(track_id)
            .map_err(|e| format!("Failed to get sample count for track {}: {}", track_id, e))?;

        let mut gpmf_sample_infos = Vec::with_capacity(sample_count as usize);
        for sample_idx in 0..sample_count {
            let sample_id = sample_idx + 1;

            let mp4_sample = mp4_reader.read_sample(track_id, sample_id)
                .map_err(|e| format!("Failed to read sample {} for track {}: {}", sample_id, track_id, e))?;
            let mp4_sample = mp4_sample.ok_or_else(|| format!("Sample {} for track {} does not exist", sample_id, track_id))?;

            let gpmf_sample_info = GpmfSampleInfo::from_mp4_sample(&mp4_sample)
                .map_err(|e| format!("Failed to create GPMF sample info from MP4 sample {}: {}", sample_id, e))?;
            gpmf_sample_infos.push(gpmf_sample_info);
        }

        Ok(Self { gpmf_sample_infos })
    }

    #[cfg(feature = "mp4")]
    pub fn find_nearest_sample(&self, time_ms: u64) -> Option<&GpmfSampleInfo> {
        let idx = self
            .gpmf_sample_infos
            .binary_search_by(|probe| probe.mp4_sample_info.as_ref().unwrap().start_time.cmp(&time_ms));
        match idx {
            Ok(idx) => Some(&self.gpmf_sample_infos[idx]),
            Err(idx) => {
                if idx == 0 {
                    None
                } else {
                    Some(&self.gpmf_sample_infos[idx - 1])
                }
            }
        }
    }

    #[cfg(feature = "gpx")]
    pub fn write_gpx<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use gpx::{Gpx, GpxVersion, Waypoint, Fix, Track as GpxTrack, TrackSegment as GpxTrackSegment};
        use geo_types::Point;

        let mut waypoints: Vec<Waypoint> = Default::default();
        for sample_info in self.gpmf_sample_infos() {
            let gps9 = sample_info.gpmf_sample().gps9();

            if gps9.fix == 0 {
                continue; // Skip samples without GPS fix
            }

            let point = Point::new(gps9.longitude as f64, gps9.latitude as f64);

            let time = gpx::Time::from(gps9.to_datetime().unwrap());

            let fix = match gps9.fix {
                2 => Fix::TwoDimensional,
                3 => Fix::ThreeDimensional,
                _ => Fix::None,
            };

            let mut waypoint = Waypoint::new(point);
            waypoint.elevation = Some(gps9.altitude as f64);
            waypoint.time = Some(time);
            waypoint.speed = Some(gps9.speed_2d as f64);
            waypoint.fix = Some(fix);
            waypoint.pdop = Some(gps9.dop as f64);
            waypoints.push(waypoint);
        }

        let mut gpx_track = GpxTrack::new();
        gpx_track.segments.push(GpxTrackSegment {
            points: waypoints,
        });

        let gpx = Gpx {
            version: GpxVersion::Gpx10,
            creator: Some("gpmf_tools".to_string()),
            metadata: None,
            waypoints: vec![],
            tracks: vec![gpx_track],
            routes: vec![],
        };

        gpx::write(&gpx, writer)?;
        Ok(())
    }
}

impl GpmfSampleInfo {
    pub fn gpmf_sample(&self) -> &gpmf_parser::GpmfSample {
        &self.sample
    }
}

#[cfg(feature = "mp4")]
#[derive(Debug)]
struct Mp4SampleInfo {
    start_time: u64,
    duration: u32,
    rendering_offset: i32,
    is_sync: bool,
}

impl GpmfSampleInfo {
    fn from_mp4_sample(mp4_sample: &mp4::Mp4Sample) -> Result<Self, String> {
        let mp4_sample_info = Mp4SampleInfo {
            start_time: mp4_sample.start_time,
            duration: mp4_sample.duration,
            rendering_offset: mp4_sample.rendering_offset,
            is_sync: mp4_sample.is_sync,
        };

        Self::from_bytes(&mp4_sample.bytes, Some(mp4_sample_info))
    }

    fn from_bytes(
        bytes: &[u8],
        mp4_sample_info: Option<Mp4SampleInfo>,
    ) -> Result<Self, String> {
        let klvs = gpmf_parser::Klv::from_reader(&mut std::io::Cursor::new(bytes))
            .map_err(|e| format!("Failed to parse GPMF KLVs: {}", e))?;

        let devc_klv = klvs
            .iter()
            .find(|klv| klv.header().fourcc().as_str() == "DEVC")
            .ok_or("DEVC KLV not found")?;

        let sample = gpmf_parser::GpmfSample::new(devc_klv);

        Ok(Self {
            sample,
            mp4_sample_info,
        })
    }
}
