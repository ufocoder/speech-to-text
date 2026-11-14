use super::Recorder;
use crate::{
    i18n::{Language, Message},
    models::SpeechModel,
    storage::model_cache,
};
use cpal::traits::{DeviceTrait, HostTrait};
use gpui_kit::component::{
    Disableable, Sizable,
    button::Button,
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::*;
use rust_i18n::t;

impl Recorder {
    pub(super) fn audio_model_menu(
        &self,
        selected: SpeechModel,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().downgrade();
        Button::new("audio-model")
            .label(t!("model", detail = selected.label()).into_owned())
            .disabled(self.audio_busy())
            .dropdown_menu(move |menu, _, _| {
                let cache = model_cache();
                SpeechModel::ALL
                    .into_iter()
                    .filter(|model| model.cached_directory(&cache).is_some())
                    .fold(menu, |menu, model| {
                        let view = view.clone();
                        menu.item(
                            PopupMenuItem::new(model.label())
                                .checked(model == selected)
                                .on_click(move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        if this.audio_path.is_some() && !this.audio_busy() {
                                            this.audio_model = Some(model);
                                            cx.notify();
                                        }
                                    });
                                }),
                        )
                    })
            })
    }

    pub(super) fn microphone_menu(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let name = self.microphone.as_ref().map_or_else(
            || t!("system_default").into_owned(),
            |(name, _)| name.to_string(),
        );
        let using_default = self.microphone.is_none();
        Button::new("microphone")
            .large()
            .label(name)
            .disabled(self.recording.is_some())
            .dropdown_menu(move |menu, _, _| {
                let default_view = view.clone();
                let mut menu = menu.item(
                    PopupMenuItem::new(t!("system_default").into_owned())
                        .checked(using_default)
                        .on_click(move |_, _, cx| {
                            let _ = default_view.update(cx, |this, cx| {
                                if this.recording.is_none() {
                                    this.microphone = None;
                                    cx.notify();
                                }
                            });
                        }),
                );
                match cpal::default_host().input_devices() {
                    Ok(devices) => {
                        for (index, device) in devices.enumerate() {
                            let name: SharedString = device
                                .name()
                                .unwrap_or_else(|_| {
                                    t!("unnamed_microphone", detail = index + 1).into_owned()
                                })
                                .into();
                            let view = view.clone();
                            menu = menu.item(PopupMenuItem::new(name.clone()).on_click(
                                move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        if this.recording.is_none() {
                                            this.microphone = Some((name.clone(), device.clone()));
                                            cx.notify();
                                        }
                                    });
                                },
                            ));
                        }
                        menu
                    }
                    Err(_) => menu.item(PopupMenuItem::label(
                        t!("microphones_unavailable").into_owned(),
                    )),
                }
            })
    }

    pub(super) fn language_menu(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let selected = self.language;
        Button::new("language")
            .label(self.language.label())
            .disabled(self.audio_busy() || self.choosing_file || self.recording.is_some())
            .dropdown_menu(move |menu, _, _| {
                [Language::Russian, Language::English]
                    .into_iter()
                    .fold(menu, |menu, language| {
                        let view = view.clone();
                        menu.item(
                            PopupMenuItem::new(language.label())
                                .checked(language == selected)
                                .on_click(move |_, window, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.language = language;
                                        language.activate();
                                        #[cfg(target_os = "macos")]
                                        crate::app_menu::set_menu(cx);
                                        window.set_window_title(&t!("window_title"));
                                        this.transcript.update(cx, |input, cx| {
                                            input.set_placeholder(
                                                t!("transcript_placeholder").into_owned(),
                                                window,
                                                cx,
                                            );
                                        });
                                        if let Err(error) = language.save() {
                                            this.settings_error = Some(Message::Detail(
                                                "language_save_failed",
                                                error.to_string(),
                                            ));
                                        }
                                        cx.notify();
                                    });
                                }),
                        )
                    })
            })
    }
}
