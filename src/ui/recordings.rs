use super::Recorder;
use crate::{
    i18n::Message,
    storage::{SavedRecording, delete_transcripts, saved_recordings, transcript_files},
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
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
            .child(div().w(px(180.)).child(t!("actions").into_owned()));
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
                    let models = transcript_files(path);
                    let badges = div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .mt_1()
                        .when(models.is_empty(), |row| {
                            row.child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t!("no_saved_transcript").into_owned()),
                            )
                        })
                        .children(models.into_keys().enumerate().map(|(model_index, model)| {
                            let source = path.clone();
                            let label = model
                                .clone()
                                .unwrap_or_else(|| t!("unknown_transcript_model").into_owned());
                            Button::new(SharedString::from(format!(
                                "archive-transcript-{index}-{model_index}"
                            )))
                            .xsmall()
                            .label(label)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.saved_recordings = None;
                                    this.load_audio_with_transcript(
                                        source.clone(),
                                        Some(model.clone()),
                                        window,
                                        cx,
                                    );
                                },
                            ))
                        }));
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
                                .child(badges),
                        )
                        .child(
                            div()
                                .w(px(180.))
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
                                    Button::new(("delete-recording", index))
                                        .small()
                                        .label(t!("delete").into_owned())
                                        .disabled(self.recognition_model_for(path).is_some())
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
            let _ = this.update_in(cx, |this, window, cx| {
                if this.recognition_model_for(&path).is_some() {
                    return;
                }
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        if this.audio_path.as_ref() == Some(&path) {
                            this.clear_audio(window, cx);
                            this.status = Message::Empty;
                        }
                        this.session_transcripts.remove(&path);
                        if let Err(error) = delete_transcripts(&path) {
                            drop(window.prompt(
                                PromptLevel::Warning,
                                &t!("delete_failed"),
                                Some(&error.to_string()),
                                &[t!("ok").as_ref()],
                                cx,
                            ));
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
