use gpui::*;
use loganalysis_core::*;

struct LogAnalysisWorkspace {
    status_text: String,
}

impl Render for LogAnalysisWorkspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1a1a1e))
            .text_color(rgb(0xe2e2e6))
            .child(
                div()
                    .h(px(40.0))
                    .w_full()
                    .bg(rgb(0x121214))
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(rgb(0x2e2e34))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(14.0))
                            .child("LogAnalysis (High Performance Rust + GPUI)"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_color(rgb(0x8a8a90))
                            .child("Drag and drop log file here or press Ctrl+O to open"),
                    ),
            )
            .child(
                div()
                    .h(px(24.0))
                    .w_full()
                    .bg(rgb(0x121214))
                    .border_t_1()
                    .border_color(rgb(0x2e2e34))
                    .flex()
                    .items_center()
                    .px(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x8a8a90))
                            .child(self.status_text.clone()),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.0), px(800.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("LogAnalysis (High Performance)".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        };

        cx.open_window(options, |_window, cx| {
            cx.new(|_cx| LogAnalysisWorkspace {
                status_text: format!("Ready | MAX_BUFFER: {} lines", MAX_MONITOR_LINES),
            })
        })
        .unwrap();
    });
}
