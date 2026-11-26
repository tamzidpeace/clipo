use regex::Regex;

use crate::models::{ClipoError, IgnoreRule, IgnoreRuleType, ItemKind, StorageSettings};

/// Evaluates user-configured ignore rules and storage kind toggles against clipboard captures.
pub struct IgnoreMatcher;

/// Validates a regex ignore pattern for use in settings UI and rule persistence.
pub fn validate_ignore_regex(pattern: &str) -> Result<(), ClipoError> {
    if pattern.trim().is_empty() {
        return Err(ClipoError::Validation(
            "ignore regex pattern cannot be empty".to_string(),
        ));
    }

    Regex::new(pattern)
        .map(|_| ())
        .map_err(|err| ClipoError::Validation(format!("invalid ignore regex `{pattern}`: {err}")))
}

/// Returns `true` when `kind` is disabled by `save_flags`.
pub fn should_ignore_kind(kind: ItemKind, save_flags: &StorageSettings) -> bool {
    match kind {
        ItemKind::Text => !save_flags.save_text,
        ItemKind::Image => !save_flags.save_images,
        ItemKind::File => !save_flags.save_files,
    }
}

impl IgnoreMatcher {
    /// Returns `true` if a clipboard snapshot should be skipped before saving.
    ///
    /// - Disabled rules (`!rule.enabled`) are skipped.
    /// - When `allowlist_mode` (`ignore_all_apps_except_listed`) is `true`, only snapshots whose
    ///   `source_app_id` matches at least one enabled `IgnoreRuleType::App` rule are allowed.
    /// - Invalid regex patterns in `IgnoreRuleType::Regex` rules are safely skipped without panicking.
    pub fn should_ignore(
        source_app_id: Option<&str>,
        formats: &[String],
        plain_text: Option<&str>,
        rules: &[IgnoreRule],
        allowlist_mode: bool,
        save_flags: &StorageSettings,
    ) -> bool {
        if should_ignore_by_save_flags(formats, plain_text, save_flags) {
            return true;
        }

        let enabled_app_patterns: Vec<&str> = rules
            .iter()
            .filter(|rule| rule.enabled && rule.rule_type == IgnoreRuleType::App)
            .map(|rule| rule.pattern.trim())
            .filter(|pattern| !pattern.is_empty())
            .collect();

        let normalized_source_app = source_app_id.map(str::trim).filter(|s| !s.is_empty());

        if allowlist_mode {
            let Some(app_id) = normalized_source_app else {
                return true;
            };
            if !enabled_app_patterns
                .iter()
                .any(|pattern| app_pattern_matches(app_id, pattern))
            {
                return true;
            }
        } else if let Some(app_id) = normalized_source_app {
            if enabled_app_patterns
                .iter()
                .any(|pattern| app_pattern_matches(app_id, pattern))
            {
                return true;
            }
        }

        for rule in rules {
            if !rule.enabled || rule.rule_type != IgnoreRuleType::PasteboardType {
                continue;
            }
            let pattern = rule.pattern.trim().to_ascii_lowercase();
            if pattern.is_empty() {
                continue;
            }
            if formats
                .iter()
                .any(|fmt| glob_match_ci(&pattern, &fmt.trim().to_ascii_lowercase()))
            {
                return true;
            }
        }

        if let Some(text) = plain_text {
            for rule in rules {
                if !rule.enabled || rule.rule_type != IgnoreRuleType::Regex {
                    continue;
                }
                if rule.pattern.trim().is_empty() {
                    continue;
                }
                if let Ok(re) = Regex::new(&rule.pattern) {
                    if re.is_match(text) {
                        return true;
                    }
                }
            }
        }

        false
    }
}

fn should_ignore_by_save_flags(
    formats: &[String],
    plain_text: Option<&str>,
    save_flags: &StorageSettings,
) -> bool {
    if !save_flags.save_text && !save_flags.save_images && !save_flags.save_files {
        return true;
    }

    let mut has_file_format = false;
    let mut has_image_format = false;
    let mut has_text_format = false;

    for raw_fmt in formats {
        let fmt = raw_fmt.trim().to_ascii_lowercase();
        if matches!(
            fmt.as_str(),
            "public.file-url"
                | "nsfilenamespboardtype"
                | "cf_hdrop"
                | "text/uri-list"
                | "x-special/gnome-copied-files"
                | "file_list"
        ) {
            has_file_format = true;
        } else if fmt.starts_with("image/")
            || matches!(
                fmt.as_str(),
                "public.png" | "public.tiff" | "public.jpeg" | "cf_dib" | "cf_dibv5" | "png" | "tiff"
            )
        {
            has_image_format = true;
        } else if fmt.starts_with("text/")
            || matches!(
                fmt.as_str(),
                "public.utf8-plain-text"
                    | "public.rtf"
                    | "public.html"
                    | "cf_unicodetext"
                    | "cf_text"
                    | "html format"
                    | "rich text format"
                    | "utf8_string"
                    | "string"
                    | "application/rtf"
            )
        {
            has_text_format = true;
        }
    }

    if has_file_format {
        return !save_flags.save_files;
    }

    if has_image_format {
        if super::has_standalone_multiword_text(plain_text, None) {
            return !save_flags.save_text;
        }
        return !save_flags.save_images;
    }

    if plain_text.is_some() || has_text_format {
        return !save_flags.save_text;
    }

    false
}

fn app_pattern_matches(source_app_id: &str, pattern: &str) -> bool {
    let source = source_app_id.to_ascii_lowercase();
    let needle = pattern.to_ascii_lowercase();
    !needle.is_empty() && (source == needle || source.contains(&needle))
}

fn glob_match_ci(pattern: &str, text: &str) -> bool {
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == text;
    }

    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_idx = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(star) = star_idx {
            pi = star + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len()
}
