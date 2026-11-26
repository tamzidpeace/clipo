use std::fs;
use std::path::PathBuf;

use clipo_lib::models::{
    AppSettings, CapturedPayload, ClipoError, IgnoreRuleType, ItemKind, SnippetInput, SortBy,
};
use clipo_lib::storage::blobs::resolve_blob_path;
use clipo_lib::storage::StorageEngine;
use image::{ImageBuffer, ImageFormat, Rgba};
use uuid::Uuid;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("clipo-storage-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temp test dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn text_payload(text: &str, html: Option<&str>, rtf: Option<&str>) -> CapturedPayload {
    CapturedPayload {
        kind: ItemKind::Text,
        plain_text: Some(text.to_string()),
        rtf_payload: rtf.map(str::to_string),
        html_payload: html.map(str::to_string),
        image_bytes: None,
        image_width: None,
        image_height: None,
        file_paths: Vec::new(),
        source_app_name: Some("Notes".to_string()),
        source_app_id: Some("com.apple.Notes".to_string()),
        pasteboard_types: vec!["public.utf8-plain-text".to_string()],
        is_concealed_or_transient: false,
    }
}

fn make_png_bytes(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        Rgba([
            seed.wrapping_add((x % 255) as u8),
            seed.wrapping_mul(3).wrapping_add((y % 255) as u8),
            seed.wrapping_add(42),
            255,
        ])
    });
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("encode test png");
    bytes
}

fn image_payload(bytes: Vec<u8>, width: u32, height: u32) -> CapturedPayload {
    CapturedPayload {
        kind: ItemKind::Image,
        plain_text: None,
        rtf_payload: None,
        html_payload: None,
        image_bytes: Some(bytes),
        image_width: Some(width),
        image_height: Some(height),
        file_paths: Vec::new(),
        source_app_name: Some("Preview".to_string()),
        source_app_id: Some("com.apple.Preview".to_string()),
        pasteboard_types: vec!["public.png".to_string()],
        is_concealed_or_transient: false,
    }
}

#[test]
fn test_dedup_updates_rich_payload_and_copy_count() {
    let dir = TempDir::new();
    let engine = StorageEngine::open(&dir.path).expect("open storage");

    let first = engine
        .insert_or_bump_capture(
            text_payload("Hello", Some("<b>Hello</b>"), None),
            200,
            SortBy::LastCopied,
        )
        .expect("insert first Hello");
    assert_eq!(first.copy_count, 1);
    assert_eq!(first.html_payload.as_deref(), Some("<b>Hello</b>"));

    let second = engine
        .insert_or_bump_capture(
            text_payload("Hello", Some("<i>Hello</i>"), Some("{\\rtf1 Hello}")),
            200,
            SortBy::LastCopied,
        )
        .expect("insert duplicate Hello");

    let items = engine
        .list_history(SortBy::LastCopied, 50)
        .expect("list history");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, first.id);
    assert_eq!(second.id, first.id);
    assert_eq!(items[0].copy_count, 2);
    assert_eq!(items[0].html_payload.as_deref(), Some("<i>Hello</i>"));
    assert_eq!(items[0].rtf_payload.as_deref(), Some("{\\rtf1 Hello}"));
    assert!(items[0].last_copied_at >= items[0].first_copied_at);
}

#[test]
fn test_pruning_keeps_pinned_items_and_cleans_unreferenced_blobs() {
    let dir = TempDir::new();
    let engine = StorageEngine::open(&dir.path).expect("open storage");

    let png_shared = make_png_bytes(160, 120, 10);
    let png_oldest_unpinned = make_png_bytes(200, 100, 50);
    let png_newer_unpinned = make_png_bytes(120, 120, 90);

    // Item 1: pinned, shares blob with item 4
    let pinned = engine
        .insert_or_bump_capture(image_payload(png_shared.clone(), 160, 120), 2, SortBy::LastCopied)
        .expect("insert pinned candidate");
    let pinned = engine
        .toggle_pin(pinned.id, Some("Alt+1".to_string()))
        .expect("pin item 1");
    assert!(pinned.is_pinned);
    let pinned_blob = pinned.blob_path.clone().expect("pinned blob_path");
    let pinned_thumb = pinned.thumb_path.clone().expect("pinned thumb_path");
    assert_eq!(pinned.image_width, Some(160));
    assert_eq!(pinned.image_height, Some(120));

    // Verify thumbnail dimensions preserve aspect ratio within 80px max height
    let thumb_disk_path = resolve_blob_path(engine.blobs_dir(), &pinned_thumb).unwrap();
    let thumb_dims = image::image_dimensions(&thumb_disk_path).expect("read thumbnail dimensions");
    assert!(thumb_dims.1 <= 80);
    assert_eq!(thumb_dims.1, 80);

    // Item 2: oldest unpinned item (will be pruned when unpinned count hits 3 with max_items=2)
    let oldest = engine
        .insert_or_bump_capture(image_payload(png_oldest_unpinned, 200, 100), 2, SortBy::LastCopied)
        .expect("insert oldest unpinned");
    let oldest_blob = oldest.blob_path.clone().expect("oldest blob");
    let oldest_thumb = oldest.thumb_path.clone().expect("oldest thumb");

    // Item 3: second unpinned item (survives)
    let second = engine
        .insert_or_bump_capture(image_payload(png_newer_unpinned, 120, 120), 2, SortBy::LastCopied)
        .expect("insert second unpinned");

    // Item 4: third unpinned item, distinct content -> triggers prune of item 2
    let png_newest = make_png_bytes(100, 100, 150);
    let newest = engine
        .insert_or_bump_capture(image_payload(png_newest, 100, 100), 2, SortBy::LastCopied)
        .expect("insert third unpinned");

    let items = engine
        .list_history(SortBy::LastCopied, 50)
        .expect("list history after prune");
    assert_eq!(items.len(), 3); // 1 pinned + 2 unpinned
    let ids: Vec<i64> = items.iter().map(|i| i.id).collect();
    assert!(ids.contains(&pinned.id));
    assert!(ids.contains(&second.id));
    assert!(ids.contains(&newest.id));
    assert!(!ids.contains(&oldest.id));

    // Oldest unpinned blob & thumbnail should be removed from disk
    assert!(!resolve_blob_path(engine.blobs_dir(), &oldest_blob)
        .unwrap()
        .exists());
    assert!(!resolve_blob_path(engine.blobs_dir(), &oldest_thumb)
        .unwrap()
        .exists());

    // Pinned item's blob & thumbnail must remain on disk
    assert!(resolve_blob_path(engine.blobs_dir(), &pinned_blob)
        .unwrap()
        .exists());
    assert!(resolve_blob_path(engine.blobs_dir(), &pinned_thumb)
        .unwrap()
        .exists());

    // Verify shared content-addressed blob stays alive when one item referencing it is deleted
    // (create an unpinned text item >64KB, duplicate? Wait: dedup merges identical (kind, content_hash),
    // whereas two items can share a rich_blob_path if two distinct texts have the exact same >64KB HTML!)
    let big_html = format!("<div>{}</div>", "x".repeat(70_000));
    let item_a = engine
        .insert_or_bump_capture(
            text_payload("Text A", Some(&big_html), None),
            10,
            SortBy::LastCopied,
        )
        .expect("insert Text A with shared big html");
    let item_b = engine
        .insert_or_bump_capture(
            text_payload("Text B", Some(&big_html), None),
            10,
            SortBy::LastCopied,
        )
        .expect("insert Text B with shared big html");
    let shared_rich_blob = item_a
        .rich_blob_path
        .clone()
        .expect("item_a rich_blob_path");
    assert_eq!(
        item_b.rich_blob_path.as_deref(),
        Some(shared_rich_blob.as_str())
    );

    engine.delete_item(item_a.id).expect("delete item_a");
    assert!(
        resolve_blob_path(engine.blobs_dir(), &shared_rich_blob)
            .unwrap()
            .exists(),
        "shared rich blob must remain while item_b still references it"
    );

    engine.delete_item(item_b.id).expect("delete item_b");
    assert!(
        !resolve_blob_path(engine.blobs_dir(), &shared_rich_blob)
            .unwrap()
            .exists(),
        "shared rich blob must be removed once last reference is deleted"
    );
}

#[test]
fn test_clear_unpinned_preserves_pins_and_snippets() {
    let dir = TempDir::new();
    let engine = StorageEngine::open(&dir.path).expect("open storage");

    let pinned_item = engine
        .insert_or_bump_capture(
            text_payload("Keep me", None, None),
            200,
            SortBy::LastCopied,
        )
        .expect("insert pinned candidate");
    let pinned_item = engine
        .toggle_pin(pinned_item.id, None)
        .expect("pin item");

    let unpinned_item = engine
        .insert_or_bump_capture(
            text_payload("Delete me", None, None),
            200,
            SortBy::LastCopied,
        )
        .expect("insert unpinned item");

    let snippet = engine
        .upsert_snippet(
            None,
            SnippetInput {
                title: "Git branch".to_string(),
                content: "git checkout -b {{date}}".to_string(),
                tags: vec!["git".to_string()],
                language: Some("bash".to_string()),
                shortcut_key: Some("Alt+G".to_string()),
                is_pinned: true,
            },
        )
        .expect("create snippet");

    let rule = engine
        .upsert_ignore_rule(None, IgnoreRuleType::App, "com.1password.1password".to_string(), true)
        .expect("create ignore rule");

    let mut settings = AppSettings::default();
    settings.storage.max_items = 150;
    engine.save_settings(&settings).expect("save settings");

    engine
        .clear_unpinned_history()
        .expect("clear unpinned history");

    let items = engine
        .list_history(SortBy::LastCopied, 50)
        .expect("list history");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, pinned_item.id);
    assert!(engine.get_history_item(unpinned_item.id).is_err());

    let snippets = engine.list_snippets().expect("list snippets");
    assert_eq!(snippets.len(), 1);
    assert_eq!(snippets[0].id, snippet.id);

    let rules = engine.list_ignore_rules().expect("list rules");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, rule.id);

    let loaded = engine.load_settings().expect("load settings");
    assert_eq!(loaded.storage.max_items, 150);
}

#[test]
fn test_missing_blob_file_handled_gracefully_on_read_and_prune() {
    let dir = TempDir::new();
    let engine = StorageEngine::open(&dir.path).expect("open storage");

    let png_a = make_png_bytes(90, 90, 21);
    let item_a = engine
        .insert_or_bump_capture(image_payload(png_a, 90, 90), 1, SortBy::LastCopied)
        .expect("insert image a");

    let blob_rel = item_a.blob_path.clone().expect("blob_path");
    let blob_abs = resolve_blob_path(engine.blobs_dir(), &blob_rel).unwrap();
    fs::remove_file(&blob_abs).expect("remove blob directly from disk");

    // Reading payload when blob file is gone should return a typed error without panic
    let read_err = engine
        .read_item_payload(item_a.id)
        .expect_err("missing blob should return typed error");
    assert!(
        matches!(read_err, ClipoError::NotFound(_) | ClipoError::Storage(_)),
        "expected NotFound or Storage error, got: {read_err:?}"
    );

    // Pruning item_a when its blob is already missing must succeed cleanly without NotFound abort
    let png_b = make_png_bytes(90, 90, 77);
    let item_b = engine
        .insert_or_bump_capture(image_payload(png_b, 90, 90), 1, SortBy::LastCopied)
        .expect("insert image b triggers prune of item_a");

    let items = engine
        .list_history(SortBy::LastCopied, 10)
        .expect("list history");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, item_b.id);
}

#[test]
fn test_large_text_offloaded_to_blob_above_64kb() {
    let dir = TempDir::new();
    let engine = StorageEngine::open(&dir.path).expect("open storage");

    let large_text = "Clipo-UTF8-বাংলা-Line\n".repeat(3_500); // > 80KB
    assert!(large_text.len() > 65_536);

    let item = engine
        .insert_or_bump_capture(
            text_payload(&large_text, None, None),
            200,
            SortBy::LastCopied,
        )
        .expect("insert large text");

    assert!(item.full_text.is_none());
    assert!(item.blob_path.is_some());
    assert!(item
        .plain_text
        .as_deref()
        .is_some_and(|p| p.starts_with("Clipo-UTF8-বাংলা-Line") && p.len() <= 65_536));
    assert!(!item.title_preview.is_empty());

    let resolved = engine
        .read_item_payload(item.id)
        .expect("read back full large text");
    assert_eq!(resolved.full_text.as_deref(), Some(large_text.as_str()));
}

#[test]
fn test_snippets_ignore_rules_files_and_stats() {
    let dir = TempDir::new();
    let engine = StorageEngine::open(&dir.path).expect("open storage");

    let file_item = engine
        .insert_or_bump_capture(
            CapturedPayload {
                kind: ItemKind::File,
                plain_text: None,
                rtf_payload: None,
                html_payload: None,
                image_bytes: None,
                image_width: None,
                image_height: None,
                file_paths: vec![
                    "/tmp/alpha.txt".to_string(),
                    "/tmp/beta.png".to_string(),
                ],
                source_app_name: Some("Finder".to_string()),
                source_app_id: Some("com.apple.finder".to_string()),
                pasteboard_types: vec!["public.file-url".to_string()],
                is_concealed_or_transient: false,
            },
            200,
            SortBy::LastCopied,
        )
        .expect("insert file item");
    assert_eq!(file_item.title_preview, "alpha.txt (+1 more)");

    let resolved_file = engine
        .read_item_payload(file_item.id)
        .expect("read file item payload");
    assert_eq!(
        resolved_file.file_paths,
        vec!["/tmp/alpha.txt".to_string(), "/tmp/beta.png".to_string()]
    );

    let snippet = engine
        .upsert_snippet(
            None,
            SnippetInput {
                title: "Greeting".to_string(),
                content: "Hello {{clipboard}}".to_string(),
                tags: vec!["general".to_string()],
                language: None,
                shortcut_key: None,
                is_pinned: false,
            },
        )
        .expect("create snippet");
    let updated = engine
        .upsert_snippet(
            Some(snippet.id),
            SnippetInput {
                title: "Greeting v2".to_string(),
                content: "Hi {{clipboard}}".to_string(),
                tags: vec!["general".to_string(), "work".to_string()],
                language: Some("markdown".to_string()),
                shortcut_key: Some("Alt+H".to_string()),
                is_pinned: true,
            },
        )
        .expect("update snippet");
    assert_eq!(updated.title, "Greeting v2");
    assert!(updated.is_pinned);

    let used = engine
        .increment_snippet_use(snippet.id)
        .expect("increment snippet use");
    assert_eq!(used.use_count, 1);

    let rule = engine
        .upsert_ignore_rule(None, IgnoreRuleType::Regex, "^secret-".to_string(), true)
        .expect("create ignore rule");
    assert!(rule.enabled);

    let stats = engine.storage_stats().expect("storage stats");
    assert_eq!(stats.total_items, 1);
    assert_eq!(stats.unpinned_items, 1);
    assert_eq!(stats.pinned_items, 0);
    assert_eq!(stats.snippet_count, 1);
    assert!(stats.db_bytes > 0);
    assert_eq!(stats.total_bytes, stats.db_bytes + stats.blob_bytes);

    engine.delete_snippet(snippet.id).expect("delete snippet");
    engine.delete_ignore_rule(rule.id).expect("delete rule");
    assert!(engine.list_snippets().expect("list snippets").is_empty());
    assert!(engine.list_ignore_rules().expect("list rules").is_empty());
}
