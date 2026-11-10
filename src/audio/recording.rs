use anyhow::{Context, Result, anyhow, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use hound::{WavSpec, WavWriter};
use rust_i18n::t;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type Writer = WavWriter<BufWriter<File>>;

#[derive(Default)]
struct Capture {
    writer: Option<Writer>,
    error: Option<String>,
}

pub struct Recording {
    stream: cpal::Stream,
    capture: Arc<Mutex<Capture>>,
    path: tempfile::TempPath,
}

impl Recording {
    pub fn start(directory: &Path, selected: Option<&cpal::Device>) -> Result<Self> {
        let device = selected
            .cloned()
            .or_else(|| cpal::default_host().default_input_device())
            .context(t!("microphone_missing").into_owned())?;
        let supported = device.default_input_config()?;
        let config = supported.config();
        std::fs::create_dir_all(directory)?;
        let (file, path) = tempfile::Builder::new()
            .prefix("recording-")
            .suffix(".wav")
            .tempfile_in(directory)?
            .into_parts();
        let spec = WavSpec {
            channels: config.channels,
            sample_rate: config.sample_rate.0,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let capture = Arc::new(Mutex::new(Capture {
            writer: Some(WavWriter::new(BufWriter::new(file), spec)?),
            error: None,
        }));
        let stream = match supported.sample_format() {
            SampleFormat::I8 => stream::<i8>(&device, &config, &capture),
            SampleFormat::I24 => stream::<cpal::I24>(&device, &config, &capture),
            SampleFormat::I64 => stream::<i64>(&device, &config, &capture),
            SampleFormat::U8 => stream::<u8>(&device, &config, &capture),
            SampleFormat::U32 => stream::<u32>(&device, &config, &capture),
            SampleFormat::U64 => stream::<u64>(&device, &config, &capture),
            SampleFormat::F64 => stream::<f64>(&device, &config, &capture),
            SampleFormat::F32 => stream::<f32>(&device, &config, &capture),
            SampleFormat::I16 => stream::<i16>(&device, &config, &capture),
            SampleFormat::U16 => stream::<u16>(&device, &config, &capture),
            SampleFormat::I32 => stream::<i32>(&device, &config, &capture),
            format => bail!(
                "{}",
                t!("input_format_unsupported", detail = format!("{format:?}"))
            ),
        }?;
        stream
            .play()
            .context(t!("microphone_start_failed").into_owned())?;
        Ok(Self {
            stream,
            capture,
            path,
        })
    }

    pub fn stop(self) -> Result<PathBuf> {
        drop(self.stream);
        let mut capture = self
            .capture
            .lock()
            .map_err(|_| anyhow!(t!("recording_failed").into_owned()))?;
        let writer = capture
            .writer
            .take()
            .context(t!("recording_file_unavailable").into_owned())?;
        let has_audio = writer.len() != 0;
        writer.finalize()?;
        if !has_audio {
            bail!(
                "{}",
                capture
                    .error
                    .as_deref()
                    .unwrap_or(&t!("microphone_no_audio"))
            );
        }
        let path = self.path.keep()?;
        if let Some(error) = &capture.error {
            bail!(
                "{}",
                t!("partial_recording", error = error, path = path.display())
            );
        }
        Ok(path)
    }
}

fn stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    capture: &Arc<Mutex<Capture>>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    i16: FromSample<T>,
{
    let audio = Arc::clone(capture);
    let errors = Arc::clone(capture);
    Ok(device.build_input_stream(
        config,
        move |samples: &[T], _| {
            let Ok(mut capture) = audio.lock() else {
                return;
            };
            if capture.error.is_some() {
                return;
            }
            if let Some(writer) = capture.writer.as_mut() {
                for &sample in samples {
                    if let Err(error) = writer.write_sample(i16::from_sample_(sample)) {
                        capture.error = Some(error.to_string());
                        break;
                    }
                }
            }
        },
        move |error| {
            if let Ok(mut capture) = errors.lock() {
                capture.error = Some(error.to_string());
            }
        },
        None,
    )?)
}
