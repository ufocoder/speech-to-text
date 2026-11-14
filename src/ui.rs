use rust_i18n::t;
mod catalog;
mod layout;
mod menus;
mod playback;
mod recordings;
mod settings;
mod speakers;

use crate::{
    audio::{Audio, Player, Recording},
    i18n::{Language, Message},
    models::SpeechModel,
    recognition,
    storage::{SavedRecording, Transcripts, model_cache, recordings_dir},
};
use anyhow::Context as _;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering::Relaxed},
};
use std::time::{Duration, Instant};
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) struct Recorder {
    recording: Option<Recording>,
    speaker_download: Option<usize>,
    processing_cancel: Arc<std::sync::atomic::AtomicBool>,
    recognition_task: Option<Task<()>>,
    opening_task: Option<Task<()>>,
    speaker_model: crate::models::SpeakerModel,
    default_speaker_model: crate::models::SpeakerModel,
    include_timestamps: bool,
    include_speakers: bool,
    speaker_task: Option<Task<()>>,
    speaker_result: Option<Result<Vec<sherpa_onnx::OfflineSpeakerDiarizationSegment>, String>>,
    microphone: Option<(SharedString, cpal::Device)>,
    recognition_job: Option<(PathBuf, SpeechModel)>,
    session_transcripts: BTreeMap<PathBuf, Transcripts>,
    transcripts: Transcripts,
    selected_transcript: Option<String>,
    recognition_started: Option<Instant>,
    model_download_started: Option<Instant>,
    speaker_download_started: Option<Instant>,
    loading_audio: bool,
    opening_progress: Arc<AtomicU8>,
    recognition_progress: Arc<AtomicU8>,
    speaker_download_progress: Arc<AtomicU8>,
    speaker_started: Option<Instant>,
    speaker_progress: Arc<AtomicU8>,
    model: SpeechModel,
    audio_model: Option<SpeechModel>,
    catalog_models: Vec<SpeechModel>,
    model_languages_expanded: bool,
    model_update_dates: BTreeMap<&'static str, chrono::NaiveDate>,
    model_update_tasks: Vec<Task<()>>,
    downloading_model: Option<SpeechModel>,
    download_percent: u8,
    download_progress_task: Option<Task<()>>,
    choosing_file: bool,
    status: Message,
    language: Language,
    settings_open: bool,
    settings_error: Option<Message>,
    transcript_font_size: u8,
    transcript: Entity<TextareaState>,
    audio: Option<Arc<Audio>>,
    audio_path: Option<PathBuf>,
    saved_recordings: Option<Result<Vec<SavedRecording>, Message>>,
    player: Option<Player>,
    playback_position: f64,
    playback_task: Option<Task<()>>,
    playback_error: Option<Message>,
}

impl Recorder {
    pub(crate) fn new(language: Language, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let transcript = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(t!("transcript_placeholder").into_owned())
        });
        Self {
            recording: None,
            speaker_download: None,
            processing_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            recognition_task: None,
            opening_task: None,
            speaker_model: crate::storage::default_speaker_model(),
            default_speaker_model: crate::storage::default_speaker_model(),
            include_timestamps: true,
            include_speakers: true,
            speaker_task: None,
            speaker_result: None,
            microphone: None,
            recognition_job: None,
            session_transcripts: BTreeMap::new(),
            transcripts: Transcripts::new(),
            selected_transcript: None,
            recognition_started: None,
            model_download_started: None,
            speaker_download_started: None,
            loading_audio: false,
            opening_progress: Arc::new(AtomicU8::new(0)),
            recognition_progress: Arc::new(AtomicU8::new(0)),
            speaker_download_progress: Arc::new(AtomicU8::new(0)),
            speaker_started: None,
            speaker_progress: Arc::new(AtomicU8::new(0)),
            model: SpeechModel::Parakeet,
            audio_model: None,
            catalog_models: crate::storage::added_models(),
            model_languages_expanded: false,
            model_update_dates: BTreeMap::new(),
            model_update_tasks: Vec::new(),
            downloading_model: None,
            download_percent: 0,
            download_progress_task: None,
            choosing_file: false,
            status: Message::Empty,
            language,
            settings_open: false,
            settings_error: None,
            transcript_font_size: 16,
            transcript,
            audio: None,
            audio_path: None,
            saved_recordings: None,
            player: None,
            playback_position: 0.,
            playback_task: None,
            playback_error: None,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn record_speech(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recording.is_some() || self.loading_audio || self.choosing_file {
            return;
        }
        self.settings_open = false;
        self.saved_recordings = None;
        self.clear_audio(window, cx);
        self.toggle(window, cx);
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading_audio || self.choosing_file {
            return;
        }
        if let Some(recording) = self.recording.take() {
            match recording.stop() {
                Ok(path) => self.load_audio(path, window, cx),
                Err(error) => {
                    self.status = Message::Detail("recording_error", format!("{error:#}"))
                }
            }
        } else {
            let device = self.microphone.as_ref().map(|(_, device)| device);
            match Recording::start(&recordings_dir(), device) {
                Ok(recording) => {
                    self.clear_audio(window, cx);
                    self.recording = Some(recording);
                    self.status = Message::Text("recording");
                }
                Err(error) => {
                    self.status = Message::Detail("microphone_error", format!("{error:#}"))
                }
            }
        }
        cx.notify();
    }

    fn reset_audio(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.processing_cancel.store(true, Relaxed);
        self.opening_task = None;
        self.recognition_task = None;
        self.speaker_task = None;
        self.loading_audio = false;
        self.recognition_job = None;
        self.clear_audio(window, cx);
        self.saved_recordings = None;
        self.settings_open = false;
        self.status = Message::Empty;
        cx.notify();
    }

    fn clear_audio(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.playback_task = None;
        self.speaker_result = None;
        self.speaker_model = self.default_speaker_model;
        self.player = None;
        self.playback_position = 0.;
        self.audio_model = None;
        self.audio = None;
        self.audio_path = None;
        self.playback_error = None;
        self.transcripts.clear();
        self.selected_transcript = None;
        self.transcript.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
    }

    fn choose_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading_audio || self.choosing_file || self.recording.is_some() {
            return;
        }
        self.choosing_file = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t!("choose_audio").into_owned().into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let selection = selection.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.choosing_file = false;
                match selection {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.load_audio(path, window, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        this.status = Message::Detail("file_picker_error", format!("{error:#}"))
                    }
                    Err(_) => this.status = Message::Text("file_picker_unavailable"),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn audio_busy(&self) -> bool {
        self.loading_audio || self.recognition_job.is_some()
    }

    fn load_audio(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.load_audio_with_transcript(path, None, window, cx);
    }

    fn load_audio_with_transcript(
        &mut self,
        path: PathBuf,
        transcript: Option<Option<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading_audio || self.choosing_file || self.recording.is_some() {
            return;
        }
        self.clear_audio(window, cx);
        self.loading_audio = true;
        self.status = Message::Detail(
            "opening_audio",
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
        );
        self.audio_path = Some(path.clone());
        let audio_path = path.clone();
        let (progress, refresh) = Self::track_progress(&self.opening_progress, cx);
        self.processing_cancel = Arc::default();
        let cancel = self.processing_cancel.clone();
        let task = cx.background_spawn(async move {
            Audio::load(&audio_path, |value| {
                anyhow::ensure!(!cancel.load(Relaxed), "Cancelled");
                progress.store(value, Relaxed);
                Ok(())
            })
        });
        self.opening_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await.context(t!("open_audio_failed").into_owned());
            drop(refresh);
            let _ = this.update_in(cx, |this, window, cx| {
                this.opening_task = None;
                this.loading_audio = false;
                match result {
                    Ok(audio) => {
                        this.audio = Some(Arc::new(audio));
                        this.transcripts = crate::storage::read_transcripts(&path);
                        if let Some(results) = this.session_transcripts.get(&path) {
                            this.transcripts.extend(results.clone());
                        }
                        let selected = transcript
                            .filter(|model| this.transcripts.contains_key(model))
                            .or_else(|| {
                                let default = Some(this.model.label().to_owned());
                                this.transcripts
                                    .get_key_value(&default)
                                    .map(|(key, _)| key.clone())
                            })
                            .or_else(|| this.transcripts.keys().next().cloned())
                            .flatten();
                        this.select_transcript(selected, window, cx);
                        this.audio_path = Some(path);
                        this.status = Message::Empty;
                    }
                    Err(error) => {
                        this.audio_path = None;
                        this.status = Message::Error(format!("{error:#}"));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn select_transcript(
        &mut self,
        model: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = match self.transcripts.get(&model) {
            Some(crate::storage::Transcript::Legacy(text)) => text.clone(),
            Some(crate::storage::Transcript::Structured(turns)) => {
                recognition::format_turns(turns, self.include_timestamps, self.include_speakers)
            }
            None => String::new(),
        };
        self.transcript
            .update(cx, |input, cx| input.set_value(text, window, cx));
        self.selected_transcript = model;
        cx.notify();
    }

    fn recognition_model_for(&self, path: &std::path::Path) -> Option<SpeechModel> {
        self.recognition_job
            .as_ref()
            .filter(|(source, _)| source == path)
            .map(|(_, model)| *model)
    }

    fn active_audio_model(&self) -> Option<SpeechModel> {
        let cache = model_cache();
        self.audio_model
            .into_iter()
            .chain([self.model])
            .chain(SpeechModel::ALL)
            .find(|model| model.cached_directory(&cache).is_some())
    }

    fn track_progress(progress: &Arc<AtomicU8>, cx: &Context<Self>) -> (Arc<AtomicU8>, Task<()>) {
        progress.store(0, Relaxed);
        let watched = progress.clone();
        let executor = cx.background_executor().clone();
        let refresh = cx.spawn(async move |this, cx| {
            let mut previous = 0;
            loop {
                executor.timer(Duration::from_millis(120)).await;
                let current = watched.load(Relaxed);
                if current != previous {
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                    previous = current;
                }
            }
        });
        (progress.clone(), refresh)
    }

    fn recognize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.audio_busy()
            || self.choosing_file
            || self.recording.is_some()
            || self.speaker_task.is_some()
            || self.speaker_download.is_some()
        {
            return;
        }
        let (Some(audio), Some(path)) = (self.audio.clone(), self.audio_path.clone()) else {
            return;
        };
        let Some((model, directory)) = self.active_audio_model().and_then(|model| {
            model
                .cached_directory(&model_cache())
                .map(|directory| (model, directory))
        }) else {
            self.status = Message::Text("download_model_hint");
            cx.notify();
            return;
        };
        self.processing_cancel = Arc::default();
        let cancel = self.processing_cancel.clone();
        let segments = self.split_speakers(window, cx);
        self.recognition_job = Some((path.clone(), model));
        let source = path.clone();
        self.status = Message::Empty;
        self.recognition_started = None;
        let (progress, refresh) = Self::track_progress(&self.recognition_progress, cx);
        self.recognition_task = Some(cx.spawn_in(window, async move |this, cx| {
            let segments = segments.await.unwrap_or_default();
            let _ = this.update_in(cx, |this, _, cx| {
                this.recognition_started = Some(Instant::now());
                cx.notify();
            });
            let result = cx
                .background_spawn(async move {
                    let report = |value| {
                        anyhow::ensure!(!cancel.load(Relaxed), "Cancelled");
                        progress.store(value, Relaxed);
                        Ok(())
                    };
                    recognition::transcribe_speakers(&audio, model, &directory, &segments, &report)
                })
                .await;
            drop(refresh);
            let _ = this.update_in(cx, |this, window, cx| {
                this.recognition_task = None;
                this.recognition_job = None;
                let source_is_open = this.audio_path.as_ref() == Some(&source);
                match result {
                    Ok(turns) => {
                        let save = crate::storage::save_transcript(&path, &turns, model);
                        let text = crate::storage::Transcript::Structured(turns);
                        let key = Some(model.label().to_owned());
                        if let Err(error) = &save {
                            this.session_transcripts
                                .entry(source.clone())
                                .or_default()
                                .insert(key.clone(), text.clone());
                            if !source_is_open {
                                drop(window.prompt(
                                    PromptLevel::Warning,
                                    &t!("save_text_error", detail = error.to_string()),
                                    None,
                                    &[t!("ok").as_ref()],
                                    cx,
                                ));
                            }
                        } else if let Some(results) = this.session_transcripts.get_mut(&source) {
                            results.remove(&key);
                        }
                        if !source_is_open {
                            cx.notify();
                            return;
                        }
                        this.status = Message::Empty;
                        if let Err(error) = save {
                            this.status = Message::Detail("save_text_error", error.to_string());
                        }
                        let first_result = this.transcripts.is_empty();
                        let selected = if first_result {
                            key.clone()
                        } else {
                            this.selected_transcript.clone()
                        };
                        this.transcripts.insert(key, text);
                        this.select_transcript(selected, window, cx);
                    }
                    Err(error) if source_is_open => {
                        this.status = Message::Error(format!("{error:#}"))
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

fn progress_label(progress: &AtomicU8) -> String {
    let value = progress.load(Relaxed);
    if value == 0 {
        String::new()
    } else {
        format!("{value}%")
    }
}

fn remaining_time(started: Option<Instant>, percent: u8) -> String {
    let Some(elapsed) = started.map(|time| time.elapsed()) else {
        return String::new();
    };
    if percent == 0 || (percent < 99 && elapsed.as_secs() < 2) {
        return t!("estimating_time").into_owned();
    }
    if percent >= 99 {
        return String::new();
    }
    format_remaining_time(estimate_seconds(elapsed, percent).unwrap_or_default())
}

fn estimate_seconds(elapsed: Duration, percent: u8) -> Option<u64> {
    (elapsed.as_secs() >= 2 && (1..99).contains(&percent)).then(|| {
        (elapsed.as_secs_f64() * f64::from(100 - percent) / f64::from(percent)).ceil() as u64
    })
}

fn format_remaining_time(seconds: u64) -> String {
    let time = if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    };
    t!("remaining_time", time = time).into_owned()
}
