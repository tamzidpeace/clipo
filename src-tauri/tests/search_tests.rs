use clipo_lib::models::{
    FilterTab, HistoryItem, ItemKind, MatchRange, PinPosition, SearchMode, SearchRowKind, Snippet,
};
use clipo_lib::search::SearchEngine;

fn make_history_item(
    id: i64,
    kind: ItemKind,
    title_preview: &str,
    full_text: Option<&str>,
    is_pinned: bool,
) -> HistoryItem {
    HistoryItem {
        id,
        kind,
        content_hash: format!("hash-{id}"),
        title_preview: title_preview.to_string(),
        full_text: full_text.map(str::to_string),
        plain_text: full_text.map(str::to_string),
        rtf_payload: None,
        html_payload: None,
        rich_blob_path: None,
        blob_path: if kind == ItemKind::Image {
            Some(format!("blobs/img-{id}.png"))
        } else {
            None
        },
        thumb_path: if kind == ItemKind::Image {
            Some(format!("blobs/thumb-{id}.png"))
        } else {
            None
        },
        image_width: if kind == ItemKind::Image {
            Some(800)
        } else {
            None
        },
        image_height: if kind == ItemKind::Image {
            Some(600)
        } else {
            None
        },
        byte_size: title_preview.len() as i64,
        source_app_name: Some("Terminal".to_string()),
        source_app_id: Some("com.apple.Terminal".to_string()),
        first_copied_at: 1_700_000_000_000 + id * 1000,
        last_copied_at: 1_700_000_000_000 + id * 1000,
        copy_count: 1,
        is_pinned,
        pin_shortcut: None,
        pinned_at: if is_pinned {
            Some(1_700_000_000_000 + id * 1000)
        } else {
            None
        },
    }
}

fn make_snippet(
    id: i64,
    title: &str,
    content: &str,
    tags: &[&str],
    is_pinned: bool,
    use_count: i64,
) -> Snippet {
    Snippet {
        id,
        title: title.to_string(),
        content: content.to_string(),
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        language: Some("bash".to_string()),
        shortcut_key: None,
        is_pinned,
        use_count,
        created_at: 1_700_000_000_000 + id * 1000,
        updated_at: 1_700_000_000_000 + id * 1000,
    }
}

fn slice_utf16(text: &str, range: MatchRange) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    String::from_utf16(&units[range.start..range.end]).expect("valid UTF-16 slice without surrogate split")
}

#[test]
fn test_all_tab_empty_query_hides_snippets_and_orders_pins() {
    let history = vec![
        make_history_item(1, ItemKind::Text, "Pinned Alpha", Some("Pinned Alpha"), true),
        make_history_item(2, ItemKind::Text, "Pinned Beta", Some("Pinned Beta"), true),
        make_history_item(3, ItemKind::Text, "Recent One", Some("Recent One"), false),
        make_history_item(4, ItemKind::Text, "Recent Two", Some("Recent Two"), false),
    ];
    let snippets = vec![
        make_snippet(10, "Git status", "git status -sb", &["git"], true, 5),
        make_snippet(11, "Docker ps", "docker ps -a", &["docker"], false, 2),
    ];

    let top_res = SearchEngine::query(
        &history,
        &snippets,
        "",
        SearchMode::Fuzzy,
        FilterTab::All,
        PinPosition::Top,
    );
    assert_eq!(top_res.regex_error, None);
    assert_eq!(top_res.total_history_count, 4);
    assert_eq!(top_res.total_snippet_count, 2);
    assert_eq!(top_res.rows.len(), 4);
    assert!(top_res
        .rows
        .iter()
        .all(|r| r.row_kind == SearchRowKind::History));
    let top_ids: Vec<i64> = top_res
        .rows
        .iter()
        .map(|r| r.history_item.as_ref().unwrap().id)
        .collect();
    assert_eq!(top_ids, vec![1, 2, 3, 4]);

    let bottom_res = SearchEngine::query(
        &history,
        &snippets,
        "   ",
        SearchMode::Fuzzy,
        FilterTab::All,
        PinPosition::Bottom,
    );
    assert_eq!(bottom_res.rows.len(), 4);
    assert!(bottom_res
        .rows
        .iter()
        .all(|r| r.row_kind == SearchRowKind::History));
    let bottom_ids: Vec<i64> = bottom_res
        .rows
        .iter()
        .map(|r| r.history_item.as_ref().unwrap().id)
        .collect();
    assert_eq!(bottom_ids, vec![3, 4, 1, 2]);
}

#[test]
fn test_all_tab_nonempty_query_includes_matching_snippets() {
    let history = vec![
        make_history_item(1, ItemKind::Text, "git fetch origin", Some("git fetch origin"), true),
        make_history_item(2, ItemKind::Text, "cargo test", Some("cargo test"), false),
        make_history_item(3, ItemKind::Text, "git log --oneline", Some("git log --oneline"), false),
    ];
    let snippets = vec![
        make_snippet(10, "Git new branch", "git checkout -b {{branch}}", &["git"], false, 3),
        make_snippet(11, "Commit helper", "git commit -m \"feat: \"", &["git"], false, 1),
        make_snippet(12, "Unrelated SQL", "SELECT 1;", &["sql"], false, 10),
    ];

    let res = SearchEngine::query(
        &history,
        &snippets,
        "git",
        SearchMode::Exact,
        FilterTab::All,
        PinPosition::Top,
    );
    assert_eq!(res.regex_error, None);

    // Order for PinPosition::Top: matching pinned -> matching snippets -> matching unpinned
    let summary: Vec<(SearchRowKind, i64)> = res
        .rows
        .iter()
        .map(|r| match r.row_kind {
            SearchRowKind::History => (SearchRowKind::History, r.history_item.as_ref().unwrap().id),
            SearchRowKind::Snippet => (SearchRowKind::Snippet, r.snippet.as_ref().unwrap().id),
        })
        .collect();

    assert_eq!(summary[0], (SearchRowKind::History, 1));
    assert_eq!(summary[1].0, SearchRowKind::Snippet);
    assert_eq!(summary[2].0, SearchRowKind::Snippet);
    assert_eq!(summary[3], (SearchRowKind::History, 3));
    assert_eq!(summary.len(), 4);

    // Verify match ranges on title highlight "git"/"Git"
    assert_eq!(res.rows[1].snippet.as_ref().unwrap().id, 10);
    assert_eq!(res.rows[2].snippet.as_ref().unwrap().id, 11);
    assert_eq!(slice_utf16("git fetch origin", res.rows[0].match_ranges[0]), "git");
    assert_eq!(slice_utf16("Git new branch", res.rows[1].match_ranges[0]), "Git");
    assert!(res.rows[2].match_ranges.is_empty());
    assert_eq!(slice_utf16("git log --oneline", res.rows[3].match_ranges[0]), "git");

    // Verify PinPosition::Bottom keeps snippets grouped above unpinned history, pinned last
    let bottom_res = SearchEngine::query(
        &history,
        &snippets,
        "git",
        SearchMode::Exact,
        FilterTab::All,
        PinPosition::Bottom,
    );
    let bottom_kinds: Vec<SearchRowKind> = bottom_res.rows.iter().map(|r| r.row_kind).collect();
    assert_eq!(
        bottom_kinds,
        vec![
            SearchRowKind::Snippet,
            SearchRowKind::Snippet,
            SearchRowKind::History, // unpinned id 3
            SearchRowKind::History, // pinned id 1
        ]
    );
    assert_eq!(bottom_res.rows[2].history_item.as_ref().unwrap().id, 3);
    assert_eq!(bottom_res.rows[3].history_item.as_ref().unwrap().id, 1);
}

#[test]
fn test_quick_pick_indices_skip_pinned_and_snippets() {
    let mut history = vec![
        make_history_item(101, ItemKind::Text, "Pinned item 1", Some("Pinned item 1"), true),
        make_history_item(102, ItemKind::Text, "Pinned item 2", Some("Pinned item 2"), true),
    ];
    for i in 1..=12 {
        history.push(make_history_item(
            i,
            ItemKind::Text,
            &format!("Item match {i}"),
            Some(&format!("Item match {i}")),
            false,
        ));
    }
    let snippets = vec![
        make_snippet(201, "Item snippet A", "echo item", &["item"], true, 4),
        make_snippet(202, "Item snippet B", "echo item 2", &["item"], false, 1),
    ];

    for pin_pos in [PinPosition::Top, PinPosition::Bottom] {
        // Empty query (only history rows)
        let empty_res = SearchEngine::query(
            &history,
            &snippets,
            "",
            SearchMode::Fuzzy,
            FilterTab::All,
            pin_pos,
        );
        let mut unpinned_picks = Vec::new();
        for row in &empty_res.rows {
            if row.row_kind == SearchRowKind::Snippet || row.history_item.as_ref().unwrap().is_pinned {
                assert_eq!(row.quick_pick_index, None);
            } else if let Some(idx) = row.quick_pick_index {
                unpinned_picks.push(idx);
            }
        }
        assert_eq!(unpinned_picks, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);

        // Non-empty query (includes pinned, snippets, and 12 unpinned)
        let query_res = SearchEngine::query(
            &history,
            &snippets,
            "Item",
            SearchMode::Exact,
            FilterTab::All,
            pin_pos,
        );
        let mut query_picks = Vec::new();
        for row in &query_res.rows {
            if row.row_kind == SearchRowKind::Snippet || row.history_item.as_ref().unwrap().is_pinned {
                assert_eq!(
                    row.quick_pick_index, None,
                    "pinned or snippet must never get quick_pick_index"
                );
            } else if let Some(idx) = row.quick_pick_index {
                query_picks.push(idx);
            }
        }
        assert_eq!(query_picks, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}

#[test]
fn test_all_four_search_modes_and_unicode() {
    let history = vec![
        make_history_item(1, ItemKind::Text, "https://maccy.app/docs", Some("https://maccy.app/docs"), false),
        make_history_item(2, ItemKind::Text, "http://example.com", Some("http://example.com"), false),
        make_history_item(3, ItemKind::Text, "src-tauri/src/search/mod.rs", Some("src-tauri/src/search/mod.rs"), false),
        make_history_item(4, ItemKind::Text, "🚀 প্রতিদিন কাজ করি", Some("🚀 প্রতিদিন কাজ করি"), false),
        make_history_item(5, ItemKind::Text, "git commit message", Some("git commit message"), false),
    ];

    // 1. Exact
    let exact = SearchEngine::query(
        &history,
        &[],
        "MACCY.APP",
        SearchMode::Exact,
        FilterTab::All,
        PinPosition::Top,
    );
    assert_eq!(exact.rows.len(), 1);
    assert_eq!(exact.rows[0].history_item.as_ref().unwrap().id, 1);
    assert_eq!(
        slice_utf16("https://maccy.app/docs", exact.rows[0].match_ranges[0]),
        "maccy.app"
    );

    // 2. Fuzzy (scattered chars)
    let fuzzy = SearchEngine::query(
        &history,
        &[],
        "stsm",
        SearchMode::Fuzzy,
        FilterTab::All,
        PinPosition::Top,
    );
    assert!(fuzzy
        .rows
        .iter()
        .any(|r| r.history_item.as_ref().unwrap().id == 3));
    let item3_row = fuzzy
        .rows
        .iter()
        .find(|r| r.history_item.as_ref().unwrap().id == 3)
        .unwrap();
    assert!(!item3_row.match_ranges.is_empty());
    for range in &item3_row.match_ranges {
        assert!(range.start < range.end);
        let _ = slice_utf16("src-tauri/src/search/mod.rs", *range);
    }

    // 3. Regex
    let regex_res = SearchEngine::query(
        &history,
        &[],
        "^https://",
        SearchMode::Regex,
        FilterTab::All,
        PinPosition::Top,
    );
    assert_eq!(regex_res.regex_error, None);
    assert_eq!(regex_res.rows.len(), 1);
    assert_eq!(regex_res.rows[0].history_item.as_ref().unwrap().id, 1);
    assert_eq!(
        slice_utf16("https://maccy.app/docs", regex_res.rows[0].match_ranges[0]),
        "https://"
    );

    // 4. Mixed: exact hit ranks higher than fuzzy-only hit
    let mixed_history = vec![
        make_history_item(10, ItemKind::Text, "g i t  c o m m i t", Some("g i t  c o m m i t"), false),
        make_history_item(11, ItemKind::Text, "git commit", Some("git commit"), false),
    ];
    let mixed = SearchEngine::query(
        &mixed_history,
        &[],
        "git",
        SearchMode::Mixed,
        FilterTab::All,
        PinPosition::Top,
    );
    assert_eq!(mixed.rows.len(), 2);
    assert_eq!(mixed.rows[0].history_item.as_ref().unwrap().id, 11);
    assert!(mixed.rows[0].score > mixed.rows[1].score);

    // 5. Bengali + Emoji UTF-16 offset safety across Exact, Fuzzy, Regex, Mixed
    let bn_title = "🚀 প্রতিদিন কাজ করি";
    for mode in [
        SearchMode::Exact,
        SearchMode::Fuzzy,
        SearchMode::Regex,
        SearchMode::Mixed,
    ] {
        let bn_res = SearchEngine::query(
            &history,
            &[],
            "প্রতিদিন",
            mode,
            FilterTab::All,
            PinPosition::Top,
        );
        assert_eq!(bn_res.rows.len(), 1, "mode {mode:?} should match Bengali text");
        let row = &bn_res.rows[0];
        assert_eq!(row.history_item.as_ref().unwrap().id, 4);
        assert!(
            !row.match_ranges.is_empty(),
            "mode {mode:?} should return match_ranges for Bengali"
        );
        let highlighted: String = row
            .match_ranges
            .iter()
            .map(|r| slice_utf16(bn_title, *r))
            .collect();
        assert_eq!(highlighted, "প্রতিদিন", "UTF-16 slice mismatch in {mode:?}");
    }
}

#[test]
fn test_invalid_regex_returns_error_gracefully() {
    let history = vec![make_history_item(1, ItemKind::Text, "hello", Some("hello"), false)];
    let snippets = vec![make_snippet(1, "hello", "echo hello", &[], false, 0)];

    let res = SearchEngine::query(
        &history,
        &snippets,
        "[unclosed",
        SearchMode::Regex,
        FilterTab::All,
        PinPosition::Top,
    );
    assert!(res.rows.is_empty());
    assert!(res.regex_error.is_some());
    assert_eq!(res.total_history_count, 1);
    assert_eq!(res.total_snippet_count, 1);
}

#[test]
fn test_filter_tabs_images_files_and_snippets() {
    let history = vec![
        make_history_item(1, ItemKind::Text, "plain note", Some("plain note"), true),
        make_history_item(2, ItemKind::Image, "screenshot-pinned.png", None, true),
        make_history_item(3, ItemKind::Image, "diagram.png", None, false),
        make_history_item(4, ItemKind::File, "/tmp/report.pdf", Some("/tmp/report.pdf"), false),
        make_history_item(5, ItemKind::File, "/tmp/pinned.txt", Some("/tmp/pinned.txt"), true),
    ];
    let snippets = vec![
        make_snippet(10, "Unpinned snippet", "echo 1", &["ops"], false, 10),
        make_snippet(11, "Pinned snippet", "echo 2", &["ops"], true, 1),
    ];

    // Images tab (empty query)
    let img_res = SearchEngine::query(
        &history,
        &snippets,
        "",
        SearchMode::Fuzzy,
        FilterTab::Images,
        PinPosition::Top,
    );
    let img_ids: Vec<i64> = img_res
        .rows
        .iter()
        .map(|r| r.history_item.as_ref().unwrap().id)
        .collect();
    assert_eq!(img_ids, vec![2, 3]);
    assert_eq!(img_res.rows[0].quick_pick_index, None);
    assert_eq!(img_res.rows[1].quick_pick_index, Some(1));

    // Files tab (PinPosition::Bottom)
    let file_res = SearchEngine::query(
        &history,
        &snippets,
        "",
        SearchMode::Fuzzy,
        FilterTab::Files,
        PinPosition::Bottom,
    );
    let file_ids: Vec<i64> = file_res
        .rows
        .iter()
        .map(|r| r.history_item.as_ref().unwrap().id)
        .collect();
    assert_eq!(file_ids, vec![4, 5]);
    assert_eq!(file_res.rows[0].quick_pick_index, Some(1));
    assert_eq!(file_res.rows[1].quick_pick_index, None);

    // Snippets tab (empty query: pinned first, no history items, no quick_pick_index)
    let snip_res = SearchEngine::query(
        &history,
        &snippets,
        "",
        SearchMode::Fuzzy,
        FilterTab::Snippets,
        PinPosition::Top,
    );
    assert!(snip_res
        .rows
        .iter()
        .all(|r| r.row_kind == SearchRowKind::Snippet && r.quick_pick_index.is_none()));
    let snip_ids: Vec<i64> = snip_res
        .rows
        .iter()
        .map(|r| r.snippet.as_ref().unwrap().id)
        .collect();
    assert_eq!(snip_ids, vec![11, 10]);
}

#[test]
fn test_regex_zero_length_match_does_not_loop_or_emit_empty_ranges() {
    let history = vec![make_history_item(1, ItemKind::Text, "abc", Some("abc"), false)];
    let res = SearchEngine::query(
        &history,
        &[],
        "^",
        SearchMode::Regex,
        FilterTab::All,
        PinPosition::Top,
    );
    assert_eq!(res.regex_error, None);
    assert_eq!(res.rows.len(), 1);
    assert!(res.rows[0].match_ranges.is_empty());
}

#[test]
fn test_history_fallback_matches_full_text_below_title_match() {
    let history = vec![
        make_history_item(
            1,
            ItemKind::Text,
            "Long document introduction...",
            Some("Long document introduction... hidden needle_keyword at the end"),
            false,
        ),
        make_history_item(
            2,
            ItemKind::Text,
            "Contains needle_keyword in title",
            Some("Contains needle_keyword in title"),
            false,
        ),
    ];

    for mode in [
        SearchMode::Exact,
        SearchMode::Fuzzy,
        SearchMode::Regex,
        SearchMode::Mixed,
    ] {
        let res = SearchEngine::query(
            &history,
            &[],
            "needle_keyword",
            mode,
            FilterTab::All,
            PinPosition::Top,
        );
        assert_eq!(res.rows.len(), 2, "both items should match in {mode:?}");
        assert_eq!(res.rows[0].history_item.as_ref().unwrap().id, 2);
        assert!(!res.rows[0].match_ranges.is_empty());
        assert_eq!(res.rows[1].history_item.as_ref().unwrap().id, 1);
        assert!(res.rows[1].match_ranges.is_empty());
    }
}
