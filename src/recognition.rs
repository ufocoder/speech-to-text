mod whisper_onnx;

use anyhow::{Context, Result, ensure};
use half::f16;
use ort::{session::Session, value::Tensor};
use rust_i18n::t;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig};
use std::path::Path;

use crate::{audio::Audio, models::SpeechModel};

pub fn transcribe(
    audio: &Audio,
    model: SpeechModel,
    directory: &Path,
    range: (f64, f64),
    report: &impl Fn(u8) -> Result<()>,
) -> Result<String> {
    if let Some(files) = model.whisper_onnx_files() {
        return whisper_onnx::transcribe(audio, model.label(), directory, files, range, report);
    }
    match model {
        SpeechModel::Tone => transcribe_tone(audio, &directory.join("model.onnx"), range, report),
        SpeechModel::GigaAm | SpeechModel::Parakeet => {
            transcribe_sherpa(audio, model, directory, range, report)
        }
        SpeechModel::WhisperTurbo => unreachable!("Whisper has an ONNX adapter configuration"),
    }
}

fn transcribe_sherpa(
    audio: &Audio,
    model: SpeechModel,
    directory: &Path,
    range: (f64, f64),
    report: &impl Fn(u8) -> Result<()>,
) -> Result<String> {
    let path = |name: &str| Some(directory.join(name).to_string_lossy().into_owned());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.num_threads = 2;
    config.model_config.tokens = path("tokens.txt");
    let segment_seconds = match model {
        SpeechModel::Parakeet => {
            config.model_config.model_type = Some("nemo_transducer".into());
            config.model_config.transducer.encoder = path("encoder.int8.onnx");
            config.model_config.transducer.decoder = path("decoder.int8.onnx");
            config.model_config.transducer.joiner = path("joiner.int8.onnx");
            60
        }
        SpeechModel::GigaAm => {
            config.model_config.nemo_ctc.model = path("model.int8.onnx");
            config.feat_config.feature_dim = 64;
            24
        }
        SpeechModel::Tone | SpeechModel::WhisperTurbo => unreachable!(),
    };
    decode_sherpa(audio, model.label(), config, segment_seconds, range, report)
}

fn decode_sherpa(
    audio: &Audio,
    model_label: &str,
    config: OfflineRecognizerConfig,
    segment_seconds: usize,
    range: (f64, f64),
    report: &impl Fn(u8) -> Result<()>,
) -> Result<String> {
    let recognizer = OfflineRecognizer::create(&config)
        .with_context(|| t!("model_load_failed", detail = model_label).into_owned())?;
    let mut phrases = Vec::new();
    let mut processed = 0;
    for chunk in audio.mono_range(16_000, 16_000 * segment_seconds, range.0, range.1) {
        let stream = recognizer.create_stream();
        if chunk.len() < 1600 {
            let mut padded = [0.; 1600];
            padded[..chunk.len()].copy_from_slice(&chunk);
            stream.accept_waveform(16_000, &padded);
        } else {
            stream.accept_waveform(16_000, &chunk);
        }
        recognizer.decode(&stream);
        processed += chunk.len();
        report(percent(range.1 - range.0, processed, 16_000))?;
        let result = stream
            .get_result()
            .context(t!("model_no_result").into_owned())?;
        if !result.text.trim().is_empty() {
            phrases.push(result.text.trim().to_owned());
        }
    }
    Ok(phrases.join(" "))
}

fn transcribe_tone(
    audio: &Audio,
    path: &Path,
    range: (f64, f64),
    report: &impl Fn(u8) -> Result<()>,
) -> Result<String> {
    // T-one takes 300 ms of signed PCM and carries an FP16 state across chunks.
    const CHUNK: usize = 2400;
    const STATE: usize = 219729;
    const CLASSES: usize = 35;
    let alphabet: Vec<_> = "абвгдеёжзийклмнопрстуфхцчшщъыьэюя ".chars().collect();
    // sherpa-onnx already links ONNX Runtime. Use the same C API for T-one.
    let api = unsafe {
        let base = ort::sys::OrtGetApiBase()
            .as_ref()
            .context(t!("ort_unavailable").into_owned())?;
        (base.GetApi)(ort::sys::ORT_API_VERSION)
            .as_ref()
            .context(t!("ort_incompatible").into_owned())?
    };
    ort::set_api(api.clone());
    let mut session = Session::builder()?
        .with_intra_threads(2)
        .map_err(|error| anyhow::anyhow!("{error}"))?
        .commit_from_file(path)?;
    let mut state = vec![f16::ZERO; STATE];
    let mut previous = None;
    let mut text = String::new();
    let mut processed = 0;
    for chunk in std::iter::once(Vec::new())
        .chain(audio.mono_range(8000, CHUNK, range.0, range.1))
        .chain([Vec::new()])
    {
        processed += chunk.len();
        let mut input = vec![0_i32; CHUNK];
        for (value, sample) in input.iter_mut().zip(chunk) {
            *value = (sample * 32768.).round().clamp(-32768., 32767.) as i32;
        }
        let outputs = session.run(ort::inputs![
            "signal" => Tensor::from_array(([1, CHUNK, 1], input))?,
            "state" => Tensor::from_array(([1, STATE], state))?,
        ])?;
        let (_, scores) = outputs[0].try_extract_tensor::<f32>()?;
        let (_, next_state) = outputs[1].try_extract_tensor::<f16>()?;
        ensure!(
            scores.len() % CLASSES == 0 && next_state.len() == STATE,
            t!("tone_incompatible").into_owned()
        );
        state = next_state.to_vec();
        report(percent(range.1 - range.0, processed, 8000))?;
        for frame in scores.chunks_exact(CLASSES) {
            let best = frame
                .iter()
                .enumerate()
                .reduce(|best, item| if item.1 > best.1 { item } else { best })
                .expect("nonempty CTC frame")
                .0;
            if previous != Some(best)
                && let Some(&character) = alphabet.get(best)
            {
                text.push(character);
            }
            previous = Some(best);
        }
    }
    Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn percent(duration: f64, samples: usize, rate: u32) -> u8 {
    (samples as f64 * 100. / f64::from(rate) / duration).min(99.) as u8
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TranscriptTurn {
    pub start: f64,
    pub end: f64,
    pub speaker: Option<i32>,
    pub text: String,
}

pub fn transcribe_speakers(
    audio: &Audio,
    model: SpeechModel,
    directory: &Path,
    segments: &[sherpa_onnx::OfflineSpeakerDiarizationSegment],
    report: &impl Fn(u8) -> Result<()>,
) -> Result<Vec<TranscriptTurn>> {
    let duration = audio.duration();
    let mut turns: Vec<_> = Vec::new();
    for segment in segments {
        let start = f64::from(segment.start);
        let end = f64::from(segment.end).min(duration);
        if end <= start {
            continue;
        }
        if let Some((speaker, _, previous_end)) = turns.last_mut()
            && *speaker == segment.speaker
            && start <= *previous_end + 0.25
        {
            *previous_end = end.max(*previous_end);
        } else {
            turns.push((segment.speaker, start, end));
        }
    }
    if turns.is_empty() {
        let text = transcribe(audio, model, directory, (0., duration), report)?;
        return Ok(vec![TranscriptTurn {
            start: 0.,
            end: duration,
            speaker: None,
            text,
        }]);
    }

    let total: f64 = turns.iter().map(|(_, start, end)| end - start).sum();
    let mut processed = 0.;
    let mut lines = Vec::new();
    for (speaker, start, end) in turns {
        report((processed * 100. / total) as u8)?;
        let text = transcribe(audio, model, directory, (start, end), &|value| {
            report(
                ((processed + (end - start) * f64::from(value) / 100.) * 100. / total).min(99.)
                    as u8,
            )
        })?;
        processed += end - start;
        if text.is_empty() {
            continue;
        }
        lines.push(TranscriptTurn {
            start,
            end,
            speaker: Some(speaker),
            text,
        });
    }
    Ok(lines)
}

pub fn format_turns(turns: &[TranscriptTurn], timestamps: bool, speakers: bool) -> String {
    turns
        .iter()
        .filter(|turn| !turn.text.is_empty())
        .map(|turn| {
            let seconds = turn.start as u64;
            let time = format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            );
            let label = turn.speaker.filter(|_| speakers).map(|speaker| {
                t!("transcript_speaker", number = i64::from(speaker) + 1).into_owned()
            });
            let text = &turn.text;
            match (timestamps, label) {
                (true, Some(label)) => format!("{time}, {label}: {text}"),
                (true, None) => format!("{time}: {text}"),
                (false, Some(label)) => format!("{label}: {text}"),
                (false, None) => text.clone(),
            }
        })
        .collect::<Vec<_>>()
        .join(if timestamps || speakers { "\n\n" } else { " " })
}
