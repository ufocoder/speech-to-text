use gpui_kit::{
    App, Background, Hsla, component::Theme, linear_color_stop, linear_gradient, px, rgb,
};

pub(crate) fn install(cx: &mut App) {
    Theme::update(cx, |theme| {
        if theme.is_dark() {
            return;
        }

        let ink = rgb(0x242019).into();
        let surface = rgb(0xfffefa).into();
        let muted = rgb(0xf1ebe0).into();
        let accent = rgb(0xe8d0a6).into();
        let hover = rgb(0xf4ede1).into();
        let border = rgb(0xdbd4c8).into();

        theme.radius = px(12.);
        theme.radius_lg = px(16.);
        let colors = &mut theme.colors;
        colors.foreground = ink;
        colors.background = surface;
        colors.border = border;
        colors.input = border;
        colors.muted = muted;
        colors.muted_foreground = rgb(0x635c52).into();
        colors.accent = hover;
        colors.accent_foreground = ink;
        colors.popover = surface;
        colors.popover_foreground = ink;
        colors.group_box_foreground = ink;
        colors.button = surface;
        colors.button_foreground = ink;
        colors.button_hover = hover;
        colors.button_active = muted;
        colors.primary = accent;
        colors.primary_foreground = ink;
        colors.primary_hover = rgb(0xe3c597).into();
        colors.primary_active = rgb(0xdcb983).into();
        colors.button_primary = colors.primary;
        colors.button_primary_foreground = colors.primary_foreground;
        colors.button_primary_hover = colors.primary_hover;
        colors.button_primary_active = colors.primary_active;
        colors.ring = rgb(0x8a6027).into();
    });
}

pub(super) fn window_background(theme: &Theme) -> Hsla {
    if theme.is_dark() {
        rgb(0x27221e).into()
    } else {
        rgb(0xf8f6f0).into()
    }
}

pub(super) fn card_background(theme: &Theme) -> Background {
    if theme.is_dark() {
        return theme.background.into();
    }

    linear_gradient(
        135.,
        linear_color_stop(rgb(0xf4ede1), 0.),
        linear_color_stop(theme.background, 1.),
    )
}
