//! Native slice-speed rendering. PCM resampling intentionally changes pitch.
//! Input is the already edited, ordered master, so effects stay in source time.
use super::super::{ExportProgressRange, VideoSegment};
use super::{annotations, storage_file_controlled, ticks, wait_operation, wait_render};
use anyhow::{bail, Context, Result};
use std::{
    io::{BufWriter, Write},
    path::Path,
};
use windows::{
    core::HSTRING,
    Media::{
        Editing::{BackgroundAudioTrack, MediaClip, MediaComposition, MediaTrimmingPreference},
        MediaProperties::MediaEncodingProfile,
        Transcoding::TranscodeFailureReason,
    },
    Win32::Media::MediaFoundation::*,
};

#[derive(Clone, Copy)]
struct Range {
    source_start: i64,
    source_end: i64,
    output_start: i64,
    output_end: i64,
    speed: f64,
}
impl Range {
    fn map(&self, source: i64) -> i64 {
        (self.output_start + ((source - self.source_start) as f64 / self.speed).round() as i64)
            .min(self.output_end)
    }
}

fn ranges(segments: &[VideoSegment]) -> Vec<Range> {
    let (mut source_start, mut output_start) = (0, 0);
    segments
        .iter()
        .map(|segment| {
            let source_end = source_start + ticks(segment.end - segment.start);
            let output_end =
                output_start + ((source_end - source_start) as f64 / segment.speed).round() as i64;
            let range = Range {
                source_start,
                source_end,
                output_start,
                output_end,
                speed: segment.speed,
            };
            source_start = source_end;
            output_start = output_end;
            range
        })
        .collect()
}

pub(super) fn render(
    source: &Path,
    destination: &Path,
    segments: &[VideoSegment],
    profile: &MediaEncodingProfile,
    progress: &ExportProgressRange,
) -> Result<()> {
    progress.check()?;
    let video_progress = progress.child(0.0, 0.5);
    let timeline = ranges(segments);
    let total = timeline.last().context("Speed export has no slices")?;
    let staging = tempfile::Builder::new()
        .prefix("kiri-video-speed-")
        .tempdir()?;
    let video_path = staging.path().join("speed-video.mp4");
    // Keep Media Foundation alive for the video writer and subsequent PCM reader.
    let reader = crate::gif::WindowsVideoReader::open(source)?;
    let (width, height) = reader.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 33_554_432 {
        bail!("Invalid video dimensions for speed export");
    }
    let video = profile.Video()?;
    let mut rate_numerator = video.FrameRate()?.Numerator()?.max(1);
    let mut rate_denominator = video.FrameRate()?.Denominator()?.max(1);
    if u64::from(rate_numerator) > 120 * u64::from(rate_denominator) {
        rate_numerator = 120;
        rate_denominator = 1;
    }
    let writer =
        unsafe { MFCreateSinkWriterFromURL(&HSTRING::from(video_path.as_os_str()), None, None) }?;
    let output_type = unsafe { MFCreateMediaType() }?;
    let input_type = unsafe { MFCreateMediaType() }?;
    unsafe {
        for media_type in [&output_type, &input_type] {
            media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            media_type.SetUINT64(
                &MF_MT_FRAME_SIZE,
                (u64::from(width) << 32) | u64::from(height),
            )?;
            media_type.SetUINT64(
                &MF_MT_FRAME_RATE,
                (u64::from(rate_numerator) << 32) | u64::from(rate_denominator),
            )?;
            media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1_u64 << 32) | 1)?;
            media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        }
        output_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        output_type.SetUINT32(&MF_MT_AVG_BITRATE, video.Bitrate()?.max(500_000))?;
        input_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        let stream = writer.AddStream(&output_type)?;
        writer.SetInputMediaType(stream, &input_type, None)?;
        writer.BeginWriting()?;
        let mut pending = reader
            .read_frame()?
            .context("Speed source has no video frames")?;
        let mut first = true;
        let mut output_index = 0_i64;
        let mut next_output_time = 0_i64;
        loop {
            progress.check()?;
            let next = reader.read_frame()?;
            let start = if first { 0 } else { pending.0.max(0) };
            first = false;
            let end = next
                .as_ref()
                .map(|frame| frame.0)
                .unwrap_or(total.source_end)
                .min(total.source_end);
            for range in &timeline {
                let left = start.max(range.source_start);
                let right = end.min(range.source_end);
                if right <= left {
                    continue;
                }
                let output_end = range.map(right);
                // Keep the encoder at the declared frame rate: repeat slow frames
                // and sample fast frames instead of feeding 4x-dense timestamps
                // into an encoder configured for the source cadence.
                while next_output_time < output_end && next_output_time < total.output_end {
                    progress.check()?;
                    output_index += 1;
                    let next =
                        ((i128::from(output_index) * 10_000_000 * i128::from(rate_denominator)
                            / i128::from(rate_numerator)) as i64)
                            .min(total.output_end);
                    annotations::write_frame(
                        &writer,
                        stream,
                        &pending.1,
                        next_output_time,
                        next - next_output_time,
                    )?;
                    next_output_time = next;
                    video_progress.report(next_output_time as f64 / total.output_end as f64);
                }
            }
            let Some(following) = next else {
                break;
            };
            if following.0 >= total.source_end {
                break;
            }
            if following.0 < pending.0 {
                bail!("Speed decoder returned non-monotonic timestamps");
            }
            pending = following;
        }
        progress.check()?;
        writer
            .Finalize()
            .context("Could not finalize speed-adjusted video")?;
    }
    drop(writer);
    let original = wait_operation(MediaClip::CreateFromFileAsync(&storage_file_controlled(source, progress)?)?, progress)?;
    if original.EmbeddedAudioTracks()?.Size()? == 0 {
        progress.check()?;
        std::fs::rename(&video_path, destination)?;
        progress.report(1.0);
        return Ok(());
    }
    let audio_path = staging.path().join("speed-audio.wav");
    render_audio(source, &audio_path, &timeline, &progress.child(0.5, 0.65))?;
    let composition = MediaComposition::new()?;
    composition
        .Clips()?
        .Append(&wait_operation(MediaClip::CreateFromFileAsync(&storage_file_controlled(&video_path, progress)?)?, progress)?)?;
    composition
        .BackgroundAudioTracks()?
        .Append(&wait_operation(BackgroundAudioTrack::CreateFromFileAsync(&storage_file_controlled(&audio_path, progress)?)?, progress)?)?;
    std::fs::File::create(destination)?;
    let result = wait_render(composition
        .RenderToFileWithProfileAsync(
            &storage_file_controlled(destination, progress)?,
            MediaTrimmingPreference::Precise,
            profile,
        )?, &progress.child(0.65, 1.0))?;
    if result != TranscodeFailureReason::None {
        bail!("Windows cannot combine speed-adjusted audio/video: {result:?}");
    }
    Ok(())
}

/// A sequential PCM cursor holds only the current decoded chunk, including any
/// real leading gaps. All source channels are resampled with the same clock.
struct PcmReader {
    reader: IMFSourceReader,
    rate: u32,
    channels: usize,
    cursor: u64,
    chunk_start: u64,
    chunk: Vec<i16>,
    eof: bool,
    decoded_frames: u64,
}

impl PcmReader {
    fn open(source: &Path) -> Result<Self> {
        let reader =
            unsafe { MFCreateSourceReaderFromURL(&HSTRING::from(source.as_os_str()), None) }?;
        unsafe {
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
            reader.SetStreamSelection(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32, true)?;
            let media = MFCreateMediaType()?;
            media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            media.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            media.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            reader.SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
                None,
                &media,
            )?;
            let actual =
                reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)?;
            let rate = actual.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
            let channels = actual.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)? as usize;
            if rate == 0
                || rate > 384_000
                || channels == 0
                || channels > 8
                || actual.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE)? != 16
            {
                bail!("Unsupported decoded audio format for speed export");
            }
            Ok(Self {
                reader,
                rate,
                channels,
                cursor: 0,
                chunk_start: 0,
                chunk: Vec::new(),
                eof: false,
                decoded_frames: 0,
            })
        }
    }

    fn next_frame(&mut self) -> Result<[i16; 8]> {
        loop {
            let end = self.chunk_start + (self.chunk.len() / self.channels) as u64;
            if self.cursor < self.chunk_start || (self.eof && self.cursor >= end) {
                self.cursor += 1;
                return Ok([0; 8]);
            }
            if self.cursor < end {
                let offset = (self.cursor - self.chunk_start) as usize * self.channels;
                let mut frame = [0; 8];
                frame[..self.channels].copy_from_slice(&self.chunk[offset..offset + self.channels]);
                self.cursor += 1;
                return Ok(frame);
            }
            let (mut flags, mut time, mut sample) = (0, 0, None);
            unsafe {
                self.reader.ReadSample(
                    MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut time),
                    Some(&mut sample),
                )?;
            }
            self.eof = flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0;
            let Some(sample) = sample else {
                continue;
            };
            let buffer = unsafe { sample.ConvertToContiguousBuffer() }?;
            let mut pointer = std::ptr::null_mut();
            let mut length = 0;
            unsafe {
                buffer.Lock(&mut pointer, None, Some(&mut length))?;
            }
            let result = (|| -> Result<Vec<i16>> {
                if pointer.is_null()
                    || length > 16 * 1024 * 1024
                    || length as usize % (self.channels * 2) != 0
                {
                    bail!("Invalid decoded PCM chunk");
                }
                let bytes = unsafe { std::slice::from_raw_parts(pointer, length as usize) };
                Ok(bytes
                    .chunks_exact(2)
                    .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
                    .collect())
            })();
            unsafe {
                buffer.Unlock()?;
            }
            self.chunk = result?;
            self.decoded_frames += (self.chunk.len() / self.channels) as u64;
            self.chunk_start =
                ((time.max(0) as f64 * f64::from(self.rate)) / 10_000_000.0).round() as u64;
        }
    }
}

fn render_audio(source: &Path, destination: &Path, timeline: &[Range], progress: &ExportProgressRange) -> Result<()> {
    progress.check()?;
    let mut reader = PcmReader::open(source)?;
    let total = timeline
        .last()
        .context("Speed audio has no slices")?
        .output_end;
    let output_frames = (total as f64 * f64::from(reader.rate) / 10_000_000.0).round() as u64;
    let data_bytes = output_frames
        .checked_mul(reader.channels as u64 * 2)
        .context("Speed audio is too long")?;
    let data_bytes = u32::try_from(data_bytes)
        .ok()
        .filter(|value| *value <= u32::MAX - 36)
        .context("Speed audio exceeds native WAV staging capacity")?;
    let mut file = BufWriter::new(std::fs::File::create(destination)?);
    file.write_all(b"RIFF")?;
    file.write_all(&(data_bytes + 36).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&(reader.channels as u16).to_le_bytes())?;
    file.write_all(&reader.rate.to_le_bytes())?;
    file.write_all(&(reader.rate * reader.channels as u32 * 2).to_le_bytes())?;
    file.write_all(&(reader.channels as u16 * 2).to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())?;
    let mut left = reader.next_frame()?;
    let mut right = reader.next_frame()?;
    let mut source_index = 0_u64;
    let mut range_index = 0;
    for output_index in 0..output_frames {
        if output_index % 4096 == 0 {
            progress.check()?;
            progress.report(output_index as f64 / output_frames as f64);
        }
        let output_time = output_index as f64 * 10_000_000.0 / f64::from(reader.rate);
        while range_index + 1 < timeline.len()
            && output_time >= timeline[range_index].output_end as f64
        {
            range_index += 1;
        }
        let range = timeline[range_index];
        let source_time =
            range.source_start as f64 + (output_time - range.output_start as f64) * range.speed;
        let position = source_time.max(0.0) * f64::from(reader.rate) / 10_000_000.0;
        let index = position.floor() as u64;
        while source_index < index {
            left = right;
            right = reader.next_frame()?;
            source_index += 1;
        }
        let fraction = position.fract();
        for channel in 0..reader.channels {
            let value = (f64::from(left[channel]) * (1.0 - fraction)
                + f64::from(right[channel]) * fraction)
                .round()
                .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            file.write_all(&value.to_le_bytes())?;
        }
    }
    file.flush()?;
    progress.check()?;
    progress.report(1.0);
    if reader.decoded_frames == 0 {
        bail!("The source audio track contained no decodable PCM; refusing silent speed export");
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn audio_stats(source: &Path, start: f64, end: f64) -> Result<(f64, f64)> {
    let _foundation = crate::gif::WindowsVideoReader::open(source)?;
    let mut reader = PcmReader::open(source)?;
    let start = (start * f64::from(reader.rate)).round() as u64;
    let end = (end * f64::from(reader.rate)).round() as u64;
    let (mut square_sum, mut crossings, mut previous) = (0.0, 0_u64, 0_i16);
    for index in 0..end {
        let frame = reader.next_frame()?;
        if index >= start {
            square_sum += f64::from(frame[0]).powi(2);
            if index > start && previous <= 0 && frame[0] > 0 {
                crossings += 1;
            }
        }
        previous = frame[0];
    }
    let samples = (end - start) as f64;
    Ok((
        (square_sum / samples).sqrt(),
        crossings as f64 * f64::from(reader.rate) / samples,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn speed_map_preserves_requested_order_and_each_boundary() {
        let timeline = ranges(&[
            VideoSegment {
                start: 3.0,
                end: 4.0,
                speed: 0.25,
            },
            VideoSegment {
                start: 0.0,
                end: 1.0,
                speed: 2.0,
            },
            VideoSegment {
                start: 1.0,
                end: 2.0,
                speed: 4.0,
            },
        ]);
        assert_eq!(timeline[0].map(ticks(0.5)), ticks(2.0));
        assert_eq!(timeline[1].map(ticks(1.5)), ticks(4.25));
        assert_eq!(timeline[2].map(ticks(2.5)), ticks(4.625));
        assert_eq!(timeline[2].output_end, ticks(4.75));
        assert_eq!(timeline[0].output_end, timeline[1].output_start);
        assert_eq!(timeline[1].output_end, timeline[2].output_start);
    }
}
