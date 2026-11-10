use super::Audio;
use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use rust_i18n::t;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

pub struct Player {
    stream: cpal::Stream,
    cursor: Arc<AtomicUsize>,
    failed: Arc<AtomicBool>,
    seek_to: Arc<AtomicUsize>,
    rate: u32,
    pub playing: bool,
}

impl Player {
    pub fn start(audio: Arc<Audio>, position: f64) -> Result<Self> {
        let device = cpal::default_host()
            .default_output_device()
            .context(t!("output_missing").into_owned())?;
        let supported = device.default_output_config()?;
        let config = supported.config();
        let rate = config.sample_rate.0;
        let start = (position * f64::from(rate)) as usize;
        let cursor = Arc::new(AtomicUsize::new(start));
        let seek_to = Arc::new(AtomicUsize::new(start));
        let failed = Arc::new(AtomicBool::new(false));
        macro_rules! output {
            ($($format:ident => $ty:ty),+ $(,)?) => {
                match supported.sample_format() {
                    $(SampleFormat::$format => build::<$ty>(&device, &config, audio, cursor.clone(), failed.clone(), seek_to.clone()),)+
                    format => bail!("{}", t!("output_format_unsupported", detail = format!("{format:?}"))),
                }
            };
        }
        let stream = output!(
            F32 => f32, F64 => f64, I8 => i8, I16 => i16, I24 => cpal::I24,
            I32 => i32, I64 => i64, U8 => u8, U16 => u16, U32 => u32, U64 => u64,
        )?;
        stream.play()?;
        Ok(Self {
            stream,
            cursor,
            failed,
            seek_to,
            rate,
            playing: true,
        })
    }

    pub fn toggle(&mut self) -> Result<()> {
        if self.playing {
            self.stream.pause()?;
        } else {
            self.stream.play()?;
        }
        self.playing = !self.playing;
        Ok(())
    }

    pub fn position(&self) -> f64 {
        let pending = self.seek_to.load(Relaxed);
        let frames = if pending == usize::MAX {
            self.cursor.load(Relaxed)
        } else {
            pending
        };
        frames as f64 / f64::from(self.rate)
    }

    pub fn seek(&self, seconds: f64) {
        self.seek_to
            .store((seconds * f64::from(self.rate)) as usize, Relaxed);
    }

    pub fn failed(&self) -> bool {
        self.failed.load(Relaxed)
    }
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio: Arc<Audio>,
    cursor: Arc<AtomicUsize>,
    failed: Arc<AtomicBool>,
    seek_to: Arc<AtomicUsize>,
) -> Result<cpal::Stream> {
    let channels = usize::from(config.channels);
    let ratio = f64::from(audio.rate) / f64::from(config.sample_rate.0);
    let mut offset = 0;
    Ok(device.build_output_stream(
        config,
        move |output: &mut [T], _| {
            let pending = seek_to.swap(usize::MAX, Relaxed);
            if pending != usize::MAX {
                offset = pending;
            }
            for frame in output.chunks_exact_mut(channels) {
                let position = offset as f64 * ratio;
                let index = position as usize;
                if index >= audio.frames() {
                    frame.fill(T::from_sample_(0.0));
                    continue;
                }
                let next = (index + 1).min(audio.frames() - 1);
                let fraction = position.fract() as f32;
                for (channel, sample) in frame.iter_mut().enumerate() {
                    let left = audio.sample(index, channel, channels);
                    let right = audio.sample(next, channel, channels);
                    *sample = T::from_sample_(left + (right - left) * fraction);
                }
                offset += 1;
            }
            cursor.store(offset, Relaxed);
        },
        move |_| failed.store(true, Relaxed),
        None,
    )?)
}
