use super::{Recorder, settings::model_columns};
use crate::{
    models::{SPEAKER_MODELS, SpeakerModel, download_speaker_model},
    storage::model_cache,
};
use gpui_kit::component::{
    ActiveTheme, StyledExt,
    group_box::{GroupBox, GroupBoxVariant, GroupBoxVariants},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::component::{Disableable, button::Button};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;
use std::sync::Arc;

impl Recorder {
    pub(super) fn speaker_model_settings(&self, cx: &Context<Self>) -> impl IntoElement {
        let cache = model_cache().join("speaker-diarization");
        let rows = SPEAKER_MODELS
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 1)
            .map(|(index, &(label, repo, file, _))| {
                let model = SpeakerModel::ALL
                    .into_iter()
                    .find(|model| model.segmentation_index() == index);
                let selected = model == Some(self.default_speaker_model);
                let installed = cache.join(file).is_file();
                let downloading = self.speaker_download == Some(index);
                let busy = self.speaker_download.is_some() || self.speaker_task.is_some();
                let view = cx.entity().downgrade();
                let status = if downloading {
                    let percent = self.speaker_download_progress.load(super::Relaxed);
                    format!(
                        "{percent}% · {}",
                        super::remaining_time(self.speaker_download_started, percent)
                    )
                } else if installed {
                    t!("speaker_installed").into_owned()
                } else {
                    t!("model_not_downloaded").into_owned()
                };
                let name = div().child(label).child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if selected {
                            format!("{} · {status}", t!("system_default"))
                        } else {
                            status
                        }),
                );
                let updated = self.model_update_dates.get(repo).map_or_else(
                    || "—".to_owned(),
                    |date| date.format(&t!("model_update_date_format")).to_string(),
                );
                let size = match index {
                    0 => "5.99",
                    2 => "1.54",
                    3 => "9.51",
                    _ => "—",
                };
                let actions = div().child(
                    Button::new(("speaker-actions", index))
                        .label("⋯")
                        .accessibility_label(t!("model_actions").into_owned())
                        .disabled(busy || (installed && model.is_none()))
                        .dropdown_menu(move |menu, _, _| {
                            let download_view = view.clone();
                            menu.when(!installed, |menu| {
                                menu.item(
                                    PopupMenuItem::new(t!("download_model").into_owned()).on_click(
                                        move |_, _, cx| {
                                            let _ = download_view.update(cx, |this, cx| {
                                                this.download_speaker(index, cx)
                                            });
                                        },
                                    ),
                                )
                            })
                            .when_some(model, |menu, model| {
                                let view = view.clone();
                                menu.item(
                                    PopupMenuItem::new(t!("speaker_set_default").into_owned())
                                        .disabled(selected)
                                        .on_click(move |_, _, cx| {
                                            let _ = view.update(cx, |this, cx| {
                                                match crate::storage::save_default_speaker_model(
                                                    model,
                                                ) {
                                                    Ok(()) => {
                                                        this.default_speaker_model = model;
                                                        this.speaker_model = model;
                                                        this.settings_error = None;
                                                    }
                                                    Err(error) => {
                                                        this.settings_error =
                                                            Some(crate::i18n::Message::Error(
                                                                error.to_string(),
                                                            ))
                                                    }
                                                }
                                                cx.notify();
                                            });
                                        }),
                                )
                            })
                        }),
                );
                model_columns(
                    name,
                    "—".to_owned(),
                    size.to_owned(),
                    div().child(t!("speaker_language_independent").into_owned()),
                    updated,
                    actions,
                )
                .border_t_1()
                .border_color(cx.theme().border)
                .when(selected, |row| row.bg(cx.theme().muted))
            })
            .collect::<Vec<_>>();
        GroupBox::new()
            .id("speaker-model-settings")
            .with_variant(GroupBoxVariant::Outline)
            .title(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(t!("speaker_catalog").into_owned())
                    .child(
                        Button::new("search-speaker-models")
                            .icon(gpui_kit::assets::IconName::Search)
                            .label(t!("search_models").into_owned())
                            .on_click(
                                cx.listener(|this, _, _, cx| this.open_model_catalog(true, cx)),
                            ),
                    ),
            )
            .gap_2()
            .child(
                div()
                    .id("speaker-table-scroll")
                    .min_w_0()
                    .overflow_x_scroll()
                    .child(
                        div()
                            .min_w(px(580.))
                            .child(
                                model_columns(
                                    div().child(t!("model_column").into_owned()),
                                    t!("model_parameters").into_owned(),
                                    t!("model_size").into_owned(),
                                    div().child(t!("model_languages").into_owned()),
                                    t!("model_updated").into_owned(),
                                    div(),
                                )
                                .font_semibold(),
                            )
                            .when(rows.is_empty(), |table| {
                                table.child(t!("speaker_no_matches").into_owned())
                            })
                            .children(rows),
                    ),
            )
    }

    pub(super) fn download_speaker(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.speaker_download.is_some() || self.speaker_task.is_some() {
            return;
        }
        self.speaker_download = Some(index);
        self.speaker_download_started = Some(super::Instant::now());
        self.settings_error = None;
        let (progress, refresh) = Self::track_progress(&self.speaker_download_progress, cx);
        let task = cx.background_spawn(async move {
            download_speaker_model(index, &model_cache().join("speaker-diarization"), |value| {
                progress.store(value, super::Relaxed)
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            drop(refresh);
            let _ = this.update(cx, |this, cx| {
                this.speaker_download = None;
                this.settings_error = result
                    .err()
                    .map(|error| crate::i18n::Message::Error(error.to_string()));
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn downloaded_speaker_models() -> impl Iterator<Item = SpeakerModel> {
        let cache = model_cache().join("speaker-diarization");
        SpeakerModel::ALL.into_iter().filter(move |model| {
            cache
                .join(SPEAKER_MODELS[model.segmentation_index()].2)
                .is_file()
        })
    }

    fn effective_speaker_model(&self) -> SpeakerModel {
        let mut installed = Self::downloaded_speaker_models();
        let first = installed.next();
        if first == Some(self.speaker_model) || installed.any(|model| model == self.speaker_model) {
            self.speaker_model
        } else {
            first.unwrap_or(self.speaker_model)
        }
    }

    pub(super) fn speaker_model_menu(&self, cx: &Context<Self>) -> impl IntoElement {
        use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

        let selected = self.effective_speaker_model();
        let has_models = Self::downloaded_speaker_models().next().is_some();
        let recorder = cx.entity().downgrade();
        Button::new("speaker-model-select")
            .label(t!("speaker_model_selection", model = selected.label()).into_owned())
            .disabled(!has_models || self.audio_busy() || self.speaker_task.is_some())
            .dropdown_menu(move |menu, _, _| {
                Self::downloaded_speaker_models().fold(menu, |menu, model| {
                    let recorder = recorder.clone();
                    menu.item(
                        PopupMenuItem::new(model.label())
                            .checked(model == selected)
                            .on_click(move |_, _, cx| {
                                let _ = recorder.update(cx, |this, cx| {
                                    if !this.audio_busy() && this.speaker_task.is_none() {
                                        this.speaker_model = model;
                                        cx.notify();
                                    }
                                });
                            }),
                    )
                })
            })
    }

    pub(super) fn speaker_panel(&self) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when_some(self.speaker_result.as_ref(), |panel, result| match result {
                Err(error) => panel.child(div().text_sm().child(error.clone())),
                Ok(segments) if segments.is_empty() => {
                    panel.child(t!("speaker_empty").into_owned())
                }
                Ok(_) => panel,
            })
    }

    pub(super) fn split_speakers(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> futures::channel::oneshot::Receiver<Vec<sherpa_onnx::OfflineSpeakerDiarizationSegment>>
    {
        let (send, receive) = futures::channel::oneshot::channel();
        let Some(audio) = self.audio.clone() else {
            return receive;
        };
        self.speaker_result = None;
        self.speaker_started = Some(super::Instant::now());
        let (progress, refresh) = Self::track_progress(&self.speaker_progress, cx);
        let cancel = self.processing_cancel.clone();
        let segmentation = self.effective_speaker_model().segmentation_index();
        self.speaker_task = Some(cx.spawn_in(window, async move |this, cx| {
            let worker_audio = audio.clone();
            let result = cx
                .background_spawn(async move {
                    let directory = model_cache().join("speaker-diarization");
                    for index in [segmentation, 1] {
                        let (_, _, file, _) = SPEAKER_MODELS[index];
                        anyhow::ensure!(!cancel.load(super::Relaxed), "Cancelled");
                        if !directory.join(file).is_file() {
                            download_speaker_model(index, &directory, |_| {})?;
                        }
                    }
                    crate::diarization::split(
                        &worker_audio,
                        &directory,
                        SPEAKER_MODELS[segmentation].2,
                        |value| {
                            anyhow::ensure!(!cancel.load(super::Relaxed), "Cancelled");
                            progress.store(value, super::Relaxed);
                            Ok(())
                        },
                    )
                })
                .await;
            drop(refresh);
            if let Ok(segments) = &result {
                let _ = send.send(segments.clone());
            }
            let _ = this.update_in(cx, |this, _, cx| {
                this.speaker_task = None;
                if this
                    .audio
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &audio))
                {
                    this.speaker_result = Some(result.map_err(|error| error.to_string()));
                }
                cx.notify();
            });
        }));
        cx.notify();
        receive
    }
}
