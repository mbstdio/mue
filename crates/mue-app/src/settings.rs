use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{Context as _, Result};
use gpui::{
    App, Context, Entity, PathPromptOptions, Render, SharedString, Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, Selectable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputState},
};
use mue_core::{
    FFMPEG_VERSION, Request,
    conversion::Engine,
    profiles::{
        EncodingSpeed, GeneralSettings, InterfaceLanguage, MediaKind, OutputFormat, Profile,
        Settings, ThemePreference,
    },
};

use crate::{platform, preferences, ui::SharedSettings};

pub(crate) struct SettingsView {
    shared: SharedSettings,
    engine: Engine,
    settings: Settings,
    selected: (bool, usize),
    draft: Profile,
    inputs: HashMap<&'static str, Entity<InputState>>,
    message: String,
    category: Category,
    ui_requests: Arc<Mutex<Vec<Request>>>,
    drafts: HashMap<uuid::Uuid, ProfileDraft>,
    media_selection: [Option<uuid::Uuid>; 2],
}

struct ProfileDraft {
    profile: Profile,
    // Keep raw input, including incomplete numbers, when navigating away from a profile.
    inputs: HashMap<&'static str, Entity<InputState>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    General,
    Image,
    Video,
}

impl Category {
    fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Image => "Image",
            Self::Video => "Video",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::General => "Customize appearance, startup and conversion notifications.",
            Self::Image => "Configure image formats and saved conversion profiles.",
            Self::Video => "Configure video formats and saved conversion profiles.",
        }
    }
    fn kind(self) -> Option<MediaKind> {
        match self {
            Self::General => None,
            Self::Image => Some(MediaKind::Image),
            Self::Video => Some(MediaKind::Video),
        }
    }
}

impl SettingsView {
    pub(crate) fn new(
        shared: SharedSettings,
        engine: Engine,
        ui_requests: Arc<Mutex<Vec<Request>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = shared.lock().unwrap().clone();
        let draft = settings.defaults[0].clone();
        let mut view = Self {
            shared,
            engine,
            settings,
            selected: (true, 0),
            draft,
            inputs: HashMap::new(),
            message: String::new(),
            category: Category::General,
            ui_requests,
            drafts: HashMap::new(),
            media_selection: [None, None],
        };
        view.populate(window, cx);
        view
    }

    fn populate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inputs.clear();
        for (key, value) in [
            ("name", self.draft.name.clone()),
            ("quality", self.draft.quality.to_string()),
            ("compression", self.draft.png_compression.to_string()),
            (
                "width",
                self.draft
                    .max_width
                    .map_or(String::new(), |v| v.to_string()),
            ),
            (
                "height",
                self.draft
                    .max_height
                    .map_or(String::new(), |v| v.to_string()),
            ),
            ("background", self.draft.jpeg_background.clone()),
            ("crf", self.draft.video_crf.to_string()),
            (
                "fps",
                self.draft.max_fps.map_or(String::new(), |v| v.to_string()),
            ),
            ("bitrate", self.draft.audio_bitrate_kbps.to_string()),
        ] {
            self.inputs.insert(
                key,
                cx.new(|cx| InputState::new(window, cx).default_value(value)),
            );
        }
        cx.notify();
    }

    fn select(&mut self, default: bool, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected == (default, index) {
            return;
        }
        self.drafts.insert(
            self.draft.id,
            ProfileDraft {
                profile: self.draft.clone(),
                inputs: std::mem::take(&mut self.inputs),
            },
        );
        self.selected = (default, index);
        self.draft = if default {
            self.settings.defaults[index].clone()
        } else {
            self.settings.profiles[index].clone()
        };
        self.message.clear();
        self.media_selection[if self.draft.format.kind() == MediaKind::Image {
            0
        } else {
            1
        }] = Some(self.draft.id);
        if let Some(draft) = self.drafts.remove(&self.draft.id) {
            self.draft = draft.profile;
            self.inputs = draft.inputs;
            cx.notify();
        } else {
            self.populate(window, cx);
        }
    }

    fn change_category(&mut self, category: Category, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(kind) = category.kind() {
            if self.draft.format.kind() != kind {
                let id = self.media_selection[if kind == MediaKind::Image { 0 } else { 1 }];
                if let Some(index) = self
                    .settings
                    .profiles
                    .iter()
                    .position(|profile| Some(profile.id) == id)
                {
                    self.select(false, index, window, cx);
                } else {
                    let index = self
                        .settings
                        .defaults
                        .iter()
                        .position(|profile| Some(profile.id) == id)
                        .unwrap_or_else(|| {
                            self.settings
                                .defaults
                                .iter()
                                .position(|profile| profile.format.kind() == kind)
                                .unwrap()
                        });
                    self.select(true, index, window, cx);
                }
            }
        }
        self.category = category;
        self.message.clear();
        cx.notify();
    }

    fn t(&self, text: &'static str) -> &'static str {
        preferences::text(self.settings.general.language, text)
    }

    fn save_general(
        &mut self,
        general: GeneralSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut settings = self.settings.clone();
        let startup_changed = general.launch_at_startup != settings.general.launch_at_startup;
        settings.general = general;
        let result = if startup_changed {
            platform::configure_startup(settings.general.launch_at_startup, || settings.save())
        } else {
            settings.save()
        };
        match result {
            Ok(()) => {
                *self.shared.lock().unwrap() = settings.clone();
                self.settings = settings;
                preferences::apply(&self.settings.general, cx);
                window.set_window_title(&format!("Mue — {}", self.t("Settings")));
                platform::update_titlebar(window, cx);
                self.message = self.t("Preferences saved.").into();
            }
            Err(error) => self.message = format!("{error:#}"),
        }
        cx.notify();
    }

    fn change_format(&mut self, format: OutputFormat, window: &mut Window, cx: &mut Context<Self>) {
        match self.read_form(cx) {
            Ok(mut draft) => {
                draft.set_format(format);
                self.draft = draft;
                self.message.clear();
                self.populate(window, cx);
            }
            Err(error) => {
                self.message = format!("{error:#}");
                cx.notify();
            }
        }
    }

    fn read_form(&self, cx: &App) -> Result<Profile> {
        let value = |key| self.inputs[key].read(cx).value().to_string();
        let optional = |key: &'static str| -> Result<Option<u32>> {
            let text = value(key);
            if text.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(
                    text.trim()
                        .parse()
                        .with_context(|| format!("Invalid {key}"))?,
                ))
            }
        };
        let mut profile = self.draft.clone();
        profile.name = value("name").trim().into();
        profile.max_width = optional("width")?;
        profile.max_height = optional("height")?;
        if profile.format.kind() == MediaKind::Image {
            profile.quality = value("quality").parse().context("Invalid image quality")?;
            profile.png_compression = value("compression")
                .parse()
                .context("Invalid PNG compression")?;
            profile.jpeg_background = value("background")
                .trim()
                .trim_start_matches('#')
                .to_uppercase();
        } else {
            profile.video_crf = value("crf").parse().context("Invalid CRF")?;
            profile.max_fps = optional("fps")?;
            profile.audio_bitrate_kbps =
                value("bitrate").parse().context("Invalid audio bitrate")?;
        }
        profile.validate()?;
        Ok(profile)
    }

    fn persist(&mut self, settings: Settings) -> Result<()> {
        settings.save()?;
        *self.shared.lock().unwrap() = settings.clone();
        self.settings = settings;
        Ok(())
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let result = (|| -> Result<()> {
            let profile = self.read_form(cx)?;
            let mut settings = self.settings.clone();
            if self.selected.0 {
                settings.defaults[self.selected.1] = profile.clone();
            } else {
                settings.profiles[self.selected.1] = profile.clone();
            }
            self.persist(settings)?;
            self.draft = profile;
            Ok(())
        })();
        self.message = match result {
            Ok(()) => self
                .t("Saved. Reopen the context menu to see changes.")
                .into(),
            Err(error) => format!("{error:#}"),
        };
        cx.notify();
    }

    fn add(&mut self, duplicate: bool, window: &mut Window, cx: &mut Context<Self>) {
        let result = (|| -> Result<()> {
            let mut profile = if duplicate {
                self.read_form(cx)?
            } else {
                Profile::new(self.draft.format)
            };
            profile.id = uuid::Uuid::new_v4();
            if duplicate {
                profile.name.push_str(" copy");
            }
            let mut settings = self.settings.clone();
            settings.profiles.push(profile);
            self.persist(settings)?;
            self.select(false, self.settings.profiles.len() - 1, window, cx);
            Ok(())
        })();
        if let Err(error) = result {
            self.message = format!("{error:#}");
        }
        cx.notify();
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.0 {
            return;
        }
        let mut settings = self.settings.clone();
        let deleted_id = self.draft.id;
        settings.profiles.remove(self.selected.1);
        match self.persist(settings) {
            Ok(()) => {
                let index = self
                    .settings
                    .defaults
                    .iter()
                    .position(|profile| profile.format.kind() == self.draft.format.kind())
                    .unwrap();
                self.select(true, index, window, cx);
                self.drafts.remove(&deleted_id);
            }
            Err(error) => {
                self.message = format!("{error:#}");
                cx.notify();
            }
        }
    }

    fn convert_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let profile = match self.read_form(cx) {
            Ok(profile) => profile,
            Err(error) => {
                self.message = format!("{error:#}");
                cx.notify();
                return;
            }
        };
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(self.t("Convert with Mue").into()),
        });
        let engine = self.engine.clone();
        cx.spawn_in(window, async move |view, cx| {
            let result = match paths.await {
                Ok(Ok(Some(files))) => engine.enqueue(files, profile),
                Ok(Ok(None)) => return,
                Ok(Err(error)) => Err(error),
                Err(error) => Err(error.into()),
            };
            let _ = view.update(cx, |view, cx| {
                view.message = result.map_or_else(
                    |error| format!("{error:#}"),
                    |_| view.t("Conversion added to queue.").into(),
                );
                cx.notify();
            });
        })
        .detach();
    }

    fn field(&self, label: &'static str, key: &'static str) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .flex_1()
            .child(div().text_sm().child(self.t(label)))
            .child(Input::new(&self.inputs[key]))
    }

    fn section(&self, title: &'static str, cx: &App) -> gpui::Div {
        div()
            .mt_2()
            .pb_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .text_lg()
            .child(self.t(title))
    }

    fn hint(&self, text: &'static str, cx: &App) -> gpui::Div {
        div()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(self.t(text))
    }

    fn general_fields(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut themes = div().flex().gap_2();
        for (index, preference, label) in [
            (0, ThemePreference::System, "System"),
            (1, ThemePreference::Light, "Light"),
            (2, ThemePreference::Dark, "Dark"),
        ] {
            themes = themes.child(
                Button::new(("theme", index as usize))
                    .label(self.t(label))
                    .when(self.settings.general.theme == preference, |button| {
                        button.primary()
                    })
                    .on_click(cx.listener(move |view, _, window, cx| {
                        let mut general = view.settings.general.clone();
                        general.theme = preference;
                        view.save_general(general, window, cx);
                    })),
            );
        }
        let mut languages = div().flex().gap_2();
        for (index, language, label) in [
            (0, InterfaceLanguage::System, "System"),
            (1, InterfaceLanguage::English, "English"),
            (2, InterfaceLanguage::French, "French"),
        ] {
            languages = languages.child(
                Button::new(("language", index as usize))
                    .label(self.t(label))
                    .when(self.settings.general.language == language, |button| {
                        button.primary()
                    })
                    .on_click(cx.listener(move |view, _, window, cx| {
                        let mut general = view.settings.general.clone();
                        general.language = language;
                        view.save_general(general, window, cx);
                    })),
            );
        }
        div().flex().flex_col().gap_4()
            .child(self.section("Appearance", cx))
            .child(div().text_sm().child(self.t("Theme")))
            .child(themes)
            .child(div().text_sm().child(self.t("Interface language")))
            .child(languages)
            .child(self.hint("Applied and saved immediately.", cx))
            .child(self.section("Startup", cx))
            .child(Checkbox::new("startup").label(self.t("Launch at sign-in"))
                .disabled(!cfg!(windows)).checked(self.settings.general.launch_at_startup)
                .on_click(cx.listener(|view, checked, window, cx| {
                    let mut general = view.settings.general.clone();
                    general.launch_at_startup = *checked;
                    view.save_general(general, window, cx);
                })))
            .child(self.hint("Mue starts in the notification area. Closing settings leaves it running.", cx))
            .child(self.section("Conversions", cx))
            .child(Checkbox::new("auto-hide").label(self.t("Automatically hide completed conversions"))
                .checked(self.settings.general.auto_hide_completed)
                .on_click(cx.listener(|view, checked, window, cx| {
                    let mut general = view.settings.general.clone();
                    general.auto_hide_completed = *checked;
                    view.save_general(general, window, cx);
                })))
            .child(self.hint("Hide after six seconds when all conversions succeed. Errors remain visible.", cx))
            .child(self.section("About", cx))
            .child(div().text_sm().child(concat!("Mue v", env!("CARGO_PKG_VERSION"))))
            .child(div().text_sm().child(format!("FFmpeg {FFMPEG_VERSION} / ffprobe {FFMPEG_VERSION}")))
            .child(self.hint("Originals are preserved. Outputs are saved next to the source without overwriting files.", cx))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("settings-folder").label(self.t("Open settings folder"))
                    .on_click(cx.listener(|view, _, _, cx| {
                        if let Err(error) = platform::open_settings_folder() {
                            view.message = format!("{error:#}"); cx.notify();
                        }
                    })))
                .child(Button::new("show-conversions").label(self.t("Show conversions"))
                    .on_click(cx.listener(|view, _, _, _| view.ui_requests.lock().unwrap().push(Request::Progress))))
                .child(Button::new("quit").label(self.t("Quit Mue"))
                    .on_click(cx.listener(|view, _, _, cx| { view.engine.stop(); cx.quit(); }))))
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .w(px(200.0))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(div().px_3().pt_3().text_2xl().child("Mue"))
            .child(
                div()
                    .px_3()
                    .py_4()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.t("SETTINGS")),
            );
        for (category, icon) in [
            (Category::General, "icons/settings.svg"),
            (Category::Image, "icons/image.svg"),
            (Category::Video, "icons/video.svg"),
        ] {
            sidebar = sidebar.child(
                Button::new(("category", category as usize))
                    .ghost()
                    .w_full()
                    .justify_start()
                    .icon(Icon::default().path(icon))
                    .label(self.t(category.title()))
                    .selected(self.category == category)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.change_category(category, window, cx)
                    })),
            );
        }
        sidebar = sidebar.child(div().flex_1()).child(
            div()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(concat!("v", env!("CARGO_PKG_VERSION"))),
        );
        let header = div()
            .flex()
            .flex_col()
            .gap_2()
            .pb_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().text_xl().child(self.t(self.category.title())))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.t(self.category.description())),
            );
        if self.category == Category::General {
            return div()
                .size_full()
                .flex()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .child(sidebar)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .p_6()
                        .gap_4()
                        .child(header)
                        .child(
                            div()
                                .id("general-content")
                                .flex_1()
                                .min_h(px(0.0))
                                .overflow_y_scroll()
                                .child(self.general_fields(cx)),
                        )
                        .child(
                            div()
                                .border_t_1()
                                .border_color(cx.theme().border)
                                .pt_3()
                                .text_sm()
                                .child(self.message.clone()),
                        ),
                )
                .into_any_element();
        }
        let kind = self.category.kind().unwrap();
        let mut defaults = div().flex().flex_wrap().gap_2();
        for (index, profile) in self
            .settings
            .defaults
            .iter()
            .enumerate()
            .filter(|(_, profile)| profile.format.kind() == kind)
        {
            defaults =
                defaults.child(
                    Button::new(("default", index))
                        .label(profile.format.label())
                        .when(self.selected == (true, index), |button| button.primary())
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.select(true, index, window, cx)
                        })),
                );
        }
        let mut profiles = div().flex().flex_wrap().gap_2();
        let mut has_profiles = false;
        for (index, profile) in self
            .settings
            .profiles
            .iter()
            .enumerate()
            .filter(|(_, profile)| profile.format.kind() == kind)
        {
            has_profiles = true;
            profiles = profiles.child(
                Button::new(("profile", index))
                    .label(SharedString::from(profile.name.clone()))
                    .when(self.selected == (false, index), |button| button.primary())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.select(false, index, window, cx)
                    })),
            );
        }
        profiles = profiles.child(
            Button::new("new-profile")
                .label(self.t("+ New profile"))
                .on_click(cx.listener(|view, _, window, cx| view.add(false, window, cx))),
        );
        let image = self.draft.format.kind() == MediaKind::Image;
        let mut form = div()
            .id("profile-form")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_sm().child(self.t("Direct conversion defaults")))
            .child(defaults)
            .child(div().text_sm().child(self.t("Saved profiles")))
            .when(!has_profiles, |form| {
                form.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.t("No profiles yet. Create one below.")),
                )
            })
            .child(profiles)
            .child(
                div()
                    .mt_2()
                    .pt_4()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_lg()
                    .child(self.t(if self.selected.0 {
                        "Direct conversion settings"
                    } else {
                        "Conversion profile"
                    })),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.t(
                    "Originals are preserved. Maximum sizes never upscale or distort the source.",
                )),
            )
            .child(self.field("Name", "name"));
        let mut formats = div().flex().gap_2();
        for format in OutputFormat::ALL
            .into_iter()
            .filter(|format| format.kind() == self.draft.format.kind())
        {
            formats = formats.child(
                Button::new(format.extension())
                    .label(format.label())
                    .when(self.draft.format == format, |button| button.primary())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !view.selected.0 {
                            view.change_format(format, window, cx);
                        }
                    })),
            );
        }
        if self.selected.0 {
            form = form.child(div().child(format!(
                "{}: {}",
                self.t("Output format"),
                self.draft.format.label()
            )));
        } else {
            form = form
                .child(div().text_sm().child(self.t("Output format")))
                .child(formats);
        }
        form = form.child(
            div()
                .flex()
                .gap_4()
                .child(self.field("Maximum width (blank = original)", "width"))
                .child(self.field("Maximum height (blank = original)", "height")),
        );
        if image {
            form = form.child(if self.draft.format == OutputFormat::Png {
                self.field("PNG compression (0–9, lossless)", "compression")
                    .into_any_element()
            } else {
                self.field("Image quality (1–100)", "quality")
                    .into_any_element()
            });
            if self.draft.format == OutputFormat::Jpg {
                form = form.child(self.field("Transparency background (RGB hex)", "background"));
            }
        } else {
            form = form.child(
                div()
                    .flex()
                    .gap_4()
                    .child(self.field("CRF (lower = higher quality)", "crf"))
                    .child(self.field("Maximum FPS (blank = original)", "fps")),
            );
            let mut speeds = div().flex().gap_2();
            for (id, speed, label) in [
                ("fast", EncodingSpeed::Fast, "Fast"),
                ("balanced", EncodingSpeed::Balanced, "Balanced"),
                ("slow", EncodingSpeed::Slow, "Slow"),
            ] {
                speeds = speeds.child(
                    Button::new(id)
                        .label(self.t(label))
                        .when(self.draft.encoding_speed == speed, |b| b.primary())
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.draft.encoding_speed = speed;
                            cx.notify();
                        })),
                );
            }
            form = form
                .child(div().text_sm().child(self.t("Encoding speed")))
                .child(speeds)
                .child(self.field("Audio bitrate (16–512 kbps)", "bitrate"))
                .child(
                    Button::new("audio")
                        .label(self.t(if self.draft.keep_audio {
                            "Audio: keep"
                        } else {
                            "Audio: remove"
                        }))
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.draft.keep_audio = !view.draft.keep_audio;
                            cx.notify();
                        })),
                );
        }
        form = form.child(
            Button::new("metadata")
                .label(self.t(if self.draft.keep_metadata {
                    "Metadata: keep when supported"
                } else {
                    "Metadata: remove"
                }))
                .on_click(cx.listener(|view, _, _, cx| {
                    view.draft.keep_metadata = !view.draft.keep_metadata;
                    cx.notify();
                })),
        );
        let footer = div()
            .flex()
            .flex_col()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("save")
                            .primary()
                            .label(self.t("Save changes"))
                            .on_click(cx.listener(|view, _, _, cx| view.save(cx))),
                    )
                    .child(
                        Button::new("duplicate")
                            .label(self.t("Duplicate as profile"))
                            .on_click(
                                cx.listener(|view, _, window, cx| view.add(true, window, cx)),
                            ),
                    )
                    .when(!self.selected.0, |row| {
                        row.child(
                            Button::new("delete")
                                .danger()
                                .label(self.t("Delete profile"))
                                .on_click(
                                    cx.listener(|view, _, window, cx| view.delete(window, cx)),
                                ),
                        )
                    }),
            )
            .child(div().text_sm().child(self.message.clone()))
            .child(
                div().flex().gap_2().child(
                    Button::new("convert-files")
                        .label(self.t("Convert files…"))
                        .on_click(
                            cx.listener(|view, _, window, cx| view.convert_files(window, cx)),
                        ),
                ),
            );
        div()
            .size_full()
            .flex()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(sidebar)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .p_6()
                    .gap_4()
                    .child(header)
                    .child(form)
                    .child(footer),
            )
            .into_any_element()
    }
}
