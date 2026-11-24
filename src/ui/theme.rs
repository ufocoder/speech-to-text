use gpui_kit::{App, Background, Hsla, component::Theme, px, rgb};

pub(crate) fn install(cx: &mut App) {
    Theme::update(cx, |theme| {
        if theme.is_dark() {
            return;
        }

        let ink = rgb(0x242424).into();
        let surface = rgb(0xffffff).into();
        let muted = rgb(0xf0f0ee).into();
        let accent = rgb(0xf4dcae).into();
        let hover = rgb(0xf7f3ea).into();
        let border = rgb(0xc8c8c3).into();

        theme.radius = px(12.);
        theme.radius_lg = px(16.);
        let colors = &mut theme.colors;
        colors.foreground = ink;
        colors.background = surface;
        colors.border = border;
        colors.input = border;
        colors.muted = muted;
        colors.muted_foreground = rgb(0x60605b).into();
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
        colors.primary_hover = rgb(0xefcf93).into();
        colors.primary_active = rgb(0xe8c17c).into();
        colors.button_primary = colors.primary;
        colors.button_primary_foreground = colors.primary_foreground;
        colors.button_primary_hover = colors.primary_hover;
        colors.button_primary_active = colors.primary_active;
        colors.ring = rgb(0x8c651f).into();
    });
}

pub(super) fn window_background(theme: &Theme) -> Hsla {
    if theme.is_dark() {
        rgb(0x27221e).into()
    } else {
        rgb(0xf7f7f5).into()
    }
}

pub(super) fn card_background(theme: &Theme) -> Background {
    theme.background.into()
}

pub(super) fn selected_background(theme: &Theme) -> Hsla {
    if theme.is_dark() {
        theme.accent
    } else {
        rgb(0xfff8ec).into()
    }
}

pub(super) fn row_border(theme: &Theme) -> Hsla {
    if theme.is_dark() {
        theme.border
    } else {
        rgb(0xe8e8e5).into()
    }
}
