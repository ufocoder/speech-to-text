use std::path::Path;

use anyhow::Result;
use sherpa_onnx::OfflineRecognizerConfig;

use crate::{audio::Audio, models::WhisperOnnxFiles};

use super::decode_sherpa;

pub(super) fn transcribe(
    audio: &Audio,
    model_label: &str,
    directory: &Path,
    files: WhisperOnnxFiles,
    range: (f64, f64),
    report: &impl Fn(u8) -> Result<()>,
) -> Result<String> {
    let path = |name: &str| Some(directory.join(name).to_string_lossy().into_owned());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.num_threads = 2;
    config.model_config.model_type = Some("whisper".into());
    config.model_config.tokens = path(files.tokens);
    config.model_config.whisper.encoder = path(files.encoder);
    config.model_config.whisper.decoder = path(files.decoder);
    config.model_config.whisper.task = Some("transcribe".into());

    // Leave room for padding in Whisper's 30-second input window.
    decode_sherpa(audio, model_label, config, 25, range, report)
}
