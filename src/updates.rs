use gpui_kit::{
    component::{Root, button::Button, spinner::Spinner},
    prelude::*,
    *,
};
use rust_i18n::t;
use serde::Deserialize;
use std::{io::Read, time::Duration};

const RELEASES: &str = "https://github.com/ufocoder/speech-to-text/releases";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

struct Updates {
    result: Option<anyhow::Result<Option<Release>>>,
}

pub fn open(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(430.), px(230.)), cx);
    if let Err(error) = cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(t!("check_updates").into_owned().into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx: &mut Context<Updates>| {
                let request = cx.background_spawn(async { check() });
                cx.spawn(async move |view, cx| {
                    let result = request.await;
                    let _ = view.update(cx, |view, cx| {
                        view.result = Some(result);
                        cx.notify();
                    });
                })
                .detach();
                Updates { result: None }
            });
            cx.new(|cx| Root::new(view, window, cx))
        },
    ) {
        eprintln!("{error}");
    }
}

fn check() -> anyhow::Result<Option<Release>> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .new_agent();
    let mut response = agent
        .get("https://api.github.com/repos/ufocoder/speech-to-text/releases/latest")
        .header("User-Agent", "Speech-to-Text")
        .header("Accept", "application/vnd.github+json")
        .call()?;
    let release: Release =
        serde_json::from_reader(response.body_mut().as_reader().take(1_000_000))?;
    let available = semver::Version::parse(release.tag_name.trim_start_matches('v'))?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
    Ok((available > current).then_some(release))
}

impl Release {
    fn download_url(&self) -> Option<&str> {
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "macos-arm64",
            ("macos", "x86_64") => "macos-x86_64",
            ("windows", "x86_64") => "windows-x64",
            _ => return None,
        };
        let name = format!("speech-to-text-{}-{platform}.zip", self.tag_name);
        let url = format!("{RELEASES}/download/{}/{name}", self.tag_name);
        self.assets
            .iter()
            .find(|asset| asset.name == name && asset.browser_download_url == url)
            .map(|asset| asset.browser_download_url.as_str())
    }
}

impl Render for Updates {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let status = match &self.result {
            None => t!("checking_updates").into_owned(),
            Some(Ok(None)) => t!("updates_current").into_owned(),
            Some(Ok(Some(release))) => {
                t!("update_available", version = release.tag_name.as_str()).into_owned()
            }
            Some(Err(error)) => t!("update_check_failed", detail = error.to_string()).into_owned(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .justify_center()
            .items_start()
            .gap_3()
            .p_5()
            .child(t!("menu_about_version", version = env!("CARGO_PKG_VERSION")).into_owned())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(self.result.is_none(), |row| row.child(Spinner::new()))
                    .child(status),
            )
            .when_some(
                self.result
                    .as_ref()
                    .and_then(|result| result.as_ref().ok())
                    .and_then(Option::as_ref),
                |view, release| {
                    let download = release.download_url().map(str::to_owned);
                    let label = if download.is_some() {
                        t!("download_update")
                    } else {
                        t!("view_releases")
                    };
                    let url =
                        download.unwrap_or_else(|| format!("{RELEASES}/tag/{}", release.tag_name));
                    view.child(
                        Button::new("download-update")
                            .label(label.into_owned())
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    )
                },
            )
    }
}
