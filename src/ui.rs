use rust_i18n::t;
mod catalog;
mod layout;
mod menus;
mod playback;
mod recordings;
mod settings;
mod speakers;
pub(crate) mod theme;
mod transcript;

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

struct TranscriptView {
    timestamps: bool,
    highlight: bool,
    speakers: bool,
    visible: bool,
    font_size: u8,
}

impl Default for TranscriptView {
    fn default() -> Self {
        Self {
            timestamps: true,
            highlight: false,
            speakers: true,
            visible: false,
            font_size: 16,
        }
    }
}

struct PlaybackState {
    player: Option<Player>,
    position: f64,
    task: Option<Task<()>>,
    error: Option<Message>,
}

struct AudioOpening {
    loading: bool,
    task: Option<Task<()>>,
    progress: Arc<AtomicU8>,
    choosing: bool,
}

struct RecognitionState {
    task: Option<Task<()>>,
    job: Option<(PathBuf, SpeechModel)>,
    started: Option<Instant>,
    progress: Arc<AtomicU8>,
    requested: bool,
}

struct ModelDownload {
    model: Option<SpeechModel>,
    percent: u8,
    progress_task: Option<Task<()>>,
    started: Option<Instant>,
}

pub(crate) struct Recorder {
    model_download: ModelDownload,
    recognition: RecognitionState,
    opening: AudioOpening,
    playback: PlaybackState,
    recording: Option<Recording>,
    speaker_download: Option<usize>,
    processing_cancel: Arc<std::sync::atomic::AtomicBool>,
    speaker_model: crate::models::SpeakerModel,
    default_speaker_model: crate::models::SpeakerModel,
    speaker_task: Option<Task<()>>,
    speaker_result: Option<Result<Vec<sherpa_onnx::OfflineSpeakerDiarizationSegment>, String>>,
    microphone: Option<(SharedString, cpal::Device)>,
    session_transcripts: BTreeMap<PathBuf, Transcripts>,
    transcripts: Transcripts,
    audio_scroll: ScrollHandle,
    audio_settings_expanded: bool,
    selected_transcript: Option<String>,
    speaker_download_started: Option<Instant>,
    speaker_download_progress: Arc<AtomicU8>,
    speaker_started: Option<Instant>,
    speaker_progress: Arc<AtomicU8>,
    model: SpeechModel,
    audio_model: Option<SpeechModel>,
    catalog_models: Vec<SpeechModel>,
    model_languages_expanded: bool,
    model_update_dates: BTreeMap<&'static str, chrono::NaiveDate>,
    model_update_tasks: Vec<Task<()>>,
    available_model_updates: Vec<SpeechModel>,
    status: Message,
    language: Language,
    settings_open: bool,
    audio_cache_bytes: u64,
    settings_error: Option<Message>,
    transcript: Entity<TextareaState>,
    transcript_view: TranscriptView,
    audio: Option<Arc<Audio>>,
    audio_path: Option<PathBuf>,
    saved_recordings: Option<Result<Vec<SavedRecording>, Message>>,
}

impl Recorder {
    pub(crate) fn new(language: Language, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let transcript = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(t!("transcript_placeholder").into_owned())
        });
        Self {
            model_download: ModelDownload {
                model: None,
                percent: 0,
                progress_task: None,
                started: None,
            },
            recognition: RecognitionState {
                task: None,
                job: None,
                started: None,
                progress: Arc::new(AtomicU8::new(0)),
                requested: false,
            },
            opening: AudioOpening {
                loading: false,
                task: None,
                progress: Arc::new(AtomicU8::new(0)),
                choosing: false,
            },
            playback: PlaybackState {
                player: None,
                position: 0.,
                task: None,
                error: None,
            },
            recording: None,
            speaker_download: None,
            processing_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            speaker_model: crate::storage::default_speaker_model(),
            default_speaker_model: crate::storage::default_speaker_model(),
            speaker_task: None,
            speaker_result: None,
            microphone: None,
            session_transcripts: BTreeMap::new(),
            transcripts: Transcripts::new(),
            audio_scroll: ScrollHandle::new(),
            audio_settings_expanded: false,
            selected_transcript: None,
            speaker_download_started: None,
            speaker_download_progress: Arc::new(AtomicU8::new(0)),
            speaker_started: None,
            speaker_progress: Arc::new(AtomicU8::new(0)),
            model: SpeechModel::Parakeet,
            audio_model: None,
            catalog_models: crate::storage::added_models(),
            model_languages_expanded: false,
            model_update_dates: BTreeMap::new(),
            model_update_tasks: Vec::new(),
            available_model_updates: Vec::new(),
            status: Message::Empty,
            language,
            settings_open: false,
            audio_cache_bytes: 0,
            settings_error: None,
            transcript,
            transcript_view: TranscriptView::default(),
            audio: None,
            audio_path: None,
            saved_recordings: None,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn record_speech(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recording.is_some() || self.opening.loading || self.opening.choosing {
            return;
        }
        self.settings_open = false;
        self.saved_recordings = None;
        self.clear_audio(window, cx);
        self.toggle(window, cx);
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.opening.loading || self.opening.choosing {
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
        self.opening.task = None;
        self.recognition.task = None;
        self.speaker_task = None;
        self.opening.loading = false;
        self.recognition.job = None;
        self.clear_audio(window, cx);
        self.saved_recordings = None;
        self.settings_open = false;
        self.status = Message::Empty;
        cx.notify();
    }

    fn clear_audio(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.playback.task = None;
        self.speaker_result = None;
        self.speaker_model = self.default_speaker_model;
        self.playback.player = None;
        self.playback.position = 0.;
        self.audio_model = None;
        self.audio = None;
        self.audio_path = None;
        self.playback.error = None;
        self.transcripts.clear();
        self.audio_scroll = ScrollHandle::new();
        self.transcript_view.visible = false;
        self.recognition.requested = false;
        self.audio_settings_expanded = false;
        self.selected_transcript = None;
        self.transcript.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
    }

    fn choose_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.opening.loading || self.opening.choosing || self.recording.is_some() {
            return;
        }
        self.opening.choosing = true;
        let selection = rfd::AsyncFileDialog::new()
            .set_title(t!("choose_audio").into_owned())
            .add_filter(
                "WAV, MP3, FLAC, Ogg Vorbis, M4A",
                crate::audio::SUPPORTED_EXTENSIONS,
            )
            .pick_file();
        cx.spawn_in(window, async move |this, cx| {
            let selection = selection.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.opening.choosing = false;
                if let Some(file) = selection {
                    this.load_audio(file.path().to_path_buf(), window, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn audio_busy(&self) -> bool {
        self.opening.loading || self.recognition.job.is_some()
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
        if self.opening.loading || self.opening.choosing || self.recording.is_some() {
            return;
        }
        if !crate::audio::is_supported(&path) {
            self.status = Message::Text("unsupported_audio_file");
            cx.notify();
            return;
        }
        self.clear_audio(window, cx);
        self.opening.loading = true;
        self.status = Message::Detail(
            "opening_audio",
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
        );
        self.audio_path = Some(path.clone());
        let audio_path = path.clone();
        let (progress, refresh) = Self::track_progress(&self.opening.progress, cx);
        self.processing_cancel = Arc::default();
        let cancel = self.processing_cancel.clone();
        let task = cx.background_spawn(async move {
            Audio::load(&audio_path, |value| {
                anyhow::ensure!(!cancel.load(Relaxed), "Cancelled");
                progress.store(value, Relaxed);
                Ok(())
            })
        });
        self.opening.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await.context(t!("open_audio_failed").into_owned());
            drop(refresh);
            let _ = this.update_in(cx, |this, window, cx| {
                this.opening.task = None;
                this.opening.loading = false;
                match result {
                    Ok(audio) => {
                        this.audio = Some(Arc::new(audio));
                        let (transcripts, errors) = crate::storage::read_transcripts_checked(&path);
                        this.transcripts = transcripts;
                        if !errors.is_empty() {
                            this.playback.error = Some(Message::Error(errors.join("\n")));
                        }
                        if let Some(results) = this.session_transcripts.get(&path) {
                            this.transcripts.extend(results.clone());
                        }
                        let transcript =
                            transcript.filter(|model| this.transcripts.contains_key(model));
                        this.transcript_view.visible = transcript.is_some();
                        let selected = transcript
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
            Some(crate::storage::Transcript::Structured(turns)) => recognition::format_turns(
                turns,
                self.transcript_view.timestamps,
                self.transcript_view.speakers,
            ),
            None => String::new(),
        };
        self.transcript
            .update(cx, |input, cx| input.set_value(text, window, cx));
        self.selected_transcript = model;
        cx.notify();
    }

    fn recognition_model_for(&self, path: &std::path::Path) -> Option<SpeechModel> {
        self.recognition
            .job
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
            || self.opening.choosing
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
        self.recognition.requested = true;
        let segments = self.split_speakers(window, cx);
        self.recognition.job = Some((path.clone(), model));
        let source = path.clone();
        self.status = Message::Empty;
        self.recognition.started = None;
        let (progress, refresh) = Self::track_progress(&self.recognition.progress, cx);
        self.recognition.task = Some(cx.spawn_in(window, async move |this, cx| {
            let segments = segments.await.unwrap_or_default();
            let _ = this.update_in(cx, |this, _, cx| {
                this.recognition.started = Some(Instant::now());
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
                this.recognition.task = None;
                this.recognition.job = None;
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
                        this.transcripts.insert(key.clone(), text);
                        this.transcript_view.visible = true;
                        this.select_transcript(key, window, cx);
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
