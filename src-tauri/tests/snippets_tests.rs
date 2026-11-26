use chrono::{Local, TimeZone};
use clipo_lib::models::{ClipoError, Snippet, SnippetInput};
use clipo_lib::snippets::{
    export_snippets_json, import_snippets_json, resolve_snippet_template, ResolvedSnippet,
    TemplateContext,
};
use uuid::Uuid;

fn fixed_local_datetime() -> chrono::DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 11, 26, 14, 5, 9)
        .single()
        .expect("valid local datetime")
}

fn sample_snippet(id: i64, title: &str, content: &str, is_pinned: bool) -> Snippet {
    Snippet {
        id,
        title: title.to_string(),
        content: content.to_string(),
        tags: vec!["git".to_string(), "workflow".to_string()],
        language: Some("bash".to_string()),
        shortcut_key: Some("Alt+1".to_string()),
        is_pinned,
        use_count: 7,
        created_at: 1_700_000_000_000,
        updated_at: 1_700_000_123_000,
    }
}

#[test]
fn test_resolves_builtin_variables() {
    let now = fixed_local_datetime();
    let deterministic_ctx = TemplateContext {
        now,
        clipboard_text: Some("{{date}} {{cursor}} literal".to_string()),
        uuid_override: Some("11111111-2222-4333-8444-555555555555".to_string()),
    };

    let template = concat!(
        "date={{date}}|",
        "fmt={{date:%Y/%m/%d}}|",
        "time={{time}}|",
        "dt={{datetime}}|",
        "id={{uuid}}|",
        "clip={{clipboard}}|",
        "bad={{date:%Q}}|",
        "unk={{unknown_var}}"
    );

    let resolved = resolve_snippet_template(template, &deterministic_ctx);
    assert_eq!(
        resolved,
        ResolvedSnippet {
            text: concat!(
                "date=2026-11-26|",
                "fmt=2026/11/26|",
                "time=14:05:09|",
                "dt=2026-11-26 14:05:09|",
                "id=11111111-2222-4333-8444-555555555555|",
                "clip={{date}} {{cursor}} literal|",
                "bad={{date:%Q}}|",
                "unk={{unknown_var}}"
            )
            .to_string(),
            cursor_back_chars: None,
        }
    );

    // Without uuid_override, each {{uuid}} expands to a fresh valid v4 UUID.
    let random_ctx = TemplateContext {
        now,
        clipboard_text: None,
        uuid_override: None,
    };
    let uuid_resolved = resolve_snippet_template("{{uuid}}|{{uuid}}|{{clipboard}}", &random_ctx);
    assert_eq!(uuid_resolved.cursor_back_chars, None);
    let parts: Vec<&str> = uuid_resolved.text.split('|').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[2], "");
    let u1 = Uuid::parse_str(parts[0]).expect("first uuid should be valid");
    let u2 = Uuid::parse_str(parts[1]).expect("second uuid should be valid");
    assert_eq!(u1.get_version(), Some(uuid::Version::Random));
    assert_eq!(u2.get_version(), Some(uuid::Version::Random));
    assert_ne!(u1, u2);

    // Unclosed tag and braced wrapper edge cases remain safe.
    let edge = resolve_snippet_template("{{{date}}} {{unclosed", &random_ctx);
    assert_eq!(edge.text, "{2026-11-26} {{unclosed");
    assert_eq!(edge.cursor_back_chars, None);
}

#[test]
fn test_cursor_placeholder_computes_utf8_char_offset() {
    let ctx = TemplateContext {
        now: fixed_local_datetime(),
        clipboard_text: Some("বাংলা".to_string()),
        uuid_override: Some("00000000-0000-4000-8000-000000000000".to_string()),
    };

    let commit = resolve_snippet_template("git commit -m \"{{cursor}}\"", &ctx);
    assert_eq!(commit.text, "git commit -m \"\"");
    assert_eq!(commit.cursor_back_chars, Some(1));

    // "বাংলা🚀" is 6 Unicode scalar values (ব, া, ং, ল, া, 🚀) despite 19 UTF-8 bytes.
    let multibyte = resolve_snippet_template("prefix {{cursor}}বাংলা🚀", &ctx);
    assert_eq!(multibyte.text, "prefix বাংলা🚀");
    assert_eq!(multibyte.cursor_back_chars, Some(6));

    // Only the first {{cursor}} determines caret position; subsequent ones are stripped.
    let multi_cursor = resolve_snippet_template("a{{cursor}}b{{cursor}}c{{clipboard}}", &ctx);
    assert_eq!(multi_cursor.text, "abcবাংলা");
    assert_eq!(multi_cursor.cursor_back_chars, Some(7));

    // Cursor at end of text yields 0 characters back.
    let at_end = resolve_snippet_template("hello {{cursor}}", &ctx);
    assert_eq!(at_end.text, "hello ");
    assert_eq!(at_end.cursor_back_chars, Some(0));
}

#[test]
fn test_snippets_json_export_import_roundtrip_and_invalid_schema() {
    let snippets = vec![
        sample_snippet(1, "Git Commit", "git commit -m \"{{cursor}}\"", true),
        Snippet {
            id: 2,
            title: "Bangla Greeting".to_string(),
            content: "শুভ সকাল {{date}} 🚀".to_string(),
            tags: vec![],
            language: None,
            shortcut_key: None,
            is_pinned: false,
            use_count: 0,
            created_at: 1_700_000_000_000,
            updated_at: 1_700_000_000_000,
        },
    ];

    let exported = export_snippets_json(&snippets).expect("export should succeed");
    let value: serde_json::Value = serde_json::from_str(&exported).expect("valid exported json");
    assert_eq!(value["version"], 1);

    let imported = import_snippets_json(&exported).expect("import should succeed");
    assert_eq!(
        imported,
        vec![
            SnippetInput {
                title: "Git Commit".to_string(),
                content: "git commit -m \"{{cursor}}\"".to_string(),
                tags: vec!["git".to_string(), "workflow".to_string()],
                language: Some("bash".to_string()),
                shortcut_key: Some("Alt+1".to_string()),
                is_pinned: true,
            },
            SnippetInput {
                title: "Bangla Greeting".to_string(),
                content: "শুভ সকাল {{date}} 🚀".to_string(),
                tags: vec![],
                language: None,
                shortcut_key: None,
                is_pinned: false,
            },
        ]
    );

    let invalid_cases = [
        "{not valid json",
        r#"{"version": 2, "snippets": []}"#,
        r#"{"snippets": []}"#,
        r#"{"version": 1, "snippets": [{"title": "   ", "content": "ok"}]}"#,
        r#"{"version": 1, "snippets": [{"content": "missing title"}]}"#,
        r#"{"version": 1, "snippets": [{"title": "Missing content"}]}"#,
        r#"{"version": 1, "snippets": [{"title": "Empty content", "content": ""}]}"#,
    ];

    for bad in invalid_cases {
        let err = import_snippets_json(bad).expect_err("expected validation error");
        assert!(
            matches!(err, ClipoError::Validation(_)),
            "expected ClipoError::Validation for `{bad}`, got {err:?}"
        );
    }
}
