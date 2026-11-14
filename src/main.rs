#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use rust_i18n::t;
#[cfg(target_os = "macos")]
mod app_menu;
mod audio;
mod diarization;
mod i18n;
mod models;
mod recognition;
mod storage;
mod ui;
mod updates;

use gpui_kit::{component::Root, *};
use ui::Recorder;

rust_i18n::i18n!("locales", fallback = "en");

fn main() {
    let language = i18n::Language::load();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            #[cfg(target_os = "macos")]
            set_dock_icon();
            language.activate();
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(t!("window_title").into_owned().into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(490.), px(460.))),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(900.), px(620.)),
                        cx,
                    ))),
                    ..Default::default()
                },
                move |window, cx| {
                    let view = cx.new(|cx| Recorder::new(language, window, cx));
                    #[cfg(target_os = "macos")]
                    app_menu::install(view.downgrade(), window.window_handle(), cx);
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .unwrap_or_else(|error| panic!("{}: {error}", t!("open_window_failed")));
            cx.activate(true);
        });
}

#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(include_bytes!("../macos/Resources/Microphone.icns"));
    if let Some(icon) = NSImage::initWithData(main_thread.alloc(), &data) {
        let app = NSApplication::sharedApplication(main_thread);
        // AppKit is accessed on the main thread and receives a valid NSImage.
        unsafe { app.setApplicationIconImage(Some(&icon)) };
    }
}
