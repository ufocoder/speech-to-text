use std::path::{Path, PathBuf};

fn app_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    if std::env::current_exe().is_ok_and(|exe| {
        exe.ancestors()
            .any(|path| path.extension().is_some_and(|ext| ext == "app"))
    }) && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join("Library/Application Support/Speech to Text");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn recordings_dir() -> PathBuf {
    std::fs::read_to_string(recordings_dir_file())
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| app_data_dir().join("recordings"))
}

fn recordings_dir_file() -> PathBuf {
    app_data_dir().join(".recordings-directory")
}

pub fn save_recordings_dir(path: &Path) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind, Write};

    let path = path.canonicalize()?;
    if !path.is_dir() {
        return Err(Error::from(ErrorKind::NotADirectory));
    }
    let value = path
        .to_str()
        .ok_or_else(|| Error::from(ErrorKind::InvalidData))?;
    let directory = app_data_dir();
    std::fs::create_dir_all(&directory)?;
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    file.write_all(value.as_bytes())?;
    file.persist(recordings_dir_file())?;
    Ok(())
}

pub struct SavedRecording {
    pub path: PathBuf,
    pub recorded_at: Option<std::time::SystemTime>,
}

pub fn saved_recordings() -> std::io::Result<Vec<SavedRecording>> {
    let entries = match std::fs::read_dir(recordings_dir()) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();
        if ["wav", "mp3", "flac", "ogg", "vorbis"]
            .iter()
            .any(|ext| extension.eq_ignore_ascii_case(ext))
            && entry.file_type()?.is_file()
        {
            let recorded_at = entry
                .metadata()
                .and_then(|metadata| metadata.created().or_else(|_| metadata.modified()))
                .ok();
            files.push(SavedRecording { path, recorded_at });
        }
    }
    files.sort_unstable_by(|left, right| {
        right
            .recorded_at
            .cmp(&left.recorded_at)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(files)
}

pub fn model_cache() -> PathBuf {
    std::env::var_os("SPEECH_MODEL_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| app_data_dir().join(".model-cache").join("hub"))
}

pub fn transcript_path(audio: &Path) -> PathBuf {
    let mut path = audio.as_os_str().to_owned();
    path.push(".txt");
    PathBuf::from(path)
}

fn transcript_model_path(audio: &Path) -> PathBuf {
    let mut path = transcript_path(audio).into_os_string();
    path.push(".model");
    PathBuf::from(path)
}

pub fn transcript_model(audio: &Path) -> Option<String> {
    std::fs::read_to_string(transcript_model_path(audio))
        .ok()
        .map(|model| model.trim().to_owned())
        .filter(|model| !model.is_empty())
}

pub fn save_transcript(
    audio: &Path,
    directory: &Path,
    text: &str,
    model: &str,
) -> std::io::Result<()> {
    use std::io::Write;

    let Some(parent) = audio.parent() else {
        return Ok(());
    };
    if parent
        .canonicalize()
        .ok()
        .zip(directory.canonicalize().ok())
        .is_none_or(|(parent, recordings)| parent != recordings)
    {
        return Ok(());
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(text.as_bytes())?;
    let model_path = transcript_model_path(audio);
    let mut metadata = tempfile::NamedTempFile::new_in(parent)?;
    metadata.write_all(model.as_bytes())?;

    // If updating an existing result fails halfway, show an unknown model
    // instead of attributing the new text to the previous model.
    match std::fs::remove_file(&model_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    file.persist(transcript_path(audio))?;
    metadata.persist(model_path)?;
    Ok(())
}

pub fn language_file() -> PathBuf {
    app_data_dir().join(".language")
}
