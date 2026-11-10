use super::Recorder;
use crate::{
    i18n::Message,
    storage::{SavedRecording, saved_recordings, transcript_model, transcript_path},
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    input::{Textarea, TextareaState},
};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;
use std::path::{Path, PathBuf};

impl Recorder {
    pub(super) fn recordings_button(&self, cx: &Context<Self>) -> Button {
        Button::new("saved-recordings")
            .label(t!("archive").into_owned())
            .icon(IconName::Archive)
            .when(self.saved_recordings.is_some(), |button| button.primary())
            .when(self.saved_recordings.is_none(), |button| button.ghost())
            .disabled(self.loading_audio || self.choosing_file || self.recording.is_some())
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings_open = false;
                this.saved_text = None;
                this.refresh_recordings(cx);
            }))
    }

    fn refresh_recordings(&mut self, cx: &mut Context<Self>) {
        self.saved_recordings = Some(
            saved_recordings()
                .map_err(|error| Message::Detail("recordings_error", error.to_string())),
        );
        cx.notify();
    }

    pub(super) fn recordings_table(
        &self,
        recordings: &Result<Vec<SavedRecording>, Message>,
        cx: &Context<Self>,
    ) -> Div {
        let border = cx.theme().border;
        let header = div()
            .flex()
            .items_center()
            .px_3()
            .py_2()
            .bg(cx.theme().muted)
            .child(div().flex_1().child(t!("file").into_owned()))
            .child(div().w(px(300.)).child(t!("actions").into_owned()));
        let rows = match recordings {
            Ok(paths) if paths.is_empty() => div()
                .p_3()
                .child(t!("no_recordings").into_owned())
                .into_any_element(),
            Ok(paths) => div()
                .id("recordings-rows")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(paths.iter().enumerate().map(|(index, recording)| {
                    let path = &recording.path;
                    let recorded_at = recording.recorded_at.map_or_else(
                        || t!("recording_time_unknown").into_owned(),
                        |time| {
                            let local = chrono::DateTime::<chrono::Local>::from(time);
                            let time = local.format(&t!("recording_time_format")).to_string();
                            t!("recording_time", time = time).into_owned()
                        },
                    );
                    let open = path.clone();
                    let delete = path.clone();
                    let read = path.clone();
                    let has_text = transcript_path(path).is_file();
                    let result = if has_text {
                        let model = transcript_model(path)
                            .unwrap_or_else(|| t!("unknown_transcript_model").into_owned());
                        t!("saved_transcript_model", model = model).into_owned()
                    } else {
                        t!("no_saved_transcript").into_owned()
                    };
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_t_1()
                        .border_color(border)
                        .px_3()
                        .py_2()
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .child(div().truncate().child(file_name(path)))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(recorded_at),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(result),
                                ),
                        )
                        .child(
                            div()
                                .w(px(300.))
                                .flex_shrink_0()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new(("open-recording", index))
                                        .small()
                                        .primary()
                                        .label(t!("open_recording").into_owned())
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.saved_recordings = None;
                                            this.load_audio(open.clone(), window, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("read-recording", index))
                                        .small()
                                        .label(t!("text").into_owned())
                                        .disabled(!has_text)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_saved_text(&read, window, cx);
                                        })),
                                )
                                .child(
                                    Button::new(("delete-recording", index))
                                        .small()
                                        .label(t!("delete").into_owned())
                                        .disabled(self.recognition_path.as_ref() == Some(path))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.confirm_delete(delete.clone(), window, cx);
                                        })),
                                ),
                        )
                }))
                .into_any_element(),
            Err(error) => div().p_3().child(error.render()).into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .child(div().text_xl().child(t!("saved_recordings").into_owned()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .rounded_md()
                    .border_1()
                    .border_color(border)
                    .child(header)
                    .child(rows),
            )
    }

    fn open_saved_text(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match std::fs::read_to_string(transcript_path(path)) {
            Ok(text) => {
                let state = cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .default_value(text)
                        .placeholder(t!("no_speech").into_owned())
                });
                self.saved_text = Some((file_name(path), state));
                cx.notify();
            }
            Err(error) => {
                drop(window.prompt(
                    PromptLevel::Warning,
                    &t!("open_text_failed"),
                    Some(&error.to_string()),
                    &[t!("ok").as_ref()],
                    cx,
                ));
                self.refresh_recordings(cx);
            }
        }
    }

    pub(super) fn saved_text_view(
        &self,
        name: &str,
        text: &Entity<TextareaState>,
        cx: &Context<Self>,
    ) -> Div {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().min_w_0().truncate().text_xl().child(name.to_owned()))
                    .child(
                        Button::new("close-saved-text")
                            .label(t!("back_to_recordings").into_owned())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.saved_text = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Textarea::new(text).h(relative(1.)).readonly(true)),
            )
    }

    fn confirm_delete(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let detail = t!("delete_detail", detail = file_name(&path)).into_owned();
        let answer = window.prompt(
            PromptLevel::Warning,
            &t!("delete_question"),
            Some(&detail),
            &[t!("cancel").as_ref(), t!("delete").as_ref()],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() != Some(1) {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| match std::fs::remove_file(&path) {
                Ok(()) => {
                    if this.audio_path.as_ref() == Some(&path) {
                        this.clear_audio(window, cx);
                        this.status = Message::Text("recording_deleted");
                    }
                    if this
                        .last_transcript
                        .as_ref()
                        .is_some_and(|(source, _)| source == &path)
                    {
                        this.last_transcript = None;
                    }
                    this.refresh_recordings(cx);
                }
                Err(error) => {
                    let detail = format!("{}: {error}", file_name(&path));
                    drop(window.prompt(
                        PromptLevel::Warning,
                        &t!("delete_failed"),
                        Some(&detail),
                        &[t!("ok").as_ref()],
                        cx,
                    ));
                    this.refresh_recordings(cx);
                }
            });
        })
        .detach();
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
