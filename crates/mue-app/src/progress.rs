use crate::{preferences, ui::SharedSettings};
use gpui::{Context, Render, SharedString, Window, div, prelude::*, px};
use gpui_component::{ActiveTheme, button::Button};
use mue_core::conversion::{Engine, JobStatus};
use mue_core::profiles::InterfaceLanguage;

pub(crate) struct ProgressView {
    pub(crate) engine: Engine,
    pub(crate) settings: SharedSettings,
    pub(crate) language: Option<InterfaceLanguage>,
}

impl Render for ProgressView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.settings.lock().unwrap().general.language;
        let t = |text| preferences::text(language, text);
        if self.language != Some(language) {
            window.set_window_title(&format!("Mue — {}", t("Conversions")));
            crate::platform::update_titlebar(window, cx);
            self.language = Some(language);
        }
        let state = self.engine.state.lock().unwrap();
        let waiting = state
            .jobs
            .iter()
            .filter(|job| matches!(job.status, JobStatus::Queued))
            .count();
        let mut content = div()
            .id("conversion-list")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3();
        if state.jobs.is_empty() {
            content = content.child(div().text_sm().child(t("No conversions yet.")));
        }
        let mut jobs: Vec<_> = state.jobs.iter().collect();
        jobs.sort_by_key(|job| !job.status.active());
        for job in jobs {
            let filename = job
                .source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let status = match &job.status {
                JobStatus::Queued => t("Waiting").into(),
                JobStatus::Running => job.progress.map_or(t("Converting…").into(), |p| {
                    format!(
                        "{:.0}%{}",
                        p * 100.0,
                        job.remaining_seconds
                            .map_or(String::new(), |s| format!(" · ~{s}s"))
                    )
                }),
                JobStatus::Completed(_) => t("Completed").into(),
                JobStatus::Failed(error) => format!("{}: {error}", t("Failed")),
                JobStatus::Cancelled => t("Cancelled").into(),
            };
            let id = job.id;
            let mut row = div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
                .bg(cx.theme().secondary)
                .child(div().text_sm().child(filename))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(job.profile.name.clone()),
                )
                .child(div().text_xs().child(status));
            if matches!(job.status, JobStatus::Running) {
                row = row.child(
                    div()
                        .w_full()
                        .h(px(4.0))
                        .rounded_sm()
                        .bg(cx.theme().border)
                        .child(
                            div()
                                .h_full()
                                .w(gpui::relative(job.progress.unwrap_or(0.08)))
                                .bg(cx.theme().primary),
                        ),
                );
            }
            if job.status.active() {
                row = row.child(
                    Button::new(SharedString::from(format!("cancel-{id}")))
                        .label(t("Cancel"))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.engine.cancel(id);
                            cx.notify();
                        })),
                );
            } else if let JobStatus::Completed(output) = &job.status {
                let output = output.clone();
                row = row.child(
                    Button::new(SharedString::from(format!("reveal-{id}")))
                        .label(t("Show file"))
                        .on_click(move |_, _, cx| cx.reveal_path(&output)),
                );
            }
            content = content.child(row);
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_lg()
                            .child(format!("Mue · {waiting} {}", t("Waiting"))),
                    )
                    .child(
                        Button::new("dismiss")
                            .label(t("Hide"))
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
            .child(content)
    }
}
