use std::cmp::Ordering;

use regex::Regex;

use crate::Result;

/// Default regular expression used to locate a version inside a file name.
/// It captures the first dotted-numeric token such as `1.2.3` or `1.2.3.4`.
pub const DEFAULT_VERSION_PATTERN: &str = r"(\d+\.\d+(?:\.\d+)*)";

/// A parsed, comparable version. Numeric components are compared by value,
/// text components are pre-releases (sort below numbers), and trailing
/// missing numeric components are treated as zero (so `1.0` equals `1.0.0`).
#[derive(Debug, Clone)]
pub struct Version {
    parts: Vec<VersionPart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VersionPart {
    Number(u64),
    Text(String),
}

pub fn parse_version(input: &str) -> Option<Version> {
    let input = input.trim();
    let input = input.strip_prefix(['v', 'V']).unwrap_or(input);
    let mut parts = Vec::new();
    let mut has_number = false;
    for segment in input
        .split(|character: char| character.is_ascii_punctuation() || character.is_whitespace())
        .filter(|segment| !segment.is_empty())
    {
        if let Ok(number) = segment.parse::<u64>() {
            has_number = true;
            parts.push(VersionPart::Number(number));
        } else {
            parts.push(VersionPart::Text(segment.to_ascii_lowercase()));
        }
    }
    if parts.is_empty() || !has_number {
        return None;
    }
    Some(Version { parts })
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let max_length = self.parts.len().max(other.parts.len());
        for index in 0..max_length {
            let left = self
                .parts
                .get(index)
                .cloned()
                .unwrap_or(VersionPart::Number(0));
            let right = other
                .parts
                .get(index)
                .cloned()
                .unwrap_or(VersionPart::Number(0));
            let ordering = match (left, right) {
                (VersionPart::Number(a), VersionPart::Number(b)) => a.cmp(&b),
                (VersionPart::Number(_), VersionPart::Text(_)) => Ordering::Greater,
                (VersionPart::Text(_), VersionPart::Number(_)) => Ordering::Less,
                (VersionPart::Text(a), VersionPart::Text(b)) => a.cmp(&b),
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        Ordering::Equal
    }
}

/// Extracts the version substring from a file name. Uses `pattern` when given,
/// otherwise [`DEFAULT_VERSION_PATTERN`]. The first capture group wins, falling
/// back to the whole match when the pattern has no capture group.
pub fn extract_version(file_name: &str, pattern: Option<&str>) -> Option<String> {
    let pattern = pattern.unwrap_or(DEFAULT_VERSION_PATTERN);
    let re = Regex::new(pattern).ok()?;
    let captures = re.captures(file_name)?;
    let value = captures
        .get(1)
        .or_else(|| captures.get(0))
        .map(|matched| matched.as_str())?;
    if value.is_empty() {
        return None;
    }
    Some(value.to_owned())
}

/// Derives the base name of a versioned file by stripping the version token and
/// any trailing separators, e.g. `80R_12.1.8.72.bin` -> `80R`. Returns `None`
/// when no version can be extracted from the file name.
pub fn base_name_of<'a>(file_name: &'a str, pattern: Option<&str>) -> Option<&'a str> {
    let pattern = pattern.unwrap_or(DEFAULT_VERSION_PATTERN);
    let re = Regex::new(pattern).ok()?;
    let matched = re.find(file_name)?;
    let prefix = &file_name[..matched.start()];
    let base = prefix.trim_end_matches(|c: char| {
        c == '_' || c == '-' || c == '.' || c == ' ' || c == '\t' || c == '('
    });
    if base.is_empty() {
        return None;
    }
    Some(base)
}

/// Returns the extension of a file name without the leading dot (`bin` for
/// `80R_12.1.8.72.bin`), or an empty string when there is none.
pub fn file_extension(file_name: &str) -> &str {
    file_name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("")
}

/// Renders a target-name template by replacing `{name}`, `{version}`, `{ext}`,
/// and numeric capture-group placeholders (`{1}`, `{2}`, ...) with values from
/// the chosen source file. Unknown placeholders are left untouched.
pub fn render_target_template(
    template: &str,
    base: &str,
    version: Option<&str>,
    extension: &str,
    captures: &[String],
) -> String {
    let mut rendered = String::with_capacity(template.len());
    let mut remaining = template;
    while let Some(open) = remaining.find('{') {
        rendered.push_str(&remaining[..open]);
        let after_open = &remaining[open + 1..];
        let Some(close) = after_open.find('}') else {
            rendered.push_str(&remaining[open..]);
            break;
        };
        let name = &after_open[..close];
        let replacement = match name {
            "name" => base.to_owned(),
            "version" => version.unwrap_or("").to_owned(),
            "ext" => extension.to_owned(),
            index
                if !index.is_empty()
                    && index.chars().all(|character| character.is_ascii_digit()) =>
            {
                captures
                    .get(index.parse::<usize>().unwrap_or(1).saturating_sub(1))
                    .cloned()
                    .unwrap_or_default()
            }
            _ => format!("{{{name}}}"),
        };
        rendered.push_str(&replacement);
        remaining = &after_open[close + 1..];
    }
    rendered.push_str(remaining);
    rendered
}

/// Applies a regular expression to a file name and returns its capture groups
/// (excluding the whole match), used to fill `{1}`, `{2}`, ... placeholders.
pub fn capture_groups(pattern: &str, input: &str) -> Option<Vec<String>> {
    let re = Regex::new(pattern).ok()?;
    let captures = re.captures(input)?;
    let groups = (1..captures.len())
        .map(|index| {
            captures
                .get(index)
                .map(|matched| matched.as_str().to_owned())
                .unwrap_or_default()
        })
        .collect();
    Some(groups)
}

/// Escapes glob metacharacters so a literal file name or base can be embedded
/// into a generated glob pattern.
pub fn escape_glob(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '*' | '?' | '[' | ']' | '{' | '}' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// Normalizes a mapping target name into a safe destination-relative path.
/// Accepts `\` or `/` separators and nested names such as `bin/EDM.DSP.dll`.
pub fn normalize_target_name(value: &str) -> Result<String> {
    let value = value.replace('\\', "/");
    let trimmed = value.trim_matches('/');
    if trimmed.is_empty()
        || value.starts_with('/')
        || value.contains(':')
        || trimmed
            .split('/')
            .any(|part| part == ".." || part.is_empty())
    {
        return Err(crate::KfcError::PathEscape(value));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_versions_compare_by_value() {
        assert!(parse_version("1.10.0").unwrap() > parse_version("1.9.9").unwrap());
        assert!(parse_version("2.0.0").unwrap() > parse_version("1.99.99").unwrap());
        assert!(parse_version("1.0.0.1").unwrap() > parse_version("1.0.0").unwrap());
    }

    #[test]
    fn trailing_zero_components_are_equal() {
        assert_eq!(parse_version("1.0"), parse_version("1.0.0"));
        assert_eq!(parse_version("1.0.0"), parse_version("1.0.0.0"));
    }

    #[test]
    fn text_parts_are_pre_releases() {
        assert!(parse_version("1.0.0").unwrap() > parse_version("1.0.0-beta").unwrap());
        assert!(parse_version("1.0.0-beta.2").unwrap() > parse_version("1.0.0-beta.1").unwrap());
        assert!(parse_version("1.0.0-alpha").unwrap() < parse_version("1.0.0-beta").unwrap());
    }

    #[test]
    fn leading_v_prefix_is_ignored() {
        assert_eq!(parse_version("v1.2.3"), parse_version("1.2.3"));
    }

    #[test]
    fn pure_text_input_is_not_a_version() {
        assert_eq!(parse_version("not-a-version"), None);
        assert_eq!(parse_version("beta"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn default_pattern_extracts_dotted_numeric_version() {
        assert_eq!(
            extract_version("EDM.DSP.Core_1.2.3.4.dll", None).as_deref(),
            Some("1.2.3.4")
        );
        assert_eq!(
            extract_version("EDM.DSP.Core-2.0.0-x64.dll", None).as_deref(),
            Some("2.0.0")
        );
        assert_eq!(extract_version("EDM.DSP.Core.dll", None), None);
    }

    #[test]
    fn custom_pattern_controls_version_extraction() {
        let pattern = r"v(\d+\.\d+\.\d+)";
        assert_eq!(
            extract_version("EDM.DSP.Core_v1.2.3.dll", Some(pattern)).as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            extract_version("EDM.DSP.Core_1.2.3.dll", Some(pattern)),
            None
        );
    }

    #[test]
    fn target_name_accepts_nested_relative_paths() {
        assert_eq!(
            normalize_target_name(r"bin\EDM.DSP.Core.dll").unwrap(),
            "bin/EDM.DSP.Core.dll"
        );
        assert_eq!(
            normalize_target_name("EDM.DSP.Core.dll").unwrap(),
            "EDM.DSP.Core.dll"
        );
    }

    #[test]
    fn target_name_rejects_escapes_and_absolute_paths() {
        assert!(normalize_target_name("../secret.dll").is_err());
        assert!(normalize_target_name("a/../b.dll").is_err());
        assert!(normalize_target_name("C:/secret.dll").is_err());
        assert!(normalize_target_name("/etc/passwd").is_err());
        assert!(normalize_target_name("").is_err());
    }

    #[test]
    fn base_name_strips_the_version_token() {
        assert_eq!(base_name_of("80R_12.1.8.72.bin", None), Some("80R"));
        assert_eq!(
            base_name_of("EDM.DSP.Core_1.2.3.dll", None),
            Some("EDM.DSP.Core")
        );
        assert_eq!(
            base_name_of("80SG_12.1.5.12_From main branch.bin", None),
            Some("80SG")
        );
        assert_eq!(
            base_name_of("EDC_STI_Release_12.0.0.32.bin", None),
            Some("EDC_STI_Release")
        );
        assert_eq!(base_name_of("app.dll", None), None);
    }

    #[test]
    fn base_name_respects_a_custom_pattern() {
        let pattern = r"v(\d+\.\d+\.\d+)";
        assert_eq!(
            base_name_of("EDM.DSP.Core_v1.2.3.dll", Some(pattern)),
            Some("EDM.DSP.Core")
        );
        assert_eq!(base_name_of("EDM.DSP.Core_1.2.3.dll", Some(pattern)), None);
    }

    #[test]
    fn file_extension_is_extension_without_dot() {
        assert_eq!(file_extension("80R_12.1.8.72.bin"), "bin");
        assert_eq!(file_extension("archive.tar.gz"), "gz");
        assert_eq!(file_extension("noext"), "");
    }

    #[test]
    fn target_template_renders_placeholders() {
        assert_eq!(
            render_target_template(
                "EDM.BIN0003{name}.dll",
                "80R",
                Some("12.1.8.72"),
                "bin",
                &[]
            ),
            "EDM.BIN000380R.dll"
        );
        assert_eq!(
            render_target_template("{name}.{ext}", "80R", Some("12.1.8.72"), "bin", &[]),
            "80R.bin"
        );
        assert_eq!(
            render_target_template("{name}-{version}.{ext}", "app", None, "dll", &[]),
            "app-.dll"
        );
    }

    #[test]
    fn target_template_renders_capture_groups() {
        let groups = vec!["R".to_owned(), "72".to_owned()];
        assert_eq!(
            render_target_template("EDM.BIN0003{1}.dll", "80R", None, "bin", &groups),
            "EDM.BIN0003R.dll"
        );
        assert_eq!(
            render_target_template("{1}.{2}.{ext}", "80R", None, "bin", &groups),
            "R.72.bin"
        );
        assert_eq!(
            render_target_template("unknown {9}", "80R", None, "bin", &groups),
            "unknown "
        );
    }

    #[test]
    fn capture_groups_extract_rename_parts() {
        let pattern = r"^80([A-Za-z]+)_\d+\.\d+";
        assert_eq!(
            capture_groups(pattern, "80R_12.1.8.72.bin"),
            Some(vec!["R".to_owned()])
        );
        assert_eq!(capture_groups(pattern, "app.dll"), None);
    }

    #[test]
    fn glob_escaping_keeps_literal_characters() {
        assert_eq!(escape_glob("80R"), "80R");
        assert_eq!(escape_glob("app[v2]"), r"app\[v2\]");
        assert_eq!(escape_glob("x*y"), r"x\*y");
    }
}
