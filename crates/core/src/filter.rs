use std::path::{Component, Path};

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::{Exclusion, KfcError, Result};

pub struct ExclusionMatcher {
    paths: Vec<String>,
    globs: GlobSet,
}

impl ExclusionMatcher {
    pub fn new(exclusions: &[Exclusion]) -> Result<Self> {
        let mut paths = Vec::new();
        let mut builder = GlobSetBuilder::new();
        for exclusion in exclusions {
            match exclusion {
                Exclusion::Path { value } => paths.push(normalize_rule_path(value)?),
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
        let globs = builder.build().map_err(|error| KfcError::InvalidPattern {
            pattern: "<glob set>".into(),
            message: error.to_string(),
        })?;
        Ok(Self { paths, globs })
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        let normalized = normalize_relative_path(path);
        self.paths.iter().any(|excluded| {
            normalized == *excluded || normalized.starts_with(&format!("{excluded}/"))
        }) || self.globs.is_match(&normalized)
    }
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
        let matcher = ExclusionMatcher::new(&[Exclusion::Path {
            value: "cache".into(),
        }])
        .unwrap();
        assert!(matcher.is_excluded(Path::new("cache")));
        assert!(matcher.is_excluded(Path::new("cache/nested/a.txt")));
        assert!(!matcher.is_excluded(Path::new("cache-other/a.txt")));
    }

    #[test]
    fn glob_exclusion_uses_forward_slashes() {
        let matcher = ExclusionMatcher::new(&[Exclusion::Glob {
            value: "**/*.tmp".into(),
        }])
        .unwrap();
        assert!(matcher.is_excluded(Path::new("nested/work.tmp")));
        assert!(!matcher.is_excluded(Path::new("nested/work.txt")));
    }

    #[test]
    fn escaping_path_is_rejected() {
        assert!(
            ExclusionMatcher::new(&[Exclusion::Path {
                value: "../secret".into()
            }])
            .is_err()
        );
    }
}
