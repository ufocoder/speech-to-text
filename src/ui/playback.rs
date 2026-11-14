use super::Recorder;
use crate::{audio::Player, i18n::Message};
use gpui_kit::component::{ActiveTheme, button::Button};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;
use std::{cell::Cell, rc::Rc, time::Duration};

impl Recorder {
    fn toggle_playback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(audio) = self.audio.clone() else {
            return;
        };
        self.playback_error = None;
        if let Some(player) = &mut self.player {
            if !player.playing && player.position() >= audio.duration() {
                player.seek(0.);
            }
            if let Err(error) = player.toggle() {
                self.player = None;
                self.playback_error = Some(Message::Detail("playback_error", format!("{error:#}")));
            }
        } else {
            let duration = audio.duration();
            let position = if self.playback_position >= duration {
                0.
            } else {
                self.playback_position
            };
            match Player::start(audio, position) {
                Ok(player) => {
                    self.player = Some(player);
                    let executor = cx.background_executor().clone();
                    self.playback_task = Some(cx.spawn_in(window, async move |this, cx| {
                        loop {
                            executor.timer(Duration::from_millis(50)).await;
                            let active = this.update(cx, |this, cx| {
                                let Some(player) = &this.player else {
                                    return false;
                                };
                                if player.failed() {
                                    this.player = None;
                                    this.playback_error =
                                        Some(Message::Text("output_disconnected"));
                                } else if player.playing {
                                    let position = player.position();
                                    if position >= duration {
                                        this.player = None;
                                    }
                                    this.playback_position = position.min(duration);
                                }
                                cx.notify();
                                this.player.is_some()
                            });
                            if !matches!(active, Ok(true)) {
                                break;
                            }
                        }
                    }));
                }
                Err(error) => {
                    self.playback_error =
                        Some(Message::Detail("playback_error", format!("{error:#}")))
                }
            }
        }
        cx.notify();
    }

    pub(super) fn seek_playback(&mut self, position: f64, cx: &mut Context<Self>) {
        if let Some(audio) = &self.audio {
            self.playback_position = position.clamp(0., audio.duration());
            if let Some(player) = &self.player {
                player.seek(self.playback_position);
            }
            cx.notify();
        }
    }

    pub(super) fn playback_panel(&self, cx: &mut Context<Self>) -> Option<Div> {
        let audio = self.audio.as_ref()?;
        let duration = audio.duration();
        let position = self
            .player
            .as_ref()
            .map_or(self.playback_position, Player::position);
        let progress = (position / duration).clamp(0., 1.);
        let theme = cx.theme();
        let (played, pending, background) = (theme.primary, theme.muted_foreground, theme.muted);
        // Normalize only the display, with a floor to avoid magnifying near-silence.
        let peak_scale = audio.peaks.iter().copied().fold(0.02_f32, f32::max);
        let bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let waveform = div()
            .id("playback-waveform")
            .relative()
            .cursor_pointer()
            .focusable()
            .tab_index(0)
            .aria_label(t!("playback_position").into_owned())
            .aria_value(format!("{} / {}", clock(position), clock(duration)))
            .border_1()
            .border_color(background)
            .focus_visible(|style| style.border_color(played))
            .on_click(cx.listener({
                let bounds = bounds.clone();
                move |this, event: &ClickEvent, _, cx| {
                    let bounds = bounds.get();
                    if !matches!(event, ClickEvent::Keyboard(_)) && bounds.size.width > px(0.) {
                        let fraction = (event.position().x - bounds.origin.x) / bounds.size.width;
                        this.seek_playback(f64::from(fraction) * duration, cx);
                    }
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let position = this
                    .player
                    .as_ref()
                    .map_or(this.playback_position, Player::position);
                let target = match event.keystroke.key.as_str() {
                    "left" => position - 5.,
                    "right" => position + 5.,
                    "home" => 0.,
                    "end" => this.audio.as_ref().map_or(0., |audio| audio.duration()),
                    _ => return,
                };
                this.seek_playback(target, cx);
                cx.stop_propagation();
            }))
            .h(px(124.))
            .w_full()
            .flex()
            .items_center()
            .gap_px()
            .px_2()
            .rounded_md()
            .bg(background)
            .children(audio.peaks.iter().enumerate().map(|(index, peak)| {
                let color = if (index as f64 + 0.5) / audio.peaks.len() as f64 <= progress {
                    played
                } else {
                    pending
                };
                div()
                    .flex_1()
                    .rounded_sm()
                    .bg(color)
                    .h(px(((peak / peak_scale).sqrt() * 108.).max(2.)))
            }))
            .child(
                canvas(
                    move |area, _, _| bounds.set(area),
                    move |area, _, window, _| {
                        let x = area.origin.x + (area.size.width - px(1.)) * progress as f32;
                        window.paint_quad(fill(
                            Bounds::new(point(x, area.origin.y), size(px(1.), area.size.height)),
                            played,
                        ));
                    },
                )
                .absolute()
                .inset_0(),
            );
        let label = if self.player.as_ref().is_some_and(|player| player.playing) {
            t!("pause").into_owned()
        } else {
            t!("play").into_owned()
        };
        Some(
            div().flex().flex_col().gap_2().child(waveform).child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(Button::new("playback").label(label).on_click(
                        cx.listener(|this, _, window, cx| this.toggle_playback(window, cx)),
                    ))
                    .child(div().text_sm().child(format!(
                        "{} / {}",
                        clock(position),
                        clock(duration)
                    ))),
            ),
        )
    }
}

fn clock(seconds: f64) -> String {
    let seconds = seconds as u64;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
