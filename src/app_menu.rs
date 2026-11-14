use gpui_kit::*;
use rust_i18n::t;

use crate::ui::Recorder;

actions!(application_menu, [About, RecordSpeech, Settings, Quit]);

pub(crate) fn set_menu(cx: &App) {
    cx.set_menus([Menu::new("Speech to Text").items([
        MenuItem::action(t!("menu_about").into_owned(), About),
        MenuItem::separator(),
        MenuItem::action(t!("menu_record_speech").into_owned(), RecordSpeech),
        MenuItem::action(t!("settings").into_owned(), Settings),
        MenuItem::separator(),
        MenuItem::action(t!("menu_quit").into_owned(), Quit),
    ])]);
}

pub(crate) fn install(recorder: WeakEntity<Recorder>, window: AnyWindowHandle, cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-,", Settings, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
    cx.on_action(move |_: &About, cx| {
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                window.activate_window();
                drop(window.prompt(
                    PromptLevel::Info,
                    &t!("window_title"),
                    Some(&t!(
                        "menu_about_version",
                        version = env!("CARGO_PKG_VERSION")
                    )),
                    &[t!("ok").as_ref()],
                    cx,
                ));
            });
        });
    });
    let recording_view = recorder.clone();
    cx.on_action(move |_: &RecordSpeech, cx| {
        let recorder = recording_view.clone();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                window.activate_window();
                let _ = recorder.update(cx, |this, cx| this.record_speech(window, cx));
            });
        });
    });
    cx.on_action(move |_: &Settings, cx| {
        let recorder = recorder.clone();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                window.activate_window();
                let _ = recorder.update(cx, |this, cx| this.open_settings(cx));
            });
        });
    });
    cx.on_action(|_: &Quit, cx| cx.quit());
    set_menu(cx);
}
