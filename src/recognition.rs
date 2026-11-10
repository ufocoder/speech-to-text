use anyhow::{Context, Result, ensure};
use half::f16;
use ort::{session::Session, value::Tensor};
use rust_i18n::t;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig};
use std::path::Path;

use crate::{audio::Audio, models::SpeechModel};

pub fn transcribe(audio: &Audio, model: SpeechModel, directory: &Path) -> Result<String> {
    match model {
        SpeechModel::Tone => transcribe_tone(audio, &directory.join("model.onnx")),
        SpeechModel::GigaAm | SpeechModel::Parakeet => transcribe_sherpa(audio, model, directory),
    }
}

fn transcribe_sherpa(audio: &Audio, model: SpeechModel, directory: &Path) -> Result<String> {
    let path = |name: &str| Some(directory.join(name).to_string_lossy().into_owned());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.num_threads = 2;
    config.model_config.tokens = path("tokens.txt");
    let segment_seconds = if model == SpeechModel::Parakeet {
        config.model_config.model_type = Some("nemo_transducer".into());
        config.model_config.transducer.encoder = path("encoder.int8.onnx");
        config.model_config.transducer.decoder = path("decoder.int8.onnx");
        config.model_config.transducer.joiner = path("joiner.int8.onnx");
        60
    } else {
        config.model_config.nemo_ctc.model = path("model.int8.onnx");
        config.feat_config.feature_dim = 64;
        24
    };
    let recognizer = OfflineRecognizer::create(&config)
        .with_context(|| t!("model_load_failed", detail = model.label()).into_owned())?;
    let samples = audio.mono(16_000);
    let mut phrases = Vec::new();
    for chunk in samples.chunks(16_000 * segment_seconds) {
        let stream = recognizer.create_stream();
        if chunk.len() < 1600 {
            let mut padded = [0.; 1600];
            padded[..chunk.len()].copy_from_slice(chunk);
            stream.accept_waveform(16_000, &padded);
        } else {
            stream.accept_waveform(16_000, chunk);
        }
        recognizer.decode(&stream);
        let result = stream
            .get_result()
            .context(t!("model_no_result").into_owned())?;
        if !result.text.trim().is_empty() {
            phrases.push(result.text.trim().to_owned());
        }
    }
    Ok(phrases.join(" "))
}

fn transcribe_tone(audio: &Audio, path: &Path) -> Result<String> {
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
    let samples = audio.mono(8000);
    let mut state = vec![f16::ZERO; STATE];
    let mut previous = None;
    let mut text = String::new();
    let silence: &[f32] = &[];
    for chunk in std::iter::once(silence)
        .chain(samples.chunks(CHUNK))
        .chain([silence])
    {
        let mut input = vec![0_i32; CHUNK];
        for (value, &sample) in input.iter_mut().zip(chunk) {
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
