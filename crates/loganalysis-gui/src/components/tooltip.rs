use gpui::*;

use crate::theme::Theme;

pub struct SimpleTooltip {
    pub text: SharedString,
    pub shortcut: Option<&'static str>,
    pub theme: Theme,
}

impl Render for SimpleTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut container = div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded_md()
            .bg(self.theme.bg_card)
            .border_1()
            .border_color(self.theme.border)
            .shadow_lg()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(self.theme.text_primary)
                    .child(self.text.clone()),
            );

        if let Some(shortcut) = self.shortcut {
            container = container.child(
                div()
                    .px(px(4.0))
                    .py(px(0.5))
                    .rounded_xs()
                    .bg(self.theme.kbd_bg)
                    .border_1()
                    .border_color(self.theme.kbd_border)
                    .text_size(px(9.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .font_family("monospace")
                    .text_color(self.theme.kbd_fg)
                    .child(shortcut),
            );
        }

        container
    }
}

pub fn render_tooltip(
    text: impl Into<SharedString>,
    shortcut: Option<&'static str>,
    theme: Theme,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |_window, cx| {
        let tooltip = SimpleTooltip {
            text: text.clone(),
            shortcut,
            theme,
        };
        cx.new(|_| tooltip).into()
    }
}
