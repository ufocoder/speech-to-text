use crate::models::SpeechModel;
use std::collections::BTreeMap;
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

fn transcript_path(audio: &Path) -> PathBuf {
    let mut path = audio.as_os_str().to_owned();
    path.push(".txt");
    PathBuf::from(path)
}

fn transcript_model_path(audio: &Path) -> PathBuf {
    let mut path = transcript_path(audio).into_os_string();
    path.push(".model");
    PathBuf::from(path)
}

fn transcript_model(audio: &Path) -> Option<String> {
    std::fs::read_to_string(transcript_model_path(audio))
        .ok()
        .map(|model| model.trim().to_owned())
        .filter(|model| !model.is_empty())
}

pub type Transcripts = BTreeMap<Option<String>, Transcript>;

#[derive(Clone, Debug, PartialEq)]
pub enum Transcript {
    Legacy(String),
    Structured(Vec<crate::recognition::TranscriptTurn>),
}

fn structured_path(audio: &Path, model: SpeechModel) -> PathBuf {
    model_transcript_path(audio, model).with_extension("json")
}

fn model_transcript_path(audio: &Path, model: SpeechModel) -> PathBuf {
    let mut path = audio.as_os_str().to_owned();
    path.push(format!(".{}.txt", model.id()));
    path.into()
}

pub fn transcript_files(audio: &Path) -> BTreeMap<Option<String>, PathBuf> {
    let mut files = BTreeMap::new();
    let legacy = transcript_path(audio);
    if legacy.is_file() {
        files.insert(transcript_model(audio), legacy);
    }
    for model in SpeechModel::ALL {
        let structured = structured_path(audio, model);
        let path = if structured.is_file() {
            structured
        } else {
            model_transcript_path(audio, model)
        };
        if path.is_file() {
            files.insert(Some(model.label().to_owned()), path);
        }
    }
    files
}

pub fn read_transcripts(audio: &Path) -> Transcripts {
    transcript_files(audio)
        .into_iter()
        .filter_map(|(model, path)| {
            let transcript = if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                let file = std::fs::File::open(path).ok()?;
                Transcript::Structured(serde_json::from_reader(file).ok()?)
            } else {
                Transcript::Legacy(std::fs::read_to_string(path).ok()?)
            };
            Some((model, transcript))
        })
        .collect()
}

pub fn save_transcript(
    audio: &Path,
    turns: &[crate::recognition::TranscriptTurn],
    model: SpeechModel,
) -> std::io::Result<()> {
    let parent = audio
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut file, turns)?;
    file.persist(structured_path(audio, model))?;
    Ok(())
}

pub fn delete_transcripts(audio: &Path) -> std::io::Result<()> {
    let mut first_error = None;
    for path in [transcript_path(audio), transcript_model_path(audio)]
        .into_iter()
        .chain(SpeechModel::ALL.map(|model| model_transcript_path(audio, model)))
        .chain(SpeechModel::ALL.map(|model| structured_path(audio, model)))
    {
        if let Err(error) = std::fs::remove_file(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

pub fn language_file() -> PathBuf {
    app_data_dir().join(".language")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcripts_are_independent_and_preserve_legacy_results() -> std::io::Result<()> {
        let directory = tempfile::tempdir()?;
        let audio = directory.path().join("recording.wav");
        std::fs::write(transcript_path(&audio), "legacy")?;
        std::fs::write(transcript_model_path(&audio), SpeechModel::Tone.label())?;
        let turns = |text: &str| {
            vec![crate::recognition::TranscriptTurn {
                start: 4.25,
                end: 6.,
                speaker: Some(1),
                text: text.to_owned(),
            }]
        };
        save_transcript(&audio, &turns("first"), SpeechModel::GigaAm)?;
        save_transcript(&audio, &turns("second"), SpeechModel::Parakeet)?;
        save_transcript(&audio, &turns("updated"), SpeechModel::GigaAm)?;
        assert_eq!(
            read_transcripts(&audio),
            [
                (SpeechModel::Tone, "legacy"),
                (SpeechModel::GigaAm, "updated"),
                (SpeechModel::Parakeet, "second"),
            ]
            .map(|(model, text)| (
                Some(model.label().to_owned()),
                if model == SpeechModel::Tone {
                    Transcript::Legacy(text.to_owned())
                } else {
                    Transcript::Structured(turns(text))
                }
            ))
            .into()
        );
        save_transcript(&audio, &turns("new"), SpeechModel::Tone)?;
        assert_eq!(read_transcripts(&audio).len(), 3);
        assert_eq!(
            read_transcripts(&audio)[&Some("T-one".to_owned())],
            Transcript::Structured(turns("new"))
        );
        delete_transcripts(&audio)?;
        assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
        Ok(())
    }
}

pub fn added_models() -> Vec<SpeechModel> {
    let saved = std::fs::read_to_string(app_data_dir().join(".added-models")).unwrap_or_default();
    SpeechModel::ALL
        .into_iter()
        .filter(|model| saved.lines().any(|id| id == model.id()))
        .collect()
}

pub fn save_added_models(models: &[SpeechModel]) -> std::io::Result<()> {
    use std::io::Write;
    let directory = app_data_dir();
    std::fs::create_dir_all(&directory)?;
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    for model in models {
        writeln!(file, "{}", model.id())?;
    }
    file.persist(directory.join(".added-models"))?;
    Ok(())
}

pub fn default_speaker_model() -> crate::models::SpeakerModel {
    use crate::models::SpeakerModel;
    match std::fs::read_to_string(app_data_dir().join(".speaker-model")).as_deref() {
        Ok("int8") => SpeakerModel::Int8,
        Ok("reverb") => SpeakerModel::Reverb,
        _ => SpeakerModel::Standard,
    }
}

pub fn save_default_speaker_model(model: crate::models::SpeakerModel) -> std::io::Result<()> {
    use std::io::Write;
    let directory = app_data_dir();
    std::fs::create_dir_all(&directory)?;
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    file.write_all(match model {
        crate::models::SpeakerModel::Standard => b"standard",
        crate::models::SpeakerModel::Int8 => b"int8",
        crate::models::SpeakerModel::Reverb => b"reverb",
    })?;
    file.persist(directory.join(".speaker-model"))?;
    Ok(())
}
