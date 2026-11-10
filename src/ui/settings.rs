use super::Recorder;
use crate::{
    i18n::Message,
    models::SpeechModel,
    storage::{model_cache, recordings_dir, save_recordings_dir},
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, StyledExt,
    button::{Button, ButtonVariants},
    group_box::{GroupBox, GroupBoxVariant, GroupBoxVariants},
    radio::{Radio, RadioGroup},
};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering::Relaxed},
    },
    time::Duration,
};

impl Recorder {
    pub(super) fn settings_button(&self, cx: &Context<Self>) -> Button {
        Button::new("settings")
            .when(self.settings_open, |button| button.primary())
            .when(!self.settings_open, |button| button.ghost())
            .icon(IconName::Settings)
            .label(t!("settings").into_owned())
            .disabled(self.audio_busy() || self.choosing_file || self.recording.is_some())
            .on_click(cx.listener(|this, _, _, cx| {
                this.saved_recordings = None;
                this.settings_open = true;
                cx.notify();
            }))
    }

    pub(super) fn settings_content(&self, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        div()
            .id("settings-content")
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .text_xl()
                            .font_semibold()
                            .child(t!("settings").into_owned()),
                    )
                    .child(
                        GroupBox::new()
                            .id("general-settings")
                            .with_variant(GroupBoxVariant::Outline)
                            .gap_3()
                            .child(setting_row(
                                t!("settings_microphone").into_owned(),
                                self.microphone_menu(view.clone()),
                            ))
                            .child(setting_row(
                                t!("recordings_folder").into_owned(),
                                Button::new("choose-recordings-directory")
                                    .label(t!("choose_folder").into_owned())
                                    .disabled(self.choosing_file)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.choose_recordings_dir(window, cx)
                                    })),
                            ))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(recordings_dir().display().to_string()),
                            )
                            .child(setting_row(
                                t!("language").into_owned(),
                                self.language_menu(view),
                            )),
                    )
                    .child(self.model_settings(cx)),
            )
    }

    fn model_settings(&self, cx: &Context<Self>) -> impl IntoElement {
        let models = [
            SpeechModel::Tone,
            SpeechModel::GigaAm,
            SpeechModel::Parakeet,
        ];
        let cached = self.model.cached_directory(&model_cache()).is_some();
        let model_status = if cached {
            "model_downloaded"
        } else {
            "model_not_downloaded"
        };
        GroupBox::new()
            .id("model-settings")
            .with_variant(GroupBoxVariant::Outline)
            .title(t!("recognition_model_label").into_owned())
            .gap_3()
            .child(
                RadioGroup::vertical("recognition-models")
                    .selected_index(models.iter().position(|model| *model == self.model))
                    .disabled(
                        self.downloading_model || self.audio_busy() || self.recording.is_some(),
                    )
                    .children(models.into_iter().map(|model| {
                        Radio::new(model.label()).label(
                            t!(
                                "model_menu_entry",
                                name = model.label(),
                                size = model.download_size_mb(),
                                languages = model.languages_summary()
                            )
                            .into_owned(),
                        )
                    }))
                    .on_change(cx.listener(move |this, index, _, cx| {
                        if !this.downloading_model && !this.audio_busy() && this.recording.is_none()
                        {
                            this.model = models[*index];
                            this.settings_error = None;
                            cx.notify();
                        }
                    })),
            )
            .when(self.model == SpeechModel::Parakeet, |group| {
                group.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("model_languages_parakeet_full").into_owned()),
                )
            })
            .child(setting_row(
                if self.downloading_model {
                    t!("model_download_progress", percent = self.download_percent).into_owned()
                } else {
                    t!(model_status).into_owned()
                },
                div().when(!cached || self.downloading_model, |row| {
                    row.child(
                        Button::new("download-model")
                            .primary()
                            .label(t!("download_model").into_owned())
                            .disabled(self.downloading_model)
                            .on_click(cx.listener(|this, _, _, cx| this.download_model(cx))),
                    )
                }),
            ))
            .when(self.downloading_model, |view| {
                view.child(
                    div()
                        .w_full()
                        .max_w(px(360.))
                        .h(px(6.))
                        .rounded_full()
                        .overflow_hidden()
                        .bg(cx.theme().muted)
                        .child(
                            div()
                                .h_full()
                                .w(relative(f32::from(self.download_percent) / 100.))
                                .bg(cx.theme().primary),
                        ),
                )
            })
            .children(
                self.settings_error
                    .iter()
                    .map(|error| div().text_sm().child(error.render())),
            )
    }

    fn download_model(&mut self, cx: &mut Context<Self>) {
        if self.downloading_model {
            return;
        }
        self.downloading_model = true;
        self.download_percent = 0;
        self.settings_error = None;
        let model = self.model;
        let progress = Arc::new(AtomicU8::new(0));
        let worker_progress = progress.clone();
        let task = cx.background_spawn(async move {
            model.prepare(&model_cache(), |bytes, total| {
                worker_progress.store((bytes * 100 / total).min(99) as u8, Relaxed);
            })
        });
        let executor = cx.background_executor().clone();
        self.download_progress_task = Some(cx.spawn(async move |this, cx| {
            loop {
                executor.timer(Duration::from_millis(120)).await;
                let active = this.update(cx, |this, cx| {
                    if !this.downloading_model {
                        return false;
                    }
                    let percent = progress.load(Relaxed);
                    if this.download_percent != percent {
                        this.download_percent = percent;
                        cx.notify();
                    }
                    true
                });
                if !matches!(active, Ok(true)) {
                    break;
                }
            }
        }));
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.downloading_model = false;
                this.download_progress_task = None;
                if let Err(error) = result {
                    this.settings_error = Some(Message::Error(format!("{error:#}")));
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn choose_recordings_dir(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.choosing_file {
            return;
        }
        self.choosing_file = true;
        self.settings_error = None;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t!("choose_folder").into_owned().into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = selection.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.choosing_file = false;
                let error = match result {
                    Ok(Ok(Some(paths))) => paths
                        .first()
                        .and_then(|path| save_recordings_dir(path).err()),
                    Ok(Ok(None)) => None,
                    Ok(Err(error)) => {
                        this.settings_error =
                            Some(Message::Detail("save_folder_error", error.to_string()));
                        cx.notify();
                        return;
                    }
                    Err(_) => {
                        this.settings_error = Some(Message::Text("file_picker_unavailable"));
                        cx.notify();
                        return;
                    }
                };
                this.settings_error =
                    error.map(|error| Message::Detail("save_folder_error", error.to_string()));
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

fn setting_row(title: String, field: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_2()
        .child(title)
        .child(field)
}
