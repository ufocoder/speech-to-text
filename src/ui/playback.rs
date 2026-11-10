use super::Recorder;
use crate::{audio::Player, i18n::Message};
use gpui_kit::component::{
    ActiveTheme,
    button::Button,
    slider::{Slider, SliderValue},
};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;
use std::time::Duration;

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
            let SliderValue::Single(progress) = self.playback_slider.read(cx).value() else {
                return;
            };
            let position = if progress >= 1. {
                0.
            } else {
                f64::from(progress) * duration
            };
            match Player::start(audio, position) {
                Ok(player) => {
                    self.player = Some(player);
                    let executor = cx.background_executor().clone();
                    self.playback_task = Some(cx.spawn_in(window, async move |this, cx| {
                        loop {
                            executor.timer(Duration::from_millis(50)).await;
                            let active = this.update_in(cx, |this, window, cx| {
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
                                    let progress = if position >= duration {
                                        0.
                                    } else {
                                        (position / duration) as f32
                                    };
                                    this.playback_slider.update(cx, |slider, cx| {
                                        slider.set_value(progress, window, cx)
                                    });
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

    pub(super) fn playback_panel(&self, cx: &mut Context<Self>) -> Option<Div> {
        let audio = self.audio.as_ref()?;
        let duration = audio.duration();
        let SliderValue::Single(seek) = self.playback_slider.read(cx).value() else {
            return None;
        };
        let position = self
            .player
            .as_ref()
            .map_or(f64::from(seek) * duration, Player::position);
        let progress = (position / duration).clamp(0., 1.);
        let theme = cx.theme();
        let (played, pending, background) = (theme.primary, theme.muted_foreground, theme.muted);
        // Normalize only the display, with a floor to avoid magnifying near-silence.
        let peak_scale = audio.peaks.iter().copied().fold(0.02_f32, f32::max);
        let waveform = div()
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
            }));
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
                    .child(
                        div().flex_1().min_w(px(32.)).child(
                            Slider::new(&self.playback_slider)
                                .accessibility_label(t!("playback_position").into_owned()),
                        ),
                    )
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
