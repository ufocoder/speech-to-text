use super::Recorder;
use crate::recognition::TranscriptTurn;
use gpui_kit::{
    component::{ActiveTheme, StyledExt},
    prelude::*,
    *,
};
use rust_i18n::t;

impl Recorder {
    pub(super) fn transcript_rows(
        &self,
        turns: &[TranscriptTurn],
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let position = self
            .player
            .as_ref()
            .map_or(self.playback_position, crate::audio::Player::position);
        let playing = self.player.as_ref().is_some_and(|player| player.playing);
        let font_size = f32::from(self.transcript_view.font_size);
        let scale = font_size / 16.;
        let label_size = font_size * 0.8;
        let line_height = font_size * 1.5;
        let label_height = line_height + 8. * scale;
        let palette = if theme.is_dark() {
            [
                (0x203b48, 0xa5d6ee),
                (0x352c48, 0xd6bbf5),
                (0x234035, 0xace1c6),
                (0x463124, 0xf1c49d),
            ]
        } else {
            [
                (0xe9f3fa, 0x245878),
                (0xf2ecfa, 0x65428b),
                (0xeaf5ef, 0x306b4d),
                (0xfff1e6, 0x87532b),
            ]
        };
        div()
            .id("transcript-rows")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(
                turns
                    .iter()
                    .filter(|turn| !turn.text.is_empty())
                    .map(|turn| {
                        let (background, foreground) =
                            palette[turn.speaker.unwrap_or_default() as usize % palette.len()];
                        let seconds = turn.start.max(0.) as u64;
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .py_1()
                            .rounded(px(9. * scale))
                            .when(
                                self.transcript_view.highlight
                                    && playing
                                    && position >= turn.start
                                    && position < turn.end,
                                |row| row.bg(theme.primary.opacity(0.35)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .gap_1()
                                    .when(self.transcript_view.timestamps, |labels| {
                                        labels.child(
                                            div()
                                                .px(px(8. * scale))
                                                .h(px(label_height))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded(px(9. * scale))
                                                .text_center()
                                                .whitespace_nowrap()
                                                .w(px(76. * scale))
                                                .flex_shrink_0()
                                                .bg(theme.muted)
                                                .text_color(theme.muted_foreground)
                                                .text_size(px(label_size))
                                                .child(format!(
                                                    "{:02}:{:02}:{:02}",
                                                    seconds / 3600,
                                                    seconds / 60 % 60,
                                                    seconds % 60
                                                )),
                                        )
                                    })
                                    .when_some(
                                        turn.speaker.filter(|_| self.transcript_view.speakers),
                                        |labels, speaker| {
                                            labels.child(
                                                div()
                                                    .px(px(8. * scale))
                                                    .h(px(label_height))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded(px(9. * scale))
                                                    .text_center()
                                                    .whitespace_nowrap()
                                                    .w(px(96. * scale))
                                                    .flex_shrink_0()
                                                    .bg(rgb(background))
                                                    .text_color(rgb(foreground))
                                                    .text_size(px(label_size))
                                                    .font_medium()
                                                    .child(
                                                        t!(
                                                            "transcript_speaker",
                                                            number = i64::from(speaker) + 1
                                                        )
                                                        .into_owned(),
                                                    ),
                                            )
                                        },
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(font_size))
                                    .line_height(px(line_height))
                                    .py(px(4. * scale))
                                    .child(turn.text.clone()),
                            )
                    }),
            )
    }
}
