use std::path::{Component, Path};

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::{Exclusion, KfcError, Result};

pub struct ExclusionMatcher {
    excluded_paths: Vec<String>,
    included_paths: Vec<String>,
    globs: GlobSet,
}

impl ExclusionMatcher {
    pub fn new(exclusions: &[Exclusion], inclusions: &[String]) -> Result<Self> {
        let mut excluded_paths = Vec::new();
        let mut builder = GlobSetBuilder::new();
        for exclusion in exclusions {
            match exclusion {
                Exclusion::Path { value } => excluded_paths.push(normalize_rule_path(value)?),
                Exclusion::Glob { value } => {
                    let normalized = value.replace('\\', "/");
                    let glob =
                        Glob::new(&normalized).map_err(|error| KfcError::InvalidPattern {
                            pattern: value.clone(),
                            message: error.to_string(),
                        })?;
                    builder.add(glob);
                }
            }
        }
        let included_paths = inclusions
            .iter()
            .map(|value| normalize_rule_path(value))
            .collect::<Result<Vec<_>>>()?;
        let globs = builder.build().map_err(|error| KfcError::InvalidPattern {
            pattern: "<glob set>".into(),
            message: error.to_string(),
        })?;
        Ok(Self {
            excluded_paths,
            included_paths,
            globs,
        })
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        let normalized = normalize_relative_path(path);
        self.is_excluded_normalized(&normalized)
    }

    pub fn should_descend(&self, path: &Path) -> bool {
        let normalized = normalize_relative_path(path);
        if !self.is_excluded_normalized(&normalized) {
            return true;
        }
        let prefix = format!("{normalized}/");
        self.included_paths
            .iter()
            .any(|included| included.starts_with(&prefix))
    }

    fn is_excluded_normalized(&self, normalized: &str) -> bool {
        let excluded_depth = deepest_match(&self.excluded_paths, normalized);
        let included_depth = deepest_match(&self.included_paths, normalized);
        match (excluded_depth, included_depth) {
            (Some(excluded), Some(included)) => excluded > included,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => self.globs.is_match(normalized),
        }
    }
}

fn deepest_match(paths: &[String], target: &str) -> Option<usize> {
    paths
        .iter()
        .filter(|path| {
            target == path.as_str()
                || target
                    .strip_prefix(path.as_str())
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
        .map(String::len)
        .max()
}

fn normalize_rule_path(value: &str) -> Result<String> {
    let value = value.replace('\\', "/");
    let value = value.trim_matches('/');
    if value.is_empty() || value.split('/').any(|part| part == ".." || part.is_empty()) {
        return Err(KfcError::PathEscape(value.into()));
    }
    Ok(value.to_owned())
}

pub fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_exclusion_includes_descendants() {
        let matcher = ExclusionMatcher::new(
            &[Exclusion::Path {
                value: "cache".into(),
            }],
            &[],
        )
        .unwrap();
        assert!(matcher.is_excluded(Path::new("cache")));
        assert!(matcher.is_excluded(Path::new("cache/nested/a.txt")));
        assert!(!matcher.is_excluded(Path::new("cache-other/a.txt")));
    }

    #[test]
    fn glob_exclusion_uses_forward_slashes() {
        let matcher = ExclusionMatcher::new(
            &[Exclusion::Glob {
                value: "**/*.tmp".into(),
            }],
            &[],
        )
        .unwrap();
        assert!(matcher.is_excluded(Path::new("nested/work.tmp")));
        assert!(!matcher.is_excluded(Path::new("nested/work.txt")));
    }

    #[test]
    fn escaping_path_is_rejected() {
        assert!(
            ExclusionMatcher::new(
                &[Exclusion::Path {
                    value: "../secret".into()
                }],
                &[],
            )
            .is_err()
        );
    }

    #[test]
    fn included_child_overrides_excluded_parent() {
        let matcher = ExclusionMatcher::new(
            &[Exclusion::Path {
                value: "cache".into(),
            }],
            &["cache/keep".into()],
        )
        .unwrap();

        assert!(matcher.is_excluded(Path::new("cache/drop.txt")));
        assert!(!matcher.is_excluded(Path::new("cache/keep")));
        assert!(!matcher.is_excluded(Path::new("cache/keep/file.txt")));
        assert!(matcher.should_descend(Path::new("cache")));
    }

    #[test]
    fn deeper_exclusion_overrides_included_child() {
        let matcher = ExclusionMatcher::new(
            &[
                Exclusion::Path {
                    value: "cache".into(),
                },
                Exclusion::Path {
                    value: "cache/keep/private".into(),
                },
            ],
            &["cache/keep".into()],
        )
        .unwrap();

        assert!(!matcher.is_excluded(Path::new("cache/keep/public.txt")));
        assert!(matcher.is_excluded(Path::new("cache/keep/private/secret.txt")));
    }

    #[test]
    fn escaping_inclusion_is_rejected() {
        assert!(ExclusionMatcher::new(&[], &["../secret".into()]).is_err());
    }
}
