use anyhow::{Context, Result, bail};
use rust_i18n::t;
use std::fs::File;
use std::io::{BufWriter, ErrorKind, Write};
use std::path::Path;
use std::sync::Arc;
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as DecodeError,
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

pub struct Audio {
    samples: memmap2::Mmap,
    channels: usize,
    pub(super) rate: u32,
    pub peaks: Arc<[f32]>,
}

impl Audio {
    pub fn load(path: &Path, report: impl Fn(u8) -> Result<()>) -> Result<Self> {
        let source = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
        let mut format = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )?
            .format;
        let track = format
            .default_track()
            .context(t!("track_missing").into_owned())?;
        let track_id = track.id;
        let total_frames = track.codec_params.n_frames.filter(|&frames| frames > 0);
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;
        let file = tempfile::tempfile()?;
        let mut pcm = BufWriter::new(&file);
        let mut sample_count = 0;
        let mut spec = None;
        let mut buffer: Option<SampleBuffer<i16>> = None;
        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(DecodeError::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(error) => return Err(error.into()),
            };
            if packet.track_id() != track_id {
                continue;
            }
            let audio = decoder.decode(&packet)?;
            let current = *audio.spec();
            if spec.is_some_and(|previous| previous != current) {
                bail!(t!("format_changes").into_owned());
            }
            spec = Some(current);
            let capacity = audio.capacity();
            if buffer
                .as_ref()
                .is_none_or(|buffer| buffer.capacity() / current.channels.count() < capacity)
            {
                buffer = Some(SampleBuffer::new(capacity as u64, current));
            }
            let buffer = buffer.as_mut().unwrap();
            buffer.copy_interleaved_ref(audio);
            let samples = buffer.samples();
            // Initialized i16 samples can be read as bytes for one write per packet.
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    samples.as_ptr().cast::<u8>(),
                    std::mem::size_of_val(samples),
                )
            };
            pcm.write_all(bytes)?;
            sample_count += samples.len();
            report(total_frames.map_or(0, |total| {
                let frames = sample_count / current.channels.count();
                (frames as f64 * 100. / total as f64).min(99.) as u8
            }))?;
        }
        let spec = spec.context(t!("empty_audio").into_owned())?;
        let channels = spec.channels.count();
        let frames = sample_count / channels;
        if frames == 0 {
            bail!(t!("empty_audio").into_owned());
        }
        pcm.flush()?;
        drop(pcm);
        // The anonymous file is private and is never modified after mapping.
        let samples = unsafe { memmap2::MmapOptions::new().map(&file)? };
        // Use a denser overview without scanning every sample.
        let bars = frames.min(192);
        let peaks = (0..bars)
            .map(|i| {
                let start = i * frames / bars;
                let end = (i + 1) * frames / bars;
                // Sample short blocks across the bin, keeping both stereo channels.
                (start..end)
                    .step_by(64)
                    .flat_map(|frame| {
                        let start = frame * channels * 2;
                        let end = ((frame + 8).min(end)) * channels * 2;
                        samples[start..end]
                            .chunks_exact(2)
                            .map(|pair| i16::from_ne_bytes([pair[0], pair[1]]).unsigned_abs())
                    })
                    .max()
                    .unwrap_or(0) as f32
                    / 32768.
            })
            .collect();
        Ok(Self {
            samples,
            channels,
            rate: spec.rate,
            peaks,
        })
    }

    pub(super) fn frames(&self) -> usize {
        self.samples.len() / 2 / self.channels
    }

    pub fn duration(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate)
    }

    pub fn mono_chunks(&self, rate: u32, chunk_size: usize) -> impl Iterator<Item = Vec<f32>> + '_ {
        self.mono_range(rate, chunk_size, 0., self.duration())
    }

    pub fn mono_range(
        &self,
        rate: u32,
        chunk_size: usize,
        start: f64,
        end: f64,
    ) -> impl Iterator<Item = Vec<f32>> + '_ {
        let ratio = f64::from(self.rate) / f64::from(rate);
        let frames = self.frames();
        let count = (frames as f64 / ratio).ceil() as usize;
        let start = ((start * f64::from(rate)) as usize).min(count);
        let end = ((end * f64::from(rate)).ceil() as usize).min(count);
        (start..end).step_by(chunk_size).map(move |offset| {
            (offset..offset.saturating_add(chunk_size).min(end))
                .map(|index| {
                    let position = index as f64 * ratio;
                    let start = (position as usize).min(frames - 1);
                    if ratio > 1. {
                        let end = (((index + 1) as f64 * ratio) as usize).clamp(start + 1, frames);
                        (start..end)
                            .map(|frame| self.sample(frame, 0, 1))
                            .sum::<f32>()
                            / (end - start) as f32
                    } else {
                        let left = self.sample(start, 0, 1);
                        let right = self.sample((start + 1).min(frames - 1), 0, 1);
                        left + (right - left) * position.fract() as f32
                    }
                })
                .collect()
        })
    }

    pub(super) fn sample(&self, frame: usize, channel: usize, output_channels: usize) -> f32 {
        let start = frame * self.channels;
        let read = |index| {
            let offset: usize = index * 2;
            f32::from(i16::from_ne_bytes([
                self.samples[offset],
                self.samples[offset + 1],
            ])) / 32768.
        };
        if output_channels == 1 {
            (start..start + self.channels).map(read).sum::<f32>() / self.channels as f32
        } else {
            read(start + channel % self.channels)
        }
    }
}
