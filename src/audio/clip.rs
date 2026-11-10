use anyhow::{Context, Result, bail};
use rust_i18n::t;
use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as DecodeError,
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

pub struct Audio {
    samples: Vec<i16>,
    channels: usize,
    pub(super) rate: u32,
    pub peaks: Arc<[f32]>,
}

impl Audio {
    pub fn load(path: &Path) -> Result<Self> {
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
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;
        let mut samples = Vec::new();
        let mut spec = None;
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
            let mut buffer = SampleBuffer::<i16>::new(audio.capacity() as u64, current);
            buffer.copy_interleaved_ref(audio);
            samples.extend_from_slice(buffer.samples());
        }
        let spec = spec.context(t!("empty_audio").into_owned())?;
        let channels = spec.channels.count();
        let frames = samples.len() / channels;
        if frames == 0 {
            bail!(t!("empty_audio").into_owned());
        }
        let bars = frames.min(96);
        let peaks = (0..bars)
            .map(|i| {
                samples[i * frames / bars * channels..(i + 1) * frames / bars * channels]
                    .iter()
                    .map(|sample| sample.unsigned_abs())
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
        self.samples.len() / self.channels
    }

    pub fn duration(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate)
    }

    pub fn mono(&self, rate: u32) -> Vec<f32> {
        let ratio = f64::from(self.rate) / f64::from(rate);
        let frames = self.frames();
        let count = (frames as f64 / ratio).ceil() as usize;
        (0..count)
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
    }

    pub(super) fn sample(&self, frame: usize, channel: usize, output_channels: usize) -> f32 {
        let start = frame * self.channels;
        if output_channels == 1 {
            self.samples[start..start + self.channels]
                .iter()
                .map(|&sample| f32::from(sample) / 32768.)
                .sum::<f32>()
                / self.channels as f32
        } else {
            f32::from(self.samples[start + channel % self.channels]) / 32768.
        }
    }
}
