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
    menu::{DropdownMenu, PopupMenuItem},
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
            .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx)))
    }

    pub(crate) fn open_settings(&mut self, cx: &mut Context<Self>) {
        if self.audio_busy() || self.choosing_file || self.recording.is_some() {
            return;
        }
        self.saved_recordings = None;
        if !self.settings_open {
            self.settings_open = true;
            self.refresh_model_update_dates(cx);
        }
        cx.notify();
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
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(t!("recordings_folder").into_owned())
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(recordings_dir().display().to_string()),
                                            ),
                                    )
                                    .child(
                                        Button::new("choose-recordings-directory")
                                            .label(t!("change_folder").into_owned())
                                            .disabled(self.choosing_file)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.choose_recordings_dir(window, cx)
                                            })),
                                    ),
                            )
                            .child(setting_row(
                                t!("language").into_owned(),
                                self.language_menu(view),
                            )),
                    )
                    .child(self.model_settings(cx))
                    .child(self.speaker_model_settings(cx)),
            )
    }

    fn model_settings(&self, cx: &Context<Self>) -> impl IntoElement {
        let cache = model_cache();
        let header = model_columns(
            div().child(t!("model_column").into_owned()),
            t!("model_parameters").into_owned(),
            t!("model_size").into_owned(),
            div().child(t!("model_languages").into_owned()),
            t!("model_updated").into_owned(),
            div(),
        )
        .font_semibold();
        GroupBox::new()
            .id("model-settings")
            .with_variant(GroupBoxVariant::Outline)
            .title(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(t!("recognition_model_label").into_owned())
                    .child(
                        Button::new("search-models")
                            .icon(IconName::Search)
                            .label(t!("search_models").into_owned())
                            .on_click(
                                cx.listener(|this, _, _, cx| this.open_model_catalog(false, cx)),
                            ),
                    ),
            )
            .gap_3()
            .child(
                div()
                    .id("model-table-scroll")
                    .min_w_0()
                    .overflow_x_scroll()
                    .child(
                        div()
                            .min_w(px(580.))
                            .flex()
                            .flex_col()
                            .child(header)
                            .children(SpeechModel::ALL.into_iter().filter_map(|model| {
                                let cached = model.cached_directory(&cache).is_some();
                                (model != SpeechModel::WhisperTurbo
                                    || self.catalog_models.contains(&model)
                                    || cached)
                                    .then(|| self.model_row(model, cached, cx))
                            })),
                    ),
            )
            .children(
                self.settings_error
                    .iter()
                    .map(|error| div().text_sm().child(error.render())),
            )
    }

    fn model_row(&self, model: SpeechModel, cached: bool, cx: &Context<Self>) -> Div {
        let active = self.model == model;
        let downloading = self.downloading_model == Some(model);
        let blocked = self.model_actions_blocked();
        let view = cx.entity().downgrade();
        let muted = cx.theme().muted_foreground;
        let name = div()
            .font_semibold()
            .child(model.label())
            .when(active, |cell| {
                cell.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(t!("active_model").into_owned()),
                )
            })
            .when(!cached && !downloading, |cell| {
                cell.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(t!("model_not_downloaded").into_owned()),
                )
            })
            .when(downloading, |cell| {
                cell.child(
                    div()
                        .flex_1()
                        .text_sm()
                        .child(format!("{}%", self.download_percent))
                        .child(super::remaining_time(
                            self.model_download_started,
                            self.download_percent,
                        ))
                        .child(
                            div()
                                .mt_1()
                                .h(px(3.))
                                .rounded_full()
                                .overflow_hidden()
                                .bg(cx.theme().border)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(f32::from(self.download_percent) / 100.))
                                        .bg(cx.theme().primary),
                                ),
                        ),
                )
            });
        let actions = div().child(
            Button::new(SharedString::from(format!("model-actions-{}", model.id())))
                .ghost()
                .label("⋯")
                .accessibility_label(t!("model_actions").into_owned())
                .disabled(blocked)
                .dropdown_menu(move |menu, _, _| {
                    let activate_view = view.clone();
                    let delete_view = view.clone();
                    let download_view = view.clone();
                    menu.item(
                        PopupMenuItem::new(t!("activate_model").into_owned())
                            .disabled(active)
                            .on_click(move |_, _, cx| {
                                let _ = activate_view.update(cx, |this, cx| {
                                    if !this.model_actions_blocked() {
                                        this.model = model;
                                        cx.notify();
                                    }
                                });
                            }),
                    )
                    .when(cached, |menu| {
                        menu.item(PopupMenuItem::new(t!("delete").into_owned()).on_click(
                            move |_, _, cx| {
                                let _ = delete_view.update(cx, |this, cx| {
                                    if !this.model_actions_blocked() {
                                        this.settings_error = model
                                            .delete_cached(&model_cache())
                                            .err()
                                            .map(|error| Message::Error(error.to_string()));
                                        cx.notify();
                                    }
                                });
                            },
                        ))
                    })
                    .when(!cached, |menu| {
                        menu.item(
                            PopupMenuItem::new(t!("download_model").into_owned()).on_click(
                                move |_, _, cx| {
                                    let _ = download_view
                                        .update(cx, |this, cx| this.download_model(model, cx));
                                },
                            ),
                        )
                    })
                }),
        );
        let languages = if model == SpeechModel::Parakeet {
            div().child(
                Button::new("expand-parakeet-languages")
                    .ghost()
                    .label(t!("parakeet_languages_short").into_owned())
                    .icon(if self.model_languages_expanded {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.model_languages_expanded = !this.model_languages_expanded;
                        cx.notify();
                    })),
            )
        } else {
            div().child(model.languages_summary())
        };
        let date = self.model_update_dates.get(model.id()).map_or_else(
            || "—".to_owned(),
            |date| date.format(&t!("model_update_date_format")).to_string(),
        );
        div()
            .border_t_1()
            .border_color(cx.theme().border)
            .when(active, |row| row.bg(cx.theme().muted))
            .child(model_columns(
                name,
                t!("model_parameter_count", count = model.parameters_millions()).into_owned(),
                model.download_size_mb().to_string(),
                languages,
                date,
                actions,
            ))
            .when(
                model == SpeechModel::Parakeet && self.model_languages_expanded,
                |row| {
                    row.child(
                        div()
                            .px_2()
                            .pb_3()
                            .text_sm()
                            .text_color(muted)
                            .child(t!("model_languages_parakeet_full").into_owned()),
                    )
                },
            )
    }

    fn model_actions_blocked(&self) -> bool {
        self.downloading_model.is_some() || self.audio_busy() || self.recording.is_some()
    }

    fn download_model(&mut self, model: SpeechModel, cx: &mut Context<Self>) {
        if self.model_actions_blocked() {
            return;
        }
        self.downloading_model = Some(model);
        self.download_percent = 0;
        self.model_download_started = Some(super::Instant::now());
        self.settings_error = None;
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
                    if this.downloading_model.is_none() {
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
                this.downloading_model = None;
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

pub(super) fn model_columns(
    name: Div,
    parameters: String,
    size: String,
    languages: Div,
    updated: String,
    actions: Div,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_2()
        .text_sm()
        .child(name.flex_1().min_w_0())
        .child(
            div()
                .w(px(92.))
                .flex_shrink_0()
                .text_right()
                .whitespace_nowrap()
                .child(parameters),
        )
        .child(
            div()
                .w(px(88.))
                .flex_shrink_0()
                .text_right()
                .whitespace_nowrap()
                .child(size),
        )
        .child(languages.w(px(104.)).flex_shrink_0())
        .child(
            div()
                .w(px(110.))
                .flex_shrink_0()
                .text_right()
                .whitespace_nowrap()
                .child(updated),
        )
        .child(actions.w(px(36.)).flex_shrink_0().flex().justify_end())
}
