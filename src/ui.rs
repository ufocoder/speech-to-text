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
    storage::{SavedRecording, model_cache, recordings_dir},
};
use anyhow::Context as _;
use gpui_kit::component::{
    input::TextareaState,
    slider::{SliderEvent, SliderState, SliderValue},
};
use gpui_kit::*;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) struct Recorder {
    recording: Option<Recording>,
    microphone: Option<(SharedString, cpal::Device)>,
    recognition_path: Option<PathBuf>,
    last_transcript: Option<(PathBuf, String)>,
    loading_audio: bool,
    model: SpeechModel,
    downloading_model: bool,
    choosing_file: bool,
    status: Message,
    language: Language,
    settings_open: bool,
    settings_error: Option<Message>,
    transcript: Entity<TextareaState>,
    audio: Option<Arc<Audio>>,
    audio_path: Option<PathBuf>,
    saved_recordings: Option<Result<Vec<SavedRecording>, Message>>,
    saved_text: Option<(String, Entity<TextareaState>)>,
    player: Option<Player>,
    playback_slider: Entity<SliderState>,
    playback_task: Option<Task<()>>,
    playback_error: Option<Message>,
}

impl Recorder {
    pub(crate) fn new(language: Language, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let transcript = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(t!("transcript_placeholder").into_owned())
        });
        let playback_slider = cx.new(|_| SliderState::new().max(1.).step(0.001));
        cx.subscribe(&playback_slider, |this, _, event, cx| {
            if let (SliderEvent::Change(SliderValue::Single(progress)), Some(audio), Some(player)) =
                (event, this.audio.as_ref(), this.player.as_ref())
            {
                player.seek(f64::from(*progress) * audio.duration());
            }
            cx.notify();
        })
        .detach();
        Self {
            recording: None,
            microphone: None,
            recognition_path: None,
            last_transcript: None,
            loading_audio: false,
            model: SpeechModel::Tone,
            downloading_model: false,
            choosing_file: false,
            status: Message::Empty,
            language,
            settings_open: false,
            settings_error: None,
            transcript,
            audio: None,
            audio_path: None,
            saved_recordings: None,
            saved_text: None,
            player: None,
            playback_slider,
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
        self.playback_slider
            .update(cx, |slider, cx| slider.set_value(0., window, cx));
        self.audio = None;
        self.audio_path = None;
        self.playback_error = None;
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
        self.loading_audio || self.recognition_path.is_some()
    }

    fn load_audio(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_audio(window, cx);
        self.loading_audio = true;
        self.status = Message::Detail("opening_audio", path.display().to_string());
        let audio_path = path.clone();
        let task = cx.background_spawn(async move {
            let audio = Audio::load(&audio_path)?;
            let saved = std::fs::read_to_string(crate::storage::transcript_path(&audio_path)).ok();
            anyhow::Ok((audio, saved))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await.context(t!("open_audio_failed").into_owned());
            let _ = this.update_in(cx, |this, window, cx| {
                this.loading_audio = false;
                match result {
                    Ok((audio, saved)) => {
                        this.audio = Some(Arc::new(audio));
                        if let Some(text) = this
                            .last_transcript
                            .as_ref()
                            .filter(|(source, _)| source == &path)
                            .map(|(_, text)| text.as_str())
                            .or(saved.as_deref())
                        {
                            this.transcript
                                .update(cx, |input, cx| input.set_value(text, window, cx));
                        }
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

    fn recognize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.audio_busy() || self.choosing_file || self.recording.is_some() {
            return;
        }
        let (Some(audio), Some(path)) = (self.audio.clone(), self.audio_path.clone()) else {
            return;
        };
        let Some(directory) = self.model.cached_directory(&model_cache()) else {
            self.status = Message::Text("download_model_hint");
            cx.notify();
            return;
        };
        self.recognition_path = Some(path.clone());
        let source = path.clone();
        let model = self.model;
        let recordings = recordings_dir();
        self.status = Message::Empty;
        let task = cx.background_spawn(async move {
            let text = recognition::transcribe(&audio, model, &directory)?;
            let save = crate::storage::save_transcript(&path, &recordings, &text, model.label());
            anyhow::Ok((text, save))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.recognition_path = None;
                let source_is_open = this.audio_path.as_ref() == Some(&source);
                match result {
                    Ok((text, save)) => {
                        this.last_transcript = Some((source, text.clone()));
                        if !source_is_open {
                            cx.notify();
                            return;
                        }
                        this.status = if text.is_empty() {
                            Message::Text("no_speech")
                        } else {
                            Message::Empty
                        };
                        if let Err(error) = save {
                            this.status = Message::Detail("save_text_error", error.to_string());
                        }
                        this.transcript.update(cx, |input, cx| {
                            input.set_value(text, window, cx);
                        });
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
