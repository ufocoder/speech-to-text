use std::path::Path;

use crate::models::WhisperOnnxFiles;
use sherpa_onnx::OfflineRecognizerConfig;

pub(super) fn config(directory: &Path, files: WhisperOnnxFiles) -> OfflineRecognizerConfig {
    let path = |name: &str| Some(directory.join(name).to_string_lossy().into_owned());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.num_threads = 2;
    config.model_config.model_type = Some("whisper".into());
    config.model_config.tokens = path(files.tokens);
    config.model_config.whisper.encoder = path(files.encoder);
    config.model_config.whisper.decoder = path(files.decoder);
    config.model_config.whisper.task = Some("transcribe".into());

    config
}
