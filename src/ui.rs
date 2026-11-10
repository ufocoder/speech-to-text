use rust_i18n::t;
mod layout;
mod menus;
mod playback;
mod recordings;
mod settings;

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
use std::sync::Arc;
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) struct Recorder {
    recording: Option<Recording>,
    microphone: Option<(SharedString, cpal::Device)>,
    recognition_job: Option<(PathBuf, SpeechModel)>,
    session_transcripts: BTreeMap<PathBuf, Transcripts>,
    transcripts: Transcripts,
    selected_transcript: Option<String>,
    loading_audio: bool,
    model: SpeechModel,
    audio_model: Option<SpeechModel>,
    downloading_model: bool,
    download_percent: u8,
    download_progress_task: Option<Task<()>>,
    choosing_file: bool,
    status: Message,
    language: Language,
    settings_open: bool,
    settings_error: Option<Message>,
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
            microphone: None,
            recognition_job: None,
            session_transcripts: BTreeMap::new(),
            transcripts: Transcripts::new(),
            selected_transcript: None,
            loading_audio: false,
            model: SpeechModel::Parakeet,
            audio_model: None,
            downloading_model: false,
            download_percent: 0,
            download_progress_task: None,
            choosing_file: false,
            status: Message::Empty,
            language,
            settings_open: false,
            settings_error: None,
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

    fn clear_audio(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.playback_task = None;
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
        self.status = Message::Detail("opening_audio", path.display().to_string());
        let audio_path = path.clone();
        let task = cx.background_spawn(async move { Audio::load(&audio_path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await.context(t!("open_audio_failed").into_owned());
            let _ = this.update_in(cx, |this, window, cx| {
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
                    Err(error) => this.status = Message::Error(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn select_transcript(
        &mut self,
        model: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.transcripts.get(&model).map_or("", String::as_str);
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

    fn active_audio_model(&self) -> SpeechModel {
        self.audio_model.unwrap_or(self.model)
    }

    fn recognize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.audio_busy() || self.choosing_file || self.recording.is_some() {
            return;
        }
        let (Some(audio), Some(path)) = (self.audio.clone(), self.audio_path.clone()) else {
            return;
        };
        let model = self.active_audio_model();
        let Some(directory) = model.cached_directory(&model_cache()) else {
            self.status = Message::Text("download_model_hint");
            cx.notify();
            return;
        };
        self.recognition_job = Some((path.clone(), model));
        let source = path.clone();
        self.status = Message::Empty;
        let task = cx.background_spawn(async move {
            let text = recognition::transcribe(&audio, model, &directory)?;
            let save = crate::storage::save_transcript(&path, &text, model);
            anyhow::Ok((text, save))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.recognition_job = None;
                let source_is_open = this.audio_path.as_ref() == Some(&source);
                match result {
                    Ok((text, save)) => {
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
        })
        .detach();
        cx.notify();
    }
}
