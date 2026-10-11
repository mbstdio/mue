use gpui::{App, Entity, IntoElement, RenderOnce, Window, div, prelude::*};
use gpui_component::{
    ActiveTheme, Disableable, Sizable,
    button::Button,
    input::{Input, InputState},
};
use mue_core::profiles::{InterfaceLanguage, MediaKind};

use crate::preferences;

#[derive(IntoElement)]
pub(crate) struct FilenameTemplateEditor {
    input: Entity<InputState>,
    media_kind: MediaKind,
    language: InterfaceLanguage,
}

impl FilenameTemplateEditor {
    pub(crate) fn new(
        input: &Entity<InputState>,
        media_kind: MediaKind,
        language: InterfaceLanguage,
    ) -> Self {
        Self {
            input: input.clone(),
            media_kind,
            language,
        }
    }
}

impl RenderOnce for FilenameTemplateEditor {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = |text| preferences::text(self.language, text);
        let audio = self.media_kind == MediaKind::Audio;
        let mut variables = div().flex().flex_wrap().gap_2();
        for (token, description, dimension) in [
            ("{filename}", "Original filename without extension", false),
            ("{ext}", "Output extension", false),
            ("{profile}", "Profile name", false),
            ("{width}", "Converted file width", true),
            ("{height}", "Converted file height", true),
            ("{source_width}", "Original file width", true),
            ("{source_height}", "Original file height", true),
        ] {
            let input = self.input.clone();
            variables = variables.child(
                Button::new(token)
                    .small()
                    .label(token)
                    .tooltip(t(if audio && dimension {
                        "Dimensions are unavailable for audio."
                    } else {
                        description
                    }))
                    .disabled(audio && dimension)
                    .on_click(move |_, window, cx| {
                        input.update(cx, |state, cx| {
                            state.replace(token, window, cx);
                            state.focus(window, cx);
                        });
                    }),
            );
        }

        div()
            .id(("filename-template-editor", self.input.entity_id()))
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child(t("Output filename template")))
            .child(Input::new(&self.input))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t("Click a variable to insert it at the cursor or replace the selected text.")),
            )
            .child(variables)
            .when(audio, |editor| {
                editor.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t("Dimensions are unavailable for audio.")),
                )
            })
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t("Example: {filename}-{profile}.{ext}. The output extension is added automatically if missing.")),
            )
    }
}
