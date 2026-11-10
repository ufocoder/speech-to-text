use super::Recorder;
use crate::{
    i18n::Message,
    storage::{model_cache, recordings_dir, save_recordings_dir},
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Disableable, StyledExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;

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
                this.saved_text = None;
                this.settings_open = true;
                cx.notify();
            }))
    }

    pub(super) fn settings_content(&self, cx: &Context<Self>) -> impl IntoElement {
        let view = &cx.entity().downgrade();
        let choose_view = view.clone();
        let cached = self.model.cached_directory(&model_cache()).is_some();
        let model_status = if self.downloading_model {
            "model_downloading"
        } else if cached {
            "model_downloaded"
        } else {
            "model_not_downloaded"
        };
        div()
            .id("settings-content")
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child(t!("settings").into_owned()))
            .child(div().child(t!("settings_microphone").into_owned()))
            .child(div().child(self.microphone_menu(view.clone())))
            .child(div().child(t!("recordings_folder").into_owned()))
            .child(
                div()
                    .text_sm()
                    .child(recordings_dir().display().to_string()),
            )
            .child(
                Button::new("choose-recordings-directory")
                    .label(t!("choose_folder").into_owned())
                    .disabled(self.choosing_file)
                    .on_click(move |_, window, cx| {
                        let _ = choose_view
                            .update(cx, |this, cx| this.choose_recordings_dir(window, cx));
                    }),
            )
            .child(div().child(t!("language").into_owned()))
            .child(div().child(self.language_menu(view.clone())))
            .child(
                div()
                    .font_bold()
                    .child(t!("recognition_model_label").into_owned()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(self.model_menu(view.clone()))
                    .when(!cached || self.downloading_model, |row| {
                        row.child(
                            Button::new("download-model")
                                .primary()
                                .label(t!("download_model").into_owned())
                                .disabled(self.downloading_model)
                                .on_click(cx.listener(|this, _, _, cx| this.download_model(cx))),
                        )
                    }),
            )
            .child(div().text_sm().child(t!(model_status).into_owned()))
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
        self.settings_error = None;
        let model = self.model;
        let task = cx.background_spawn(async move { model.prepare(&model_cache()) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.downloading_model = false;
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
