//! GIF stores whole centiseconds. Round elapsed time, not each frame period.

pub struct GifFrameClock {
    fps: u32,
    remainder: u32,
    centiseconds: u64,
}

impl GifFrameClock {
    pub fn new(fps: u32) -> Option<Self> {
        (1..=60).contains(&fps).then_some(Self {
            fps,
            remainder: fps / 2,
            centiseconds: 0,
        })
    }

    pub fn next_delay_ms(&mut self) -> u32 {
        self.remainder += 100;
        let ticks = self.remainder / self.fps;
        self.remainder %= self.fps;
        self.centiseconds += u64::from(ticks);
        ticks * 10
    }

    pub fn duration_seconds(&self) -> f64 {
        self.centiseconds as f64 / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::AnimationDecoder;

    #[test]
    fn elapsed_rounding_stays_within_half_a_centisecond_at_every_frame() {
        for fps in [1, 7, 12, 24, 30, 60] {
            let mut clock = GifFrameClock::new(fps).unwrap();
            for frames in 1..=12_001 {
                let delay = clock.next_delay_ms();
                assert!(delay > 0 && delay % 10 == 0);
                let expected = f64::from(frames) / f64::from(fps);
                assert!((clock.duration_seconds() - expected).abs() <= 0.005_000_001);
            }
        }
        for invalid in [0, 61, u32::MAX] {
            assert!(GifFrameClock::new(invalid).is_none());
        }
    }

    #[test]
    fn actual_long_gif_delays_and_metadata_keep_the_1850_frame_timing() {
        let mut bytes = Vec::new();
        let mut clock = GifFrameClock::new(12).unwrap();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut bytes, 30);
            for frame in 0..1850 {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(1, 1, image::Rgba([frame as u8, 0, 0, 255])),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(clock.next_delay_ms(), 1),
                    ))
                    .unwrap();
            }
        }
        let decoder =
            image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes.as_slice())).unwrap();
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 1850);
        let mut total_ms = 0u64;
        let mut delays = std::collections::BTreeMap::new();
        for frame in frames {
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            assert_eq!(denominator, 1);
            assert!([80, 90].contains(&numerator));
            total_ms += u64::from(numerator);
            *delays.entry(numerator).or_insert(0u32) += 1;
        }
        assert_eq!(total_ms, 154_170);
        assert_eq!(clock.duration_seconds(), total_ms as f64 / 1000.0);
        assert!((clock.duration_seconds() - 154.156_667).abs() < 1.0 / 12.0);
        if let Some(root) = std::env::var_os("KIRI_LINUX_MEDIA_QA_DIR") {
            let directory = std::path::PathBuf::from(root).join("gif-timing");
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("1850-frame-fixture.gif"), bytes).unwrap();
            std::fs::write(
                directory.join("timing.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "native_gui": false,
                    "kind": "actual Rust GIF encoder fixture, 1x1 pixels",
                    "source_sha": std::env::var("GITHUB_SHA").unwrap_or_default(),
                    "fps": 12, "frames": 1850, "delay_ms_counts": delays,
                    "encoded_duration_ms": total_ms,
                    "metadata_duration_s": clock.duration_seconds()
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }
}
