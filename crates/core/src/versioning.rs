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
}
