use super::Recorder;
use crate::{
    models::{SPEAKER_MODELS, SpeakerModel, SpeechModel},
    storage::{model_cache, save_added_models},
};
use gpui_kit::component::{
    Disableable, Root, StyledExt,
    button::Button,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;
use serde::Deserialize;
use std::{io::Read, time::Duration};

#[derive(Deserialize)]
struct HubModel {
    id: String,
    #[serde(default)]
    downloads: u64,
}

struct ModelCatalog {
    speakers: bool,
    recorder: WeakEntity<Recorder>,
    search: Entity<InputState>,
    results: Vec<HubModel>,
    query: String,
    loading: bool,
    error: Option<String>,
}

impl Recorder {
    pub(super) fn refresh_model_update_dates(&mut self, cx: &mut Context<Self>) {
        self.model_update_tasks = SpeechModel::ALL
            .into_iter()
            .map(|model| (model.id(), model.repository()))
            .chain(
                crate::models::SPEAKER_MODELS
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| matches!(index, 0 | 3))
                    .map(|(_, &(_, repo, _, _))| (repo, repo)),
            )
            .map(|(key, repo)| {
                let request = cx.background_spawn(async move { model_update_date(repo) });
                cx.spawn(async move |this, cx| {
                    if let Ok(Some(date)) = request.await {
                        let _ = this.update(cx, |this, cx| {
                            this.model_update_dates.insert(key, date);
                            cx.notify();
                        });
                    }
                })
            })
            .collect();
    }

    pub(super) fn open_model_catalog(&mut self, speakers: bool, cx: &mut Context<Self>) {
        let recorder = cx.entity().downgrade();
        // The first draw reads Recorder, so open after the button listener releases it.
        cx.defer(move |cx| {
            let bounds =
                WindowBounds::Windowed(Bounds::centered(None, size(px(700.), px(540.)), cx));
            let catalog_recorder = recorder.clone();
            let result = cx.open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(
                            t!(if speakers {
                                "speaker_catalog"
                            } else {
                                "search_models"
                            })
                            .into_owned()
                            .into(),
                        ),
                        ..Default::default()
                    }),
                    window_bounds: Some(bounds),
                    ..Default::default()
                },
                move |window, cx| {
                    let catalog = cx.new(|cx| {
                        let search = cx.new(|cx| InputState::new(window, cx));
                        cx.subscribe(&search, |this: &mut ModelCatalog, _, event, cx| {
                            if matches!(event, InputEvent::PressEnter { .. }) {
                                this.search(cx);
                            }
                        })
                        .detach();
                        let mut catalog = ModelCatalog {
                            speakers,
                            recorder: catalog_recorder,
                            search,
                            results: Vec::new(),
                            query: String::new(),
                            loading: false,
                            error: None,
                        };
                        catalog.search(cx);
                        catalog
                    });
                    cx.new(|cx| Root::new(catalog, window, cx))
                },
            );
            if let Err(error) = result {
                let _ = recorder.update(cx, |this, cx| {
                    this.settings_error = Some(crate::i18n::Message::Error(error.to_string()));
                    cx.notify();
                });
            }
        });
    }
}

impl ModelCatalog {
    fn search(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.query = self
            .search
            .read(cx)
            .value()
            .trim()
            .chars()
            .take(150)
            .collect();
        let query = self.query.clone();
        if self.speakers {
            cx.notify();
            return;
        }
        self.loading = true;
        self.results.clear();
        self.error = None;
        let task = cx.background_spawn(async move { search_hub(&query) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(results) => this.results = results,
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn add(&mut self, model: SpeechModel, cx: &mut Context<Self>) {
        let result = self.recorder.update(cx, |recorder, cx| {
            if recorder.catalog_models.contains(&model) {
                return Ok(());
            }
            let mut models = recorder.catalog_models.clone();
            models.push(model);
            save_added_models(&models)?;
            recorder.catalog_models = models;
            cx.notify();
            Ok::<_, std::io::Error>(())
        });
        self.error = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error.to_string()),
            Err(error) => Some(error.to_string()),
        };
        cx.notify();
    }
}

impl Render for ModelCatalog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.query.to_lowercase();
        let cache = model_cache();
        let added = self
            .recorder
            .read_with(cx, |recorder, _| recorder.catalog_models.clone())
            .unwrap_or_default();
        let speaker_rows = SpeakerModel::ALL
            .into_iter()
            .filter_map(|model| {
                let index = model.segmentation_index();
                let (label, repo, file, _) = SPEAKER_MODELS[index];
                if !self.speakers
                    || !(label.to_lowercase().contains(&query) || repo.contains(&query))
                {
                    return None;
                }
                let installed = cache.join("speaker-diarization").join(file).is_file();
                let busy = self
                    .recorder
                    .read_with(cx, |recorder, _| {
                        recorder.speaker_download.is_some() || recorder.speaker_task.is_some()
                    })
                    .unwrap_or(true);
                let recorder = self.recorder.clone();
                Some(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .min_w_0()
                                .child(label)
                                .child(div().text_sm().child(repo)),
                        )
                        .child(
                            Button::new(("catalog-speaker-download", index))
                                .disabled(installed || busy)
                                .label(
                                    t!(if installed {
                                        "speaker_installed"
                                    } else {
                                        "download_model"
                                    })
                                    .into_owned(),
                                )
                                .on_click(move |_, window, cx| {
                                    if recorder
                                        .update(cx, |this, cx| this.download_speaker(index, cx))
                                        .is_ok()
                                    {
                                        window.remove_window();
                                    }
                                }),
                        ),
                )
            })
            .collect::<Vec<_>>();
        let mut models = Vec::new();
        for item in &self.results {
            if let Some(model) = SpeechModel::ALL.into_iter().find(|model| {
                model.repository().eq_ignore_ascii_case(&item.id)
                    || match model {
                        SpeechModel::Parakeet => {
                            item.id.eq_ignore_ascii_case("nvidia/parakeet-tdt-0.6b-v3")
                        }
                        SpeechModel::GigaAm => item.id.eq_ignore_ascii_case("ai-sage/GigaAM-v3"),
                        SpeechModel::WhisperTurbo => item
                            .id
                            .eq_ignore_ascii_case("openai/whisper-large-v3-turbo"),
                        SpeechModel::Tone => false,
                    }
            }) && !models.iter().any(|(existing, _)| *existing == model)
            {
                models.push((model, Some(item.downloads)));
            }
        }
        for model in SpeechModel::ALL.into_iter().filter(|_| !self.speakers) {
            if (model.repository().to_lowercase().contains(&query)
                || model.label().to_lowercase().contains(&query))
                && !models.iter().any(|(existing, _)| *existing == model)
            {
                models.push((model, None));
            }
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(Input::new(&self.search)))
                    .child(
                        Button::new("hub-search")
                            .label(t!("hub_search").into_owned())
                            .disabled(self.loading)
                            .on_click(cx.listener(|this, _, _, cx| this.search(cx))),
                    ),
            )
            .when(self.loading, |view| {
                view.child(t!("hub_loading").into_owned())
            })
            .children(
                self.error
                    .iter()
                    .map(|error| div().text_sm().child(error.clone())),
            )
            .child(
                div()
                    .id("hub-results")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .font_semibold()
                                    .child(t!("hub_compatible").into_owned()),
                            )
                            .when(
                                !self.loading && models.is_empty() && speaker_rows.is_empty(),
                                |view| view.child(t!("no_matching_models").into_owned()),
                            )
                            .children(speaker_rows)
                            .children(models.into_iter().map(|(model, downloads)| {
                                let in_table = model != SpeechModel::WhisperTurbo
                                    || added.contains(&model)
                                    || model.cached_directory(&cache).is_some();
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .child(model.label())
                                            .child(div().text_sm().child(model.repository()))
                                            .child(
                                                div().text_sm().child(
                                                    t!(
                                                        "hub_model_details",
                                                        size = model.download_size_mb(),
                                                        parameters = model.parameters_millions()
                                                    )
                                                    .into_owned(),
                                                ),
                                            )
                                            .when_some(downloads, |details, count| {
                                                details.child(div().text_sm().child(
                                                    t!("hub_downloads", count = count).into_owned(),
                                                ))
                                            }),
                                    )
                                    .child(
                                        Button::new(model.id())
                                            .disabled(in_table)
                                            .label(
                                                if in_table {
                                                    t!("hub_added")
                                                } else {
                                                    t!("hub_add")
                                                }
                                                .into_owned(),
                                            )
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.add(model, cx)
                                            })),
                                    )
                            })),
                    ),
            )
    }
}

fn search_hub(query: &str) -> anyhow::Result<Vec<HubModel>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let mut response = agent
        .get("https://huggingface.co/api/models")
        .query("pipeline_tag", "automatic-speech-recognition")
        .query("sort", "downloads")
        .query("direction", "-1")
        .query("limit", "25")
        .query("search", query)
        .call()?;
    Ok(serde_json::from_reader(
        response.body_mut().as_reader().take(2_000_000),
    )?)
}

fn model_update_date(repository: &str) -> anyhow::Result<Option<chrono::NaiveDate>> {
    #[derive(Deserialize)]
    struct ModelInfo {
        #[serde(rename = "lastModified")]
        last_modified: Option<String>,
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent
        .get(format!("https://huggingface.co/api/models/{repository}"))
        .query("expand", "lastModified")
        .call()?;
    let info: ModelInfo = serde_json::from_reader(response.body_mut().as_reader().take(64_000))?;
    Ok(info
        .last_modified
        .as_deref()
        .map(chrono::DateTime::parse_from_rfc3339)
        .transpose()?
        .map(|date| date.date_naive()))
}
