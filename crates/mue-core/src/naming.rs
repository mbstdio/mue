use std::{ffi::OsString, path::Path};

use anyhow::{Context, Result, ensure};

pub(crate) struct Dimensions {
    pub width: u32,
    pub height: u32,
}

enum Part<'a> {
    Literal(&'a str),
    Variable(Variable),
}

#[derive(Clone, Copy)]
enum Variable {
    Filename,
    Extension,
    Profile,
    Width,
    Height,
    SourceWidth,
    SourceHeight,
}

pub(crate) struct FilenameTemplate<'a> {
    parts: Vec<Part<'a>>,
}

impl<'a> FilenameTemplate<'a> {
    pub fn parse(template: &'a str) -> Result<Self> {
        ensure!(
            !template.trim().is_empty(),
            "Output filename template cannot be empty"
        );
        let mut parts = Vec::new();
        let mut remaining = template;
        while let Some(start) = remaining.find('{') {
            let literal = &remaining[..start];
            ensure!(
                !literal.contains('}'),
                "Unmatched brace in output filename template"
            );
            parts.push(Part::Literal(literal));
            let end = remaining[start + 1..]
                .find('}')
                .context("Unmatched brace in output filename template")?
                + start
                + 1;
            let variable = &remaining[start + 1..end];
            let variable = match variable {
                "filename" => Variable::Filename,
                "ext" => Variable::Extension,
                "profile" => Variable::Profile,
                "width" => Variable::Width,
                "height" => Variable::Height,
                "source_width" => Variable::SourceWidth,
                "source_height" => Variable::SourceHeight,
                _ => anyhow::bail!("Unknown output filename variable: {{{variable}}}"),
            };
            parts.push(Part::Variable(variable));
            remaining = &remaining[end + 1..];
        }
        ensure!(
            !remaining.contains('}'),
            "Unmatched brace in output filename template"
        );
        parts.push(Part::Literal(remaining));
        for part in &parts {
            if let Part::Literal(text) = part {
                ensure!(
                    !text.chars().any(invalid_character),
                    "Output filename template cannot contain paths, control characters, or <>:\"/\\|?*"
                );
            }
        }
        Ok(Self { parts })
    }

    pub fn needs_output_dimensions(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, Part::Variable(Variable::Width | Variable::Height)))
    }

    pub fn validate(&self, extension: &str, profile: &str) -> Result<()> {
        // Source-dependent names can only be fully validated with an actual conversion.
        if self.parts.iter().any(|part| {
            matches!(
                part,
                Part::Variable(
                    Variable::Filename
                        | Variable::Width
                        | Variable::Height
                        | Variable::SourceWidth
                        | Variable::SourceHeight
                )
            )
        }) {
            return Ok(());
        }
        self.render(Path::new(""), extension, profile, None, None)?;
        Ok(())
    }

    pub fn render(
        &self,
        source: &Path,
        extension: &str,
        profile: &str,
        source_dimensions: Option<&Dimensions>,
        output_dimensions: Option<&Dimensions>,
    ) -> Result<OsString> {
        let mut name = OsString::new();
        for part in &self.parts {
            match part {
                Part::Literal(text) => name.push(text),
                Part::Variable(variable) => match *variable {
                    Variable::Filename => {
                        name.push(source.file_stem().context("Missing source filename")?)
                    }
                    Variable::Extension => name.push(extension),
                    Variable::Profile => name.push(
                        profile
                            .chars()
                            .map(|c| if invalid_character(c) { '_' } else { c })
                            .collect::<String>(),
                    ),
                    Variable::Width
                    | Variable::Height
                    | Variable::SourceWidth
                    | Variable::SourceHeight => {
                        let dimensions =
                            if matches!(variable, Variable::SourceWidth | Variable::SourceHeight) {
                                source_dimensions
                            } else {
                                output_dimensions
                            }
                            .context("Missing media dimensions for output filename template")?;
                        name.push(
                            if matches!(variable, Variable::Width | Variable::SourceWidth) {
                                dimensions.width
                            } else {
                                dimensions.height
                            }
                            .to_string(),
                        );
                    }
                },
            }
        }
        let suffix = format!(".{extension}");
        ensure!(
            !name.is_empty() && !matches!(name.to_str(), Some("." | "..")),
            "Output filename must have a nonempty stem"
        );
        if !name
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(&suffix)
        {
            name.push(&suffix);
        }
        ensure!(
            !name.to_string_lossy().eq_ignore_ascii_case(&suffix),
            "Output filename must have a nonempty stem"
        );
        validate_filename(&name)?;
        Ok(name)
    }
}

fn invalid_character(character: char) -> bool {
    character.is_control() || "<>:\"/\\|?*".contains(character)
}

pub(crate) fn validate_filename(name: &std::ffi::OsStr) -> Result<()> {
    let text = name.to_string_lossy();
    ensure!(
        !text.chars().any(invalid_character)
            && !text.ends_with(['.', ' '])
            && !matches!(text.as_ref(), "." | "..")
            && text.encode_utf16().count() <= 255,
        "Output filename must be a valid filename of at most 255 characters with a nonempty stem"
    );
    let stem = text.split('.').next().unwrap_or_default().trim_end();
    let upper = stem.to_uppercase();
    let reserved = matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
        .is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        });
    ensure!(
        !reserved,
        "Output filename cannot use a reserved device name"
    );
    Ok(())
}
