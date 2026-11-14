use super::Recorder;
use crate::i18n::Message;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, StyledExt,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::Textarea,
    spinner::Spinner,
    tab::{Tab, TabBar},
};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;

impl Render for Recorder {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let foreground = theme.foreground;
        let background = if theme.is_dark() {
            rgb(0x27221e)
        } else {
            rgb(0xf3eee7)
        };
        let content = if self.settings_open {
            self.settings_content(cx).into_any_element()
        } else if let Some(recordings) = &self.saved_recordings {
            self.recordings_table(recordings, cx).into_any_element()
        } else if let Some(path) = &self.audio_path {
            self.audio_view(path, cx).into_any_element()
        } else {
            self.start_view(cx).into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(background)
            .text_color(foreground)
            .child(self.navigation(cx))
            .child(div().flex_1().min_h_0().child(content))
    }
}

impl Recorder {
    fn navigation(&self, cx: &Context<Self>) -> Div {
        let home_active =
            !self.settings_open && self.saved_recordings.is_none() && self.audio_path.is_none();
        div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_3()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .child(t!("title").into_owned()),
            )
            .child(
                div()
                    .ml_auto()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("home")
                            .label(t!("home").into_owned())
                            .icon(IconName::House)
                            .when(home_active, |button| button.primary())
                            .when(!home_active, |button| button.ghost())
                            .disabled(
                                self.recording.is_some()
                                    || self.loading_audio
                                    || self.choosing_file,
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                let from_archive = this.saved_recordings.take().is_some();
                                let from_settings = std::mem::take(&mut this.settings_open);
                                if !from_archive && !from_settings {
                                    this.clear_audio(window, cx);
                                    this.status = Message::Empty;
                                }
                                cx.notify();
                            })),
                    )
                    .child(self.recordings_button(cx))
                    .child(self.settings_button(cx)),
            )
    }

    fn start_view(&self, cx: &Context<Self>) -> Div {
        let recording = self.recording.is_some();
        let theme = cx.theme();
        let button_style = ButtonCustomVariant::new(cx)
            .foreground(theme.foreground)
            .hover(if theme.is_dark() {
                rgb(0x473831).into()
            } else {
                rgb(0xffffff).into()
            });
        let record = Button::new("record")
            .large()
            .h(px(64.))
            .icon(if recording {
                IconName::Square
            } else {
                IconName::Mic
            })
            .label(if recording {
                t!("stop_recording").into_owned()
            } else {
                t!("make_recording").into_owned()
            })
            .disabled(self.loading_audio || self.choosing_file)
            .on_click(cx.listener(|this, _, window, cx| this.toggle(window, cx)));
        let actions = if recording {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_center()
                .gap_3()
                .child(record.danger())
                .child(div().text_sm().child(t!("recording").into_owned()))
        } else {
            div()
                .flex()
                .items_center()
                .max_w_full()
                .rounded_lg()
                .overflow_hidden()
                .border_1()
                .border_color(theme.border)
                .bg(theme.background)
                .child(record.custom(button_style.clone()).rounded_none())
                .child(div().w(px(1.)).h(px(64.)).bg(theme.border))
                .child(
                    Button::new("audio-file")
                        .large()
                        .h(px(64.))
                        .rounded_none()
                        .icon(IconName::FolderOpen)
                        .custom(button_style)
                        .label(t!("open_audio").into_owned())
                        .disabled(self.loading_audio || self.choosing_file)
                        .on_click(cx.listener(|this, _, window, cx| this.choose_file(window, cx))),
                )
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .p_4()
            .child(actions)
            .when(
                !recording && !matches!(self.status, Message::Empty),
                |view| {
                    view.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .when(self.loading_audio, |row| row.child(Spinner::new().small()))
                            .child(self.status.render())
                            .when(self.loading_audio, |row| {
                                row.child(super::progress_label(&self.opening_progress))
                            }),
                    )
                },
            )
            .when(self.recognition_job.is_some(), |view| {
                view.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .child(Spinner::new().small())
                        .child(t!("recognizing").into_owned()),
                )
            })
    }

    fn audio_view(&self, path: &std::path::Path, cx: &mut Context<Self>) -> impl IntoElement {
        let file = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        let delete_path = path.to_path_buf();
        let busy = self.recognition_job.is_some()
            || self.speaker_task.is_some()
            || self.speaker_download.is_some();
        let processing = self.recognition_model_for(path);
        let selected_model = self.active_audio_model();
        let model_ready = selected_model.is_some();
        let has_text = !self.transcript.read(cx).value().trim().is_empty();
        let theme = cx.theme();
        let border = theme.border;
        let card = theme.background;
        let muted = theme.muted_foreground;
        let content = div()
            .h_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div().min_w_0().child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(Icon::from(IconName::AudioLines))
                                .child(div().min_w_0().truncate().text_xl().font_semibold().child(
                                    t!("audio_file_title", name = file.as_ref()).into_owned(),
                                )),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("reset-audio")
                                    .label(t!("reset_audio").into_owned())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.reset_audio(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("delete-audio")
                                    .outline()
                                    .icon(IconName::Trash)
                                    .accessibility_label(t!("delete").into_owned())
                                    .tooltip(t!("delete").into_owned())
                                    .disabled(self.loading_audio || processing.is_some())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.confirm_delete(delete_path.clone(), window, cx);
                                    })),
                            ),
                    ),
            )
            .when(self.loading_audio, |view| {
                view.child(
                    div()
                        .rounded_lg()
                        .border_1()
                        .border_color(border)
                        .bg(card)
                        .p_4()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Spinner::new().small())
                        .child(t!("preparing_audio").into_owned())
                        .child(super::progress_label(&self.opening_progress)),
                )
            })
            .children(self.playback_panel(cx).map(|player| {
                div()
                    .rounded_lg()
                    .border_1()
                    .border_color(border)
                    .bg(card)
                    .p_3()
                    .child(player)
            }))
            .when(!self.loading_audio, |view| {
                view.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_3()
                        .child(
                            Button::new("transcribe-audio")
                                .primary()
                                .label(t!("transcribe").into_owned())
                                .disabled(busy || !model_ready)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.recognize(window, cx)),
                                ),
                        )
                        .when(
                            !(processing.is_some() || self.speaker_task.is_some()),
                            |row| {
                                row.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap_3()
                                        .when_some(selected_model, |row, model| {
                                            row.child(self.audio_model_menu(model, cx))
                                        })
                                        .child(self.speaker_model_menu(cx)),
                                )
                            },
                        )
                        .when(
                            processing.is_some() || self.speaker_task.is_some(),
                            |view| {
                                let splitting = self.speaker_task.is_some();
                                let status = match (processing.is_some(), splitting) {
                                    (true, true) => t!("processing_both"),
                                    (true, false) => t!("processing_recognition"),
                                    _ => t!("processing_speakers"),
                                };
                                let remaining = [
                                    (
                                        processing.is_some(),
                                        self.recognition_started,
                                        &self.recognition_progress,
                                    ),
                                    (splitting, self.speaker_started, &self.speaker_progress),
                                ]
                                .into_iter()
                                .filter(|(active, _, _)| *active)
                                .filter_map(|(_, started, progress)| {
                                    super::estimate_seconds(
                                        started?.elapsed(),
                                        progress.load(super::Relaxed),
                                    )
                                })
                                .max()
                                .map(super::format_remaining_time)
                                .unwrap_or_else(|| t!("estimating_time").into_owned());
                                view.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .text_sm()
                                        .text_color(muted)
                                        .child(Spinner::new().small())
                                        .child(format!("{status} · {remaining}")),
                                )
                            },
                        ),
                )
            })
            .when(
                !self.loading_audio && !model_ready && processing.is_none(),
                |view| {
                    view.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(t!("download_model_hint").into_owned()),
                    )
                },
            )
            .when(
                !self.loading_audio && !matches!(self.status, Message::Empty),
                |view| view.child(div().text_sm().child(self.status.render())),
            )
            .children(
                self.playback_error
                    .iter()
                    .map(|error| div().text_sm().child(error.render())),
            )
            .when(!self.loading_audio, |view| view.child(self.speaker_panel()))
            .when(!self.transcripts.is_empty(), |view| {
                view.child(
                    div()
                        .flex_1()
                        .min_h(px(180.))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(border)
                        .bg(card)
                        .p_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    div()
                                        .mr_auto()
                                        .font_semibold()
                                        .child(t!("transcript").into_owned()),
                                )
                                .when(
                                    matches!(
                                        self.transcripts.get(&self.selected_transcript),
                                        Some(crate::storage::Transcript::Structured(_))
                                    ),
                                    |card| {
                                        card.child(
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .items_center()
                                                .gap_3()
                                                .child(
                                                    Button::new("include-speakers")
                                                        .when(self.include_speakers, |button| {
                                                            button.primary()
                                                        })
                                                        .when(!self.include_speakers, |button| {
                                                            button.outline()
                                                        })
                                                        .label(t!("include_speakers").into_owned())
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                this.include_speakers =
                                                                    !this.include_speakers;
                                                                this.select_transcript(
                                                                    this.selected_transcript
                                                                        .clone(),
                                                                    window,
                                                                    cx,
                                                                );
                                                            },
                                                        )),
                                                )
                                                .child(
                                                    Button::new("include-timestamps")
                                                        .when(self.include_timestamps, |button| {
                                                            button.primary()
                                                        })
                                                        .when(!self.include_timestamps, |button| {
                                                            button.outline()
                                                        })
                                                        .label(
                                                            t!("include_timestamps").into_owned(),
                                                        )
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                this.include_timestamps =
                                                                    !this.include_timestamps;
                                                                this.select_transcript(
                                                                    this.selected_transcript
                                                                        .clone(),
                                                                    window,
                                                                    cx,
                                                                );
                                                            },
                                                        )),
                                                ),
                                        )
                                    },
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(border)
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .px_2()
                                                .text_sm()
                                                .child(t!("transcript_font").into_owned()),
                                        )
                                        .children(
                                            [
                                                ("font-smaller", "−", -1, "font_decrease"),
                                                ("font-larger", "+", 1, "font_increase"),
                                            ]
                                            .map(
                                                |(id, label, step, description)| {
                                                    div().border_l_1().border_color(border).child(
                                                        Button::new(id)
                                                            .ghost()
                                                            .rounded_none()
                                                            .label(label)
                                                            .accessibility_label(
                                                                t!(description).into_owned(),
                                                            )
                                                            .disabled(if step < 0 {
                                                                self.transcript_font_size <= 12
                                                            } else {
                                                                self.transcript_font_size >= 28
                                                            })
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.transcript_font_size =
                                                                        this.transcript_font_size
                                                                            .saturating_add_signed(
                                                                                step,
                                                                            )
                                                                            .clamp(12, 28);
                                                                    cx.notify();
                                                                },
                                                            )),
                                                    )
                                                },
                                            ),
                                        ),
                                )
                                .child(
                                    Button::new("copy-transcript")
                                        .disabled(!has_text)
                                        .icon(IconName::Copy)
                                        .accessibility_label(t!("save_to_clipboard").into_owned())
                                        .tooltip(t!("save_to_clipboard").into_owned())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            let text = this.transcript.read(cx).value().to_string();
                                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                                        })),
                                ),
                        )
                        .child(
                            TabBar::new("transcript-tabs")
                                .underline()
                                .selected_index(
                                    self.transcripts
                                        .keys()
                                        .position(|model| model == &self.selected_transcript)
                                        .unwrap_or_default(),
                                )
                                .children(self.transcripts.keys().map(|model| {
                                    let label = model.clone().unwrap_or_else(|| {
                                        t!("unknown_transcript_model").into_owned()
                                    });
                                    Tab::new().label(label)
                                }))
                                .on_click(cx.listener(|this, index, window, cx| {
                                    if let Some(model) =
                                        this.transcripts.keys().nth(*index).cloned()
                                    {
                                        this.select_transcript(model, window, cx);
                                    }
                                })),
                        )
                        .when(!has_text, |view| {
                            view.child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(t!("no_speech").into_owned()),
                            )
                        })
                        .when(has_text, |view| {
                            view.child(
                                Textarea::new(&self.transcript)
                                    .flex_1()
                                    .min_h(px(96.))
                                    .text_size(px(f32::from(self.transcript_font_size)))
                                    .bordered(false)
                                    .appearance(false)
                                    .readonly(true),
                            )
                        }),
                )
            });
        div()
            .id("audio-view-scroll")
            .size_full()
            .overflow_y_scroll()
            .child(content)
    }
}
