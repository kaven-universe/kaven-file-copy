use std::path::Path;

use crate::{KfcError, Result};

/// Decides which source entries a copy rule selects.
///
/// A rule copies an entry only when it is explicitly selected: the entry (or
/// one of its ancestors) appears in the rule's `include` list, and no deeper
/// `exclude` override deselects it. An empty `include` list copies nothing, so
/// the UI presents the source tree as "check the files you want to copy".
pub struct InclusionMatcher {
    included_paths: Vec<String>,
    excluded_paths: Vec<String>,
}

impl InclusionMatcher {
    pub fn new(inclusions: &[String], exclusions: &[String]) -> Result<Self> {
        let included_paths = inclusions
            .iter()
            .map(|value| normalize_rule_path(value))
            .collect::<Result<Vec<_>>>()?;
        let excluded_paths = exclusions
            .iter()
            .map(|value| normalize_rule_path(value))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            included_paths,
            excluded_paths,
        })
    }

    pub fn is_included(&self, path: &Path) -> bool {
        let normalized = normalize_relative_path(path);
        self.is_included_normalized(&normalized)
    }

    pub fn should_descend(&self, path: &Path) -> bool {
        let normalized = normalize_relative_path(path);
        if self.included_paths.is_empty() {
            return false;
        }
        if self.is_included_normalized(&normalized) {
            return true;
        }
        // The directory is not selected itself, but a selected entry may live
        // beneath it (for example an override that re-selects a child).
        let prefix = format!("{normalized}/");
        self.included_paths.iter().any(|included| {
            included.starts_with(&prefix)
                && deepest_match(&self.excluded_paths, included)
                    .is_none_or(|excluded_depth| excluded_depth < included.len())
        })
    }

    fn is_included_normalized(&self, normalized: &str) -> bool {
        let included_depth = match deepest_match(&self.included_paths, normalized) {
            Some(depth) => depth,
            None => return false,
        };
        match deepest_match(&self.excluded_paths, normalized) {
            Some(excluded_depth) => excluded_depth < included_depth,
            None => true,
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
            std::path::Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_selection_copies_nothing() {
        let matcher = InclusionMatcher::new(&[], &[]).unwrap();
        assert!(!matcher.is_included(Path::new("cache/a.txt")));
        assert!(!matcher.is_included(Path::new("README.md")));
        assert!(!matcher.should_descend(Path::new("cache")));
    }

    #[test]
    fn selected_path_includes_descendants() {
        let matcher = InclusionMatcher::new(&["cache".into()], &[]).unwrap();
        assert!(matcher.is_included(Path::new("cache")));
        assert!(matcher.is_included(Path::new("cache/nested/a.txt")));
        assert!(!matcher.is_included(Path::new("cache-other/a.txt")));
        assert!(matcher.should_descend(Path::new("cache")));
        assert!(matcher.should_descend(Path::new("cache/nested")));
        assert!(!matcher.should_descend(Path::new("other")));
    }

    #[test]
    fn deselecting_a_child_overrides_its_parent() {
        let matcher = InclusionMatcher::new(&["cache".into()], &["cache/keep".into()]).unwrap();
        assert!(matcher.is_included(Path::new("cache/drop.txt")));
        assert!(!matcher.is_included(Path::new("cache/keep")));
        assert!(!matcher.is_included(Path::new("cache/keep/file.txt")));
        assert!(matcher.should_descend(Path::new("cache")));
        assert!(!matcher.should_descend(Path::new("cache/keep")));
    }

    #[test]
    fn deeper_selection_overrides_deselected_child() {
        let matcher = InclusionMatcher::new(
            &["cache".into(), "cache/keep/private".into()],
            &["cache/keep".into()],
        )
        .unwrap();

        assert!(!matcher.is_included(Path::new("cache/keep/public.txt")));
        assert!(matcher.is_included(Path::new("cache/keep/private/secret.txt")));
        assert!(matcher.should_descend(Path::new("cache/keep")));
    }

    #[test]
    fn escaping_path_is_rejected() {
        assert!(InclusionMatcher::new(&["../secret".into()], &[]).is_err());
        assert!(InclusionMatcher::new(&[], &["../secret".into()]).is_err());
    }

    #[test]
    fn descendant_of_selected_subtree_is_scanned() {
        let matcher = InclusionMatcher::new(&["data".into()], &[]).unwrap();
        assert!(matcher.should_descend(Path::new("data/sub")));
        assert!(matcher.is_included(Path::new("data/sub/file.txt")));
    }
}
