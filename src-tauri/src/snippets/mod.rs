use std::fmt::Write as _;

use chrono::format::{Item, StrftimeItems};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{ClipoError, Snippet, SnippetInput};

/// Current schema version for exported snippet JSON bundles.
pub const SNIPPETS_EXPORT_VERSION: u32 = 1;

/// Execution context supplied when expanding snippet template placeholders.
#[derive(Debug, Clone)]
pub struct TemplateContext {
    pub now: DateTime<Local>,
    pub clipboard_text: Option<String>,
    pub uuid_override: Option<String>,
}

impl Default for TemplateContext {
    fn default() -> Self {
        Self {
            now: Local::now(),
            clipboard_text: None,
            uuid_override: None,
        }
    }
}

/// Output of resolving a snippet template, including optional caret rewind distance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSnippet {
    pub text: String,
    pub cursor_back_chars: Option<usize>,
}

/// Resolves built-in `{{...}}` placeholders in a single non-recursive pass.
///
/// Expanded values (such as `{{clipboard}}`) are never re-scanned for template tags.
/// Unknown tags and invalid `{{date:<fmt>}}` format strings are preserved literally.
pub fn resolve_snippet_template(template: &str, ctx: &TemplateContext) -> ResolvedSnippet {
    let mut out = String::with_capacity(template.len());
    let mut cursor_byte_pos: Option<usize> = None;
    let mut rest = template;

    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];

        // Preserve leading `{` in runs like `{{{date}}}` so the inner `{{date}}` still matches.
        if after_open.starts_with('{') {
            out.push('{');
            rest = &rest[open + 1..];
            continue;
        }

        let Some(close) = after_open.find("}}") else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };

        let token = &after_open[..close];

        // If another `{{` appears before `}}`, treat the earlier `{{...` as literal text
        // and resume scanning from the inner `{{`.
        if let Some(inner_open) = token.find("{{") {
            out.push_str("{{");
            out.push_str(&token[..inner_open]);
            rest = &after_open[inner_open..];
            continue;
        }

        match token {
            "date" => {
                out.push_str(&ctx.now.format("%Y-%m-%d").to_string());
            }
            "time" => {
                out.push_str(&ctx.now.format("%H:%M:%S").to_string());
            }
            "datetime" => {
                out.push_str(&ctx.now.format("%Y-%m-%d %H:%M:%S").to_string());
            }
            "uuid" => {
                if let Some(override_id) = &ctx.uuid_override {
                    out.push_str(override_id);
                } else {
                    out.push_str(&Uuid::new_v4().to_string());
                }
            }
            "clipboard" => {
                if let Some(clip) = &ctx.clipboard_text {
                    out.push_str(clip);
                }
            }
            "cursor" => {
                if cursor_byte_pos.is_none() {
                    cursor_byte_pos = Some(out.len());
                }
            }
            _ => {
                if let Some(fmt) = token.strip_prefix("date:") {
                    if let Some(formatted) = format_date_safely(ctx.now, fmt) {
                        out.push_str(&formatted);
                    } else {
                        push_literal_token(&mut out, token);
                    }
                } else {
                    push_literal_token(&mut out, token);
                }
            }
        }

        rest = &after_open[close + 2..];
    }

    out.push_str(rest);

    let cursor_back_chars = cursor_byte_pos.map(|pos| out[pos..].chars().count());

    ResolvedSnippet {
        text: out,
        cursor_back_chars,
    }
}

fn push_literal_token(out: &mut String, token: &str) {
    out.push_str("{{");
    out.push_str(token);
    out.push_str("}}");
}

fn format_date_safely(now: DateTime<Local>, fmt: &str) -> Option<String> {
    if fmt.is_empty() {
        return None;
    }
    let items: Vec<Item<'_>> = StrftimeItems::new(fmt).collect();
    if items.iter().any(|item| matches!(item, Item::Error)) {
        return None;
    }

    let mut formatted = String::new();
    write!(formatted, "{}", now.format_with_items(items.into_iter())).ok()?;
    Some(formatted)
}

#[derive(Debug, Serialize)]
struct ExportEnvelope<'a> {
    version: u32,
    snippets: Vec<ExportSnippetRecord<'a>>,
}

#[derive(Debug, Serialize)]
struct ExportSnippetRecord<'a> {
    title: &'a str,
    content: &'a str,
    tags: &'a [String],
    language: Option<&'a str>,
    shortcut_key: Option<&'a str>,
    is_pinned: bool,
}

#[derive(Debug, Deserialize)]
struct ImportEnvelope {
    version: Option<u32>,
    snippets: Option<Vec<ImportSnippetRecord>>,
}

#[derive(Debug, Deserialize)]
struct ImportSnippetRecord {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    shortcut_key: Option<String>,
    #[serde(default)]
    is_pinned: bool,
}

/// Serializes snippets into a versioned (`version: 1`) JSON string for backup or sharing.
pub fn export_snippets_json(snippets: &[Snippet]) -> Result<String, ClipoError> {
    let envelope = ExportEnvelope {
        version: SNIPPETS_EXPORT_VERSION,
        snippets: snippets
            .iter()
            .map(|snippet| ExportSnippetRecord {
                title: &snippet.title,
                content: &snippet.content,
                tags: &snippet.tags,
                language: snippet.language.as_deref(),
                shortcut_key: snippet.shortcut_key.as_deref(),
                is_pinned: snippet.is_pinned,
            })
            .collect(),
    };

    Ok(serde_json::to_string_pretty(&envelope)?)
}

/// Parses and validates a versioned snippet JSON bundle into `SnippetInput` entries.
pub fn import_snippets_json(raw_json: &str) -> Result<Vec<SnippetInput>, ClipoError> {
    let envelope: ImportEnvelope = serde_json::from_str(raw_json)
        .map_err(|err| ClipoError::Validation(format!("malformed snippets JSON: {err}")))?;

    match envelope.version {
        Some(SNIPPETS_EXPORT_VERSION) => {}
        Some(other) => {
            return Err(ClipoError::Validation(format!(
                "unsupported snippets export version: {other}"
            )));
        }
        None => {
            return Err(ClipoError::Validation(
                "missing required `version` field in snippets JSON".to_string(),
            ));
        }
    }

    let records = envelope.snippets.ok_or_else(|| {
        ClipoError::Validation("missing required `snippets` array in snippets JSON".to_string())
    })?;

    let mut inputs = Vec::with_capacity(records.len());
    for (idx, record) in records.into_iter().enumerate() {
        let title = record.title.ok_or_else(|| {
            ClipoError::Validation(format!(
                "snippet at index {idx} is missing required field `title`"
            ))
        })?;
        if title.trim().is_empty() {
            return Err(ClipoError::Validation(format!(
                "snippet at index {idx} has empty `title`"
            )));
        }

        let content = record.content.ok_or_else(|| {
            ClipoError::Validation(format!(
                "snippet at index {idx} is missing required field `content`"
            ))
        })?;
        if content.is_empty() {
            return Err(ClipoError::Validation(format!(
                "snippet at index {idx} has empty `content`"
            )));
        }

        inputs.push(SnippetInput {
            title,
            content,
            tags: record.tags,
            language: record.language,
            shortcut_key: record.shortcut_key,
            is_pinned: record.is_pinned,
        });
    }

    Ok(inputs)
}
