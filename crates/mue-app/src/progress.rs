use crate::{preferences, ui::SharedSettings};
use gpui::{Context, Render, SharedString, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Disableable, Icon, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
};
use mue_core::conversion::{Engine, JobStatus};
use mue_core::profiles::InterfaceLanguage;

pub(crate) struct ProgressView {
    pub(crate) engine: Engine,
    pub(crate) settings: SharedSettings,
    pub(crate) language: Option<InterfaceLanguage>,
    pub(crate) expanded: bool,
    pub(crate) requested_height: f32,
}

impl ProgressView {
    pub(crate) fn window_height(engine: &Engine, expanded: bool) -> f32 {
        let state = engine.state.lock().unwrap();
        Self::height_for(
            expanded,
            state.jobs.len(),
            state
                .jobs
                .iter()
                .any(|job| matches!(job.status, JobStatus::Failed(_))),
        )
    }

    fn height_for(expanded: bool, job_count: usize, failed: bool) -> f32 {
        if expanded {
            (64.0 + job_count as f32 * 148.0).clamp(150.0, crate::platform::PROGRESS_MAX_HEIGHT)
        } else if failed {
            210.0
        } else {
            crate::platform::PROGRESS_HEIGHT
        }
    }

    pub(crate) fn show_queue(&mut self, cx: &mut Context<Self>) {
        self.expanded = true;
        cx.notify();
    }
}

impl Render for ProgressView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.settings.lock().unwrap().general.language;
        let t = |text| preferences::text(language, text);
        let button_style = ButtonCustomVariant::new(cx)
            .hover(cx.theme().accent)
            .active(cx.theme().border);
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
        let failed = state
            .jobs
            .iter()
            .filter(|job| matches!(job.status, JobStatus::Failed(_)))
            .count();
        let height = Self::height_for(self.expanded, state.jobs.len(), failed > 0);
        if self.requested_height != height {
            crate::platform::position_progress(window, height, false, cx);
            self.requested_height = height;
        }
        let mut content = div()
            .id("conversion-list")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3();
        if state.jobs.is_empty() {
            content = content.child(div().text_sm().child(t("No conversions yet.")));
        }
        let mut jobs: Vec<_> = state.jobs.iter().collect();
        jobs.sort_by_key(|job| !job.status.active());
        if !self.expanded {
            let current = state
                .jobs
                .iter()
                .find(|job| matches!(job.status, JobStatus::Running))
                .or_else(|| state.jobs.iter().find(|job| job.status.active()))
                .or_else(|| {
                    state
                        .jobs
                        .iter()
                        .rev()
                        .find(|job| matches!(job.status, JobStatus::Failed(_)))
                })
                .or_else(|| state.jobs.back());
            jobs = current.into_iter().collect();
        }
        for job in jobs {
            let filename = job
                .source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let mut status = match &job.status {
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
            if !self.expanded && waiting > 0 {
                status.push_str(&format!(" · {waiting} {}", t("Waiting")));
            }
            let id = job.id;
            let mut row = div()
                .flex()
                .flex_col()
                .gap_1()
                .when(self.expanded, |row| row.p_2())
                .rounded_md()
                .when(self.expanded, |row| row.bg(cx.theme().secondary))
                .child(div().text_sm().truncate().child(filename))
                .when(self.expanded, |row| {
                    row.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(job.profile.name.clone()),
                    )
                })
                .child(div().text_xs().whitespace_normal().child(status));
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
            if self.expanded && job.status.active() {
                row = row.child(
                    Button::new(SharedString::from(format!("cancel-{id}")))
                        .custom(button_style)
                        .rounded(px(6.0))
                        .small()
                        .icon(Icon::default().path("icons/close.svg"))
                        .tooltip(t("Cancel"))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.engine.cancel(id);
                            cx.notify();
                        })),
                );
            } else if self.expanded
                && let JobStatus::Completed(output) = &job.status
            {
                let output = output.clone();
                row = row.child(
                    Button::new(SharedString::from(format!("reveal-{id}")))
                        .custom(button_style)
                        .rounded(px(6.0))
                        .small()
                        .icon(Icon::default().path("icons/folder-open.svg"))
                        .tooltip(t("Show file"))
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
                    .child(div().text_sm().child("Mue"))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new("clear-finished")
                                    .custom(button_style)
                                    .rounded(px(6.0))
                                    .small()
                                    .icon(Icon::default().path("icons/trash.svg"))
                                    .tooltip(t("Clear finished conversions"))
                                    .disabled(!state.jobs.iter().any(|job| !job.status.active()))
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.engine.clear_finished();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("toggle-queue")
                                    .custom(button_style)
                                    .rounded(px(6.0))
                                    .small()
                                    .icon(Icon::default().path(if self.expanded {
                                        "icons/chevron-down.svg"
                                    } else {
                                        "icons/chevron-up.svg"
                                    }))
                                    .tooltip(t(if self.expanded {
                                        "Hide queue"
                                    } else {
                                        "Show queue"
                                    }))
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.expanded = !view.expanded;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("dismiss")
                                    .custom(button_style)
                                    .rounded(px(6.0))
                                    .small()
                                    .icon(Icon::default().path("icons/close.svg"))
                                    .tooltip(t("Hide"))
                                    .on_click(|_, window, _| window.remove_window()),
                            ),
                    ),
            )
            .when(!self.expanded && failed > 0, |view| {
                view.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(format!("{failed} · {}", t("Failed"))),
                )
            })
            .child(content)
    }
}
