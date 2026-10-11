use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use gpui::{
    ClickEvent, Context, Entity, ExternalPaths, FocusHandle, Focusable, Image, PathPromptOptions,
    Render, SharedString, Window, actions, div, img, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    menu::{PopupMenu, PopupMenuItem},
    popover::Popover,
    tooltip::Tooltip,
};
use mue_core::{
    Request,
    conversion::{Engine, Job, JobStatus},
    profiles::{InterfaceLanguage, MediaKind, Profile, Settings},
};
use uuid::Uuid;

use crate::{preferences, ui::SharedSettings};

actions!(conversions, [SelectAllFiles, DeselectAllFiles]);

struct PreparedFile {
    id: Uuid,
    path: PathBuf,
    kind: MediaKind,
    profile: Option<Uuid>,
    selected: bool,
    preview: Option<Arc<Image>>,
    preview_pending: bool,
}

pub(crate) struct ConversionsView {
    settings: SharedSettings,
    engine: Engine,
    requests: Arc<Mutex<Vec<Request>>>,
    files: Vec<PreparedFile>,
    message: String,
    language: Option<InterfaceLanguage>,
    open_picker: Option<Uuid>,
    picker_menu: Option<Entity<PopupMenu>>,
    focus_handle: FocusHandle,
    selection_anchor: Option<Uuid>,
    preview_running: bool,
}

impl ConversionsView {
    pub(crate) fn close_profile_menu(&mut self) {
        self.open_picker = None;
        self.picker_menu = None;
    }
    pub(crate) fn new(
        settings: SharedSettings,
        engine: Engine,
        requests: Arc<Mutex<Vec<Request>>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            settings,
            engine,
            requests,
            files: Vec::new(),
            message: String::new(),
            language: None,
            open_picker: None,
            picker_menu: None,
            focus_handle: cx.focus_handle(),
            selection_anchor: None,
            preview_running: false,
        }
    }

    fn select_all(&mut self, selected: bool, cx: &mut Context<Self>) {
        for file in &mut self.files {
            file.selected = selected;
        }
        self.selection_anchor = None;
        cx.notify();
    }

    fn select_file(&mut self, id: Uuid, selected: bool, shift: bool, cx: &mut Context<Self>) {
        let Some(index) = self.files.iter().position(|file| file.id == id) else {
            return;
        };
        let anchor = self
            .selection_anchor
            .and_then(|id| self.files.iter().position(|file| file.id == id));
        if let Some(anchor) = anchor.filter(|_| shift) {
            for file in &mut self.files[index.min(anchor)..=index.max(anchor)] {
                file.selected = true;
            }
        } else {
            self.files[index].selected = selected;
            self.selection_anchor = Some(id);
        }
        cx.notify();
    }

    fn clear_missing_anchor(&mut self) {
        if !self
            .files
            .iter()
            .any(|file| Some(file.id) == self.selection_anchor)
        {
            self.selection_anchor = None;
        }
    }

    fn add_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let language = self.settings.lock().unwrap().general.language;
        let active = self
            .engine
            .state
            .lock()
            .unwrap()
            .jobs
            .iter()
            .filter(|job| job.status.active())
            .count();
        let mut rejected = Vec::new();
        for path in paths {
            let resolved = std::fs::canonicalize(&path).ok();
            let Some((path, kind)) = resolved.and_then(|path| {
                let kind = MediaKind::for_path(&path)?;
                path.is_file().then_some((path, kind))
            }) else {
                rejected.push(format!(
                    "{}: {}",
                    preferences::text(language, "Unsupported or unavailable file"),
                    path.display()
                ));
                continue;
            };
            if self.files.iter().any(|file| file.path == path) {
                continue;
            }
            if self.files.len() + active >= 1000 {
                rejected.push(
                    preferences::text(language, "The queue is full (1000 files maximum)").into(),
                );
                break;
            }
            self.files.push(PreparedFile {
                id: Uuid::new_v4(),
                path,
                kind,
                profile: None,
                selected: false,
                preview: None,
                preview_pending: kind != MediaKind::Audio,
            });
        }
        self.message = rejected.join("\n");
        self.load_next_preview(cx);
        cx.notify();
    }

    fn load_next_preview(&mut self, cx: &mut Context<Self>) {
        if self.preview_running {
            return;
        }
        let Some(file) = self.files.iter_mut().find(|file| file.preview_pending) else {
            return;
        };
        file.preview_pending = false;
        let id = file.id;
        let path = file.path.clone();
        self.preview_running = true;
        // Generate one thumbnail at a time to avoid competing with the conversion worker.
        let task = cx
            .background_executor()
            .spawn(async move { crate::preview::thumbnail(&path) });
        cx.spawn(async move |view, cx| {
            let preview = task.await;
            let _ = view.update(cx, |view, cx| {
                if let Some(file) = view.files.iter_mut().find(|file| file.id == id) {
                    file.preview = preview;
                }
                view.preview_running = false;
                view.load_next_preview(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn pick_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.settings.lock().unwrap().general.language;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(preferences::text(language, "Add files…").into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = paths.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok(Ok(Some(paths))) => view.add_files(paths, cx),
                    Ok(Ok(None)) => return,
                    Ok(Err(error)) => view.message = format!("{error:#}"),
                    Err(error) => view.message = format!("{error:#}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn profile<'a>(settings: &'a Settings, file: &PreparedFile) -> Option<&'a Profile> {
        settings
            .defaults
            .iter()
            .chain(&settings.profiles)
            .find(|profile| Some(profile.id) == file.profile && profile.format.kind() == file.kind)
    }

    fn start(&mut self, only: Option<Uuid>, cx: &mut Context<Self>) {
        let settings = self.settings.lock().unwrap().clone();
        let mut ids = Vec::new();
        let mut batch = Vec::new();
        for file in &self.files {
            if only.is_some_and(|id| id != file.id) {
                continue;
            }
            if let Some(profile) = Self::profile(&settings, file) {
                ids.push(file.id);
                batch.push((file.path.clone(), profile.clone()));
            }
        }
        if batch.is_empty() {
            return;
        }
        match self.engine.enqueue_batch(batch) {
            Ok(_) => {
                self.files.retain(|file| !ids.contains(&file.id));
                self.clear_missing_anchor();
                self.message.clear();
            }
            Err(error) => self.message = format!("{error:#}"),
        }
        cx.notify();
    }

    fn profile_picker(
        &self,
        id: Option<Uuid>,
        kind: Option<MediaKind>,
        current: Option<Uuid>,
        label: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let key = id.unwrap_or(Uuid::nil());
        let language = self.settings.lock().unwrap().general.language;
        let button = Button::new(SharedString::from(
            id.map_or_else(|| "bulk-profile".into(), |id| format!("profile-{id}")),
        ))
        .small()
        .outline()
        .w(px(245.0))
        .child(div().max_w(px(190.0)).truncate().child(label))
        .icon(Icon::default().path("icons/chevron-down.svg"))
        .tooltip(preferences::text(
            language,
            if id.is_some() {
                "Choose a profile"
            } else {
                "Apply a profile to selected files"
            },
        ))
        .disabled(kind.is_none())
        .on_click(cx.listener(move |view, event, window, cx| {
            if matches!(event, ClickEvent::Keyboard(_)) && kind.is_some() {
                view.open_profile_menu(id, kind, current, window, cx);
            }
        }));
        let menu = self
            .picker_menu
            .clone()
            .filter(|_| self.open_picker == Some(key));
        Popover::new(SharedString::from(format!("picker-{key}")))
            .open(self.open_picker == Some(key) && kind.is_some())
            .appearance(false)
            .trigger(button)
            .on_open_change(cx.listener(move |view, open, window, cx| {
                if *open && kind.is_some() {
                    view.open_profile_menu(id, kind, current, window, cx);
                } else {
                    view.open_picker = None;
                    view.picker_menu = None;
                    cx.notify();
                }
            }))
            .content(move |_, _, _| div().children(menu.clone()))
    }

    fn open_profile_menu(
        &mut self,
        id: Option<Uuid>,
        kind: Option<MediaKind>,
        current: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let settings = self.settings.lock().unwrap().clone();
        let return_focus = window.focused(cx);
        let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
            menu = menu.when_some(return_focus.clone(), |menu, focus| {
                menu.action_context(focus)
            });
            menu = menu.scrollable(true).max_h(px(360.0));
            for (title, profiles) in [
                ("Direct conversion defaults", &settings.defaults),
                ("Saved profiles", &settings.profiles),
            ] {
                menu = menu.label(preferences::text(settings.general.language, title));
                for profile in profiles
                    .iter()
                    .filter(|profile| Some(profile.format.kind()) == kind)
                {
                    let profile_id = profile.id;
                    let view = view.clone();
                    let label = if title == "Direct conversion defaults" {
                        profile.format.label().to_owned()
                    } else {
                        format!("{} · {}", profile.name, profile.format.label())
                    };
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(current == Some(profile_id))
                            .on_click(move |_, _, cx| {
                                let _ = view.update(cx, |view, cx| {
                                    for file in &mut view.files {
                                        if Some(file.kind) == kind
                                            && id.map_or(file.selected, |id| id == file.id)
                                        {
                                            file.profile = Some(profile_id);
                                        }
                                    }
                                    view.message.clear();
                                    view.open_picker = None;
                                    view.picker_menu = None;
                                    cx.notify();
                                });
                            }),
                    );
                }
            }
            menu
        });
        cx.subscribe_in(
            &menu,
            window,
            |view, dismissed, _: &gpui::DismissEvent, _, cx| {
                if view
                    .picker_menu
                    .as_ref()
                    .is_some_and(|menu| menu.entity_id() == dismissed.entity_id())
                {
                    view.close_profile_menu();
                    cx.notify();
                }
            },
        )
        .detach();
        menu.focus_handle(cx).focus(window);
        self.open_picker = Some(id.unwrap_or(Uuid::nil()));
        self.picker_menu = Some(menu);
        cx.notify();
    }

    fn file_label(
        id: Uuid,
        path: &std::path::Path,
        kind: MediaKind,
        preview: Option<Arc<Image>>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let full_path = display_path(path);
        let icon_path = match kind {
            MediaKind::Image => "icons/image.svg",
            MediaKind::Video => "icons/video.svg",
            MediaKind::Audio => "icons/audio.svg",
        };
        let icon = move || Icon::default().path(icon_path).into_any_element();
        div()
            .id(SharedString::from(format!("file-label-{id}")))
            .flex()
            .items_center()
            .gap_3()
            .flex_1()
            .min_w(px(0.0))
            .child(
                div()
                    .w(px(48.0))
                    .h(px(36.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(match preview {
                        Some(preview) => img(preview)
                            .size_full()
                            .with_loading(icon)
                            .with_fallback(icon)
                            .into_any_element(),
                        None => icon(),
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap_1()
                    .child(
                        div().text_sm().truncate().child(
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                        ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(full_path.clone()),
                    ),
            )
            .tooltip(move |window, cx| Tooltip::new(full_path.clone()).build(window, cx))
    }

    fn job_row(
        &self,
        job: &Job,
        language: InterfaceLanguage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = |text| preferences::text(language, text);
        let id = job.id;
        let status = crate::progress::job_status(job, language);
        let mut actions = div().flex().gap_1().w(px(72.0)).justify_end();
        if job.status.active() {
            actions = actions.child(
                Button::new(SharedString::from(format!("cancel-{id}")))
                    .small()
                    .ghost()
                    .icon(Icon::default().path("icons/close.svg"))
                    .tooltip(t("Cancel"))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.engine.cancel(id);
                        cx.notify();
                    })),
            );
        } else if matches!(job.status, JobStatus::Cancelled) {
            let source = job.source.clone();
            let profile = job.profile.id;
            actions = actions.child(
                Button::new(SharedString::from(format!("restore-{id}")))
                    .small()
                    .ghost()
                    .icon(Icon::default().path("icons/undo.svg"))
                    .tooltip(t("Return to list"))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.add_files(vec![source.clone()], cx);
                        if let Some(file) = view.files.iter_mut().find(|file| file.path == source) {
                            file.profile.get_or_insert(profile);
                            view.engine.state.lock().unwrap().jobs.retain(|job| {
                                job.id != id || !matches!(job.status, JobStatus::Cancelled)
                            });
                            view.focus_handle.focus(window);
                            cx.notify();
                        }
                    })),
            );
        } else if let JobStatus::Completed(output) = &job.status {
            let output = output.clone();
            actions = actions.child(
                Button::new(SharedString::from(format!("reveal-{id}")))
                    .small()
                    .ghost()
                    .icon(Icon::default().path("icons/folder-open.svg"))
                    .tooltip(t("Show file"))
                    .on_click(move |_, _, cx| crate::platform::reveal_path(&output, cx)),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .bg(cx.theme().secondary)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().w(px(16.0)))
                    .child(Self::file_label(
                        id,
                        &job.source,
                        job.profile.format.kind(),
                        None,
                        cx,
                    ))
                    .child(div().w(px(245.0)).text_sm().truncate().child(format!(
                        "{} · {}",
                        job.profile.name,
                        job.profile.format.label()
                    )))
                    .child(actions),
            )
            .child(
                div()
                    .pl(px(28.0))
                    .text_xs()
                    .whitespace_normal()
                    .when(matches!(job.status, JobStatus::Failed(_)), |row| {
                        row.text_color(cx.theme().danger)
                    })
                    .child(status),
            )
            .when(matches!(job.status, JobStatus::Running), |row| {
                row.child(
                    div()
                        .w_full()
                        .h(px(3.0))
                        .rounded_sm()
                        .bg(cx.theme().border)
                        .child(
                            div()
                                .h_full()
                                .w(gpui::relative(job.progress.unwrap_or(0.0)))
                                .bg(cx.theme().primary),
                        ),
                )
            })
    }
}

fn display_path(path: &std::path::Path) -> String {
    let path = path.display().to_string();
    if let Some(path) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else if let Some(path) = path
        .strip_prefix(r"\\?\")
        .filter(|path| path.as_bytes().get(1..3) == Some(b":\\"))
    {
        path.to_owned()
    } else {
        path
    }
}

impl Focusable for ConversionsView {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ConversionsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.settings.lock().unwrap().clone();
        let language = settings.general.language;
        let t = |text| preferences::text(language, text);
        if self.language != Some(language) {
            window.set_window_title(&format!("Mue — {}", t("Conversions")));
            self.language = Some(language);
        }
        let jobs: Vec<_> = self
            .engine
            .state
            .lock()
            .unwrap()
            .jobs
            .iter()
            .cloned()
            .collect();
        let selected: Vec<_> = self.files.iter().filter(|file| file.selected).collect();
        let selected_count = selected.len();
        let common_kind = selected
            .first()
            .map(|file| file.kind)
            .filter(|kind| selected.iter().all(|file| file.kind == *kind));
        let ready = self
            .files
            .iter()
            .filter(|file| Self::profile(&settings, file).is_some())
            .count();
        let mut list = div()
            .id("prepared-file-list")
            .track_focus(&self.focus_handle)
            .key_context("ConversionList")
            .on_action(cx.listener(|view, _: &SelectAllFiles, _, cx| {
                if view.open_picker.is_some() {
                    cx.propagate();
                    return;
                }
                view.select_all(true, cx);
            }))
            .on_action(cx.listener(|view, _: &DeselectAllFiles, _, cx| {
                if view.open_picker.is_some() {
                    cx.propagate();
                    return;
                }
                view.select_all(false, cx);
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|view, _, window, _| {
                    view.focus_handle.focus(window);
                }),
            )
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2();
        if self.files.is_empty() && jobs.is_empty() {
            list = list.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded_lg()
                    .p_6()
                    .child(
                        Icon::default()
                            .path("icons/upload.svg")
                            .size(px(40.0))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(div().text_lg().child(t("Drop files here")))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(t("Images, videos and audio · or use the + button")),
                    ),
            );
        }
        for file in &self.files {
            let id = file.id;
            let checked = file.selected;
            let selection_hint = format!(
                "{}: {}",
                t("Select file"),
                file.path.file_name().unwrap_or_default().to_string_lossy()
            );
            let profile = Self::profile(&settings, file);
            let label = profile.map_or_else(
                || t("Choose a profile").into(),
                |profile| {
                    if settings.defaults.iter().any(|p| p.id == profile.id) {
                        profile.format.label().into()
                    } else {
                        profile.name.clone()
                    }
                },
            );
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(if file.selected {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                Checkbox::new(SharedString::from(format!("select-{id}")))
                                    .checked(file.selected)
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(selection_hint.clone()).build(window, cx)
                                    })
                                    .on_click(cx.listener(move |view, checked, window, cx| {
                                        view.select_file(
                                            id,
                                            *checked,
                                            window.modifiers().shift,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("select-label-{id}")))
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(Self::file_label(
                                        id,
                                        &file.path,
                                        file.kind,
                                        file.preview.clone(),
                                        cx,
                                    ))
                                    .on_click(cx.listener(
                                        move |view, event: &ClickEvent, window, cx| {
                                            view.focus_handle.focus(window);
                                            view.select_file(
                                                id,
                                                !checked,
                                                event.modifiers().shift,
                                                cx,
                                            );
                                        },
                                    )),
                            )
                            .child(self.profile_picker(
                                Some(id),
                                Some(file.kind),
                                file.profile,
                                label,
                                cx,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .w(px(72.0))
                                    .justify_end()
                                    .child(
                                        Button::new(SharedString::from(format!("start-{id}")))
                                            .small()
                                            .ghost()
                                            .icon(Icon::default().path("icons/play.svg"))
                                            .tooltip(t("Convert this file"))
                                            .disabled(profile.is_none())
                                            .on_click(cx.listener(move |view, _, _, cx| {
                                                view.start(Some(id), cx)
                                            })),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!("remove-{id}")))
                                            .small()
                                            .ghost()
                                            .icon(Icon::default().path("icons/trash.svg"))
                                            .tooltip(t("Remove file"))
                                            .on_click(cx.listener(move |view, _, _, cx| {
                                                view.files.retain(|file| file.id != id);
                                                view.clear_missing_anchor();
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .pl(px(28.0))
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(t(if profile.is_some() {
                                "Ready"
                            } else if file.profile.is_some() {
                                "Profile no longer available"
                            } else {
                                "Choose a profile before converting"
                            })),
                    ),
            );
        }
        for job in &jobs {
            list = list.child(self.job_row(job, language, cx));
        }
        div().id("conversions-view").size_full().flex().flex_col().gap_4().p_5()
            .bg(cx.theme().background).text_color(cx.theme().foreground)
            .drag_over::<ExternalPaths>(|style, _, _, cx| style.bg(cx.theme().accent))
            .on_drop(cx.listener(|view, paths: &ExternalPaths, _, cx| view.add_files(paths.paths().to_vec(), cx)))
            .child(div().flex().items_center().justify_between().gap_3()
                .child(div().flex().flex_col().gap_1().child(div().text_xl().child(t("Conversions")))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground)
                        .child(format!("{} {} · {ready} {}", self.files.len(), t("Prepared"), t("Ready")))))
                .child(div().flex().items_center().gap_2()
                    .child(Button::new("add-files").outline().icon(Icon::default().path("icons/plus.svg")).tooltip(t("Add files…"))
                        .on_click(cx.listener(|view, _, window, cx| view.pick_files(window, cx))))
                    .child(Button::new("start-queue").primary().icon(Icon::default().path("icons/list-play.svg")).tooltip(t("Convert ready files"))
                        .disabled(ready == 0).on_click(cx.listener(|view, _, _, cx| view.start(None, cx))))
                    .child(Button::new("clear-finished").ghost().icon(Icon::default().path("icons/trash.svg")).tooltip(t("Clear finished conversions"))
                        .disabled(!jobs.iter().any(|job| !job.status.active()))
                        .on_click(cx.listener(|view, _, _, cx| { view.engine.clear_finished(); cx.notify(); })))
                    .child(Button::new("open-settings").ghost().icon(Icon::default().path("icons/settings.svg")).tooltip(t("Settings"))
                        .on_click(cx.listener(|view, _, _, _| view.requests.lock().unwrap().push(Request::Settings))))))
            .child(div().flex().flex_wrap().items_center().gap_3()
                .child(Checkbox::new("select-all").label(t("Select all")).checked(!self.files.is_empty() && selected_count == self.files.len())
                    .tooltip(move |window, cx| Tooltip::new(format!("{} · Ctrl+A", preferences::text(language, "Select all"))).build(window, cx))
                    .disabled(self.files.is_empty()).on_click(cx.listener(|view, checked, window, cx| {
                        view.focus_handle.focus(window);
                        view.select_all(*checked, cx);
                    })))
                .when(selected_count > 0, |bar| bar
                    .child(div().text_sm().child(format!("{selected_count} {}", t("Selected"))))
                    .child(Button::new("deselect-all").small().ghost().label(t("Deselect all"))
                        .tooltip(format!("{} · Ctrl+Shift+A", t("Deselect all")))
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.focus_handle.focus(window);
                            view.select_all(false, cx);
                        })))
                    .child(self.profile_picker(None, common_kind, None, t("Apply a profile…").into(), cx))
                    .child(Button::new("remove-selected").small().ghost().icon(Icon::default().path("icons/trash.svg")).tooltip(t("Remove selected files"))
                        .on_click(cx.listener(|view, _, _, cx| { view.files.retain(|file| !file.selected); view.clear_missing_anchor(); cx.notify(); })))
                    .when(common_kind.is_none(), |bar| bar.child(div().text_xs().flex_1().child(t("Select files of the same type to apply a profile."))))))
            .when(!self.message.is_empty(), |view| view.child(div().id("queue-message").max_h(px(100.0)).overflow_y_scroll()
                .text_sm().whitespace_normal().text_color(cx.theme().danger).child(self.message.clone())))
            .child(list)
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child(t("Originals are preserved. Outputs are saved next to the source without overwriting files.")))
    }
}
