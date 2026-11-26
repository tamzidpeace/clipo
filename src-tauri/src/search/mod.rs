use std::cmp::Reverse;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use regex::Regex;

use crate::models::{
    FilterTab, HistoryItem, ItemKind, MatchRange, PinPosition, SearchMode, SearchResultDto,
    SearchRowDto, SearchRowKind, Snippet,
};

const TITLE_BONUS: i64 = 10_000_000;
const TAG_BONUS: i64 = 2_000_000;
const MIXED_EXACT_TIER: i64 = 100_000_000;
const FUZZY_FALLBACK_MAX_BYTES: usize = 64 * 1024;

/// Stateless search and Maccy quick-pick ranking engine over clipboard history and snippets.
pub struct SearchEngine;

impl SearchEngine {
    /// Query `history` and `snippets` using the active `mode`, `filter` tab, and `pin_to` ordering.
    pub fn query(
        history: &[HistoryItem],
        snippets: &[Snippet],
        query: &str,
        mode: SearchMode,
        filter: FilterTab,
        pin_to: PinPosition,
    ) -> SearchResultDto {
        let total_history_count = history.len();
        let total_snippet_count = snippets.len();

        if query.trim().is_empty() {
            let mut rows = build_empty_query_rows(history, snippets, filter, pin_to);
            assign_quick_pick_indices(&mut rows);
            return SearchResultDto {
                tab: filter,
                query: query.to_string(),
                rows,
                total_history_count,
                total_snippet_count,
                regex_error: None,
            };
        }

        let mut plan = match QueryPlan::compile(query, mode) {
            Ok(plan) => plan,
            Err(err) => {
                return SearchResultDto {
                    tab: filter,
                    query: query.to_string(),
                    rows: Vec::new(),
                    total_history_count,
                    total_snippet_count,
                    regex_error: Some(err.to_string()),
                };
            }
        };

        let mut rows = build_matched_rows(history, snippets, filter, pin_to, &mut plan);
        assign_quick_pick_indices(&mut rows);

        SearchResultDto {
            tab: filter,
            query: query.to_string(),
            rows,
            total_history_count,
            total_snippet_count,
            regex_error: None,
        }
    }
}

fn build_empty_query_rows(
    history: &[HistoryItem],
    snippets: &[Snippet],
    filter: FilterTab,
    pin_to: PinPosition,
) -> Vec<SearchRowDto> {
    match filter {
        FilterTab::All => empty_history_rows(history, None, pin_to),
        FilterTab::Images => empty_history_rows(history, Some(ItemKind::Image), pin_to),
        FilterTab::Files => empty_history_rows(history, Some(ItemKind::File), pin_to),
        FilterTab::Snippets => {
            let mut pinned = Vec::new();
            let mut unpinned = Vec::new();
            for snippet in snippets {
                let row = snippet_row(snippet, Vec::new(), 0);
                if snippet.is_pinned {
                    pinned.push(row);
                } else {
                    unpinned.push(row);
                }
            }
            pinned.extend(unpinned);
            pinned
        }
    }
}

fn empty_history_rows(
    history: &[HistoryItem],
    kind_filter: Option<ItemKind>,
    pin_to: PinPosition,
) -> Vec<SearchRowDto> {
    let mut pinned = Vec::new();
    let mut unpinned = Vec::new();

    for item in history {
        if let Some(required_kind) = kind_filter {
            if item.kind != required_kind {
                continue;
            }
        }
        let row = history_row(item, Vec::new(), 0);
        if item.is_pinned {
            pinned.push(row);
        } else {
            unpinned.push(row);
        }
    }

    combine_pinned_and_unpinned(pinned, unpinned, pin_to)
}

fn build_matched_rows(
    history: &[HistoryItem],
    snippets: &[Snippet],
    filter: FilterTab,
    pin_to: PinPosition,
    plan: &mut QueryPlan,
) -> Vec<SearchRowDto> {
    match filter {
        FilterTab::All => {
            let (mut pinned_history, mut unpinned_history) =
                match_history_items(history, None, plan);
            let mut matched_snippets = match_snippets(snippets, plan);

            sort_rows_by_score(&mut pinned_history);
            sort_rows_by_score(&mut matched_snippets);
            sort_rows_by_score(&mut unpinned_history);

            let mut rows = Vec::with_capacity(
                pinned_history.len() + matched_snippets.len() + unpinned_history.len(),
            );
            match pin_to {
                PinPosition::Top => {
                    rows.extend(pinned_history);
                    rows.extend(matched_snippets);
                    rows.extend(unpinned_history);
                }
                PinPosition::Bottom => {
                    rows.extend(matched_snippets);
                    rows.extend(unpinned_history);
                    rows.extend(pinned_history);
                }
            }
            rows
        }
        FilterTab::Snippets => {
            let mut matched_snippets = match_snippets(snippets, plan);
            matched_snippets.sort_by_key(|row| {
                let s = row.snippet.as_ref().expect("snippet row");
                (Reverse(row.score), Reverse(s.is_pinned), Reverse(s.use_count))
            });
            matched_snippets
        }
        FilterTab::Images => {
            let (mut pinned, mut unpinned) =
                match_history_items(history, Some(ItemKind::Image), plan);
            sort_rows_by_score(&mut pinned);
            sort_rows_by_score(&mut unpinned);
            combine_pinned_and_unpinned(pinned, unpinned, pin_to)
        }
        FilterTab::Files => {
            let (mut pinned, mut unpinned) =
                match_history_items(history, Some(ItemKind::File), plan);
            sort_rows_by_score(&mut pinned);
            sort_rows_by_score(&mut unpinned);
            combine_pinned_and_unpinned(pinned, unpinned, pin_to)
        }
    }
}

fn match_history_items(
    history: &[HistoryItem],
    kind_filter: Option<ItemKind>,
    plan: &mut QueryPlan,
) -> (Vec<SearchRowDto>, Vec<SearchRowDto>) {
    let mut pinned = Vec::new();
    let mut unpinned = Vec::new();

    for item in history {
        if let Some(required_kind) = kind_filter {
            if item.kind != required_kind {
                continue;
            }
        }
        if let Some((score, ranges)) = score_history_item(item, plan) {
            let row = history_row(item, ranges, score);
            if item.is_pinned {
                pinned.push(row);
            } else {
                unpinned.push(row);
            }
        }
    }

    (pinned, unpinned)
}

fn match_snippets(snippets: &[Snippet], plan: &mut QueryPlan) -> Vec<SearchRowDto> {
    let mut rows = Vec::new();
    for snippet in snippets {
        if let Some((score, ranges)) = score_snippet(snippet, plan) {
            rows.push(snippet_row(snippet, ranges, score));
        }
    }
    rows
}

fn combine_pinned_and_unpinned(
    mut pinned: Vec<SearchRowDto>,
    mut unpinned: Vec<SearchRowDto>,
    pin_to: PinPosition,
) -> Vec<SearchRowDto> {
    match pin_to {
        PinPosition::Top => {
            pinned.append(&mut unpinned);
            pinned
        }
        PinPosition::Bottom => {
            unpinned.append(&mut pinned);
            unpinned
        }
    }
}

fn sort_rows_by_score(rows: &mut [SearchRowDto]) {
    rows.sort_by_key(|row| Reverse(row.score));
}

/// Assigns `1..=9` quick-pick indices strictly to the first 9 unpinned history rows.
fn assign_quick_pick_indices(rows: &mut [SearchRowDto]) {
    let mut next_pick: u8 = 1;
    for row in rows {
        let is_unpinned_history = row.row_kind == SearchRowKind::History
            && row.history_item.as_ref().is_some_and(|item| !item.is_pinned);

        if is_unpinned_history && next_pick <= 9 {
            row.quick_pick_index = Some(next_pick);
            next_pick += 1;
        } else {
            row.quick_pick_index = None;
        }
    }
}

fn history_row(item: &HistoryItem, match_ranges: Vec<MatchRange>, score: i64) -> SearchRowDto {
    SearchRowDto {
        row_kind: SearchRowKind::History,
        history_item: Some(item.clone()),
        snippet: None,
        match_ranges,
        quick_pick_index: None,
        score,
    }
}

fn snippet_row(snippet: &Snippet, match_ranges: Vec<MatchRange>, score: i64) -> SearchRowDto {
    SearchRowDto {
        row_kind: SearchRowKind::Snippet,
        history_item: None,
        snippet: Some(snippet.clone()),
        match_ranges,
        quick_pick_index: None,
        score,
    }
}

struct ExactNeedle {
    ascii_lower: Option<String>,
    chars_lower: Vec<char>,
}

impl ExactNeedle {
    fn new(query: &str) -> Self {
        let ascii_lower = if query.is_ascii() {
            Some(query.to_ascii_lowercase())
        } else {
            None
        };
        let chars_lower = query.chars().flat_map(char::to_lowercase).collect();
        Self {
            ascii_lower,
            chars_lower,
        }
    }
}

struct FuzzyState {
    pattern: Pattern,
    matcher: Matcher,
    char_buf: Vec<char>,
    indices_buf: Vec<u32>,
}

impl FuzzyState {
    fn new(query: &str) -> Self {
        Self {
            pattern: Pattern::new(
                query,
                CaseMatching::Smart,
                Normalization::Smart,
                AtomKind::Fuzzy,
            ),
            matcher: Matcher::new(Config::DEFAULT),
            char_buf: Vec::new(),
            indices_buf: Vec::new(),
        }
    }
}

enum QueryPlan {
    Exact(ExactNeedle),
    Fuzzy(FuzzyState),
    Regex(Regex),
    Mixed {
        exact: ExactNeedle,
        fuzzy: FuzzyState,
        fallback_regex: Option<Regex>,
    },
}

impl QueryPlan {
    fn compile(query: &str, mode: SearchMode) -> Result<Self, regex::Error> {
        match mode {
            SearchMode::Exact => Ok(Self::Exact(ExactNeedle::new(query))),
            SearchMode::Fuzzy => Ok(Self::Fuzzy(FuzzyState::new(query))),
            SearchMode::Regex => Ok(Self::Regex(Regex::new(query)?)),
            SearchMode::Mixed => {
                let fallback_regex = if looks_like_regex(query) {
                    Regex::new(query).ok()
                } else {
                    None
                };
                Ok(Self::Mixed {
                    exact: ExactNeedle::new(query),
                    fuzzy: FuzzyState::new(query),
                    fallback_regex,
                })
            }
        }
    }
}

fn looks_like_regex(query: &str) -> bool {
    query
        .chars()
        .any(|c| matches!(c, '^' | '$' | '[' | ']' | '(' | ')' | '|' | '*' | '+' | '?' | '\\' | '.'))
}

fn score_history_item(item: &HistoryItem, plan: &mut QueryPlan) -> Option<(i64, Vec<MatchRange>)> {
    let fallback_texts = [item.plain_text.as_deref(), item.full_text.as_deref()];
    score_primary_and_fallbacks(&item.title_preview, &fallback_texts, 0, plan)
}

fn score_snippet(snippet: &Snippet, plan: &mut QueryPlan) -> Option<(i64, Vec<MatchRange>)> {
    // Check title first so we preserve title highlight ranges whenever the title matches.
    if let Some((score, ranges)) = score_single_target(&snippet.title, true, TITLE_BONUS, plan) {
        return Some((score, ranges));
    }

    let mut best_score: Option<i64> = None;
    for tag in &snippet.tags {
        if let Some((score, _)) = score_single_target(tag, false, TAG_BONUS, plan) {
            best_score = Some(best_score.map_or(score, |b| b.max(score)));
        }
    }

    if let Some((score, _)) = score_single_target(&snippet.content, false, 0, plan) {
        best_score = Some(best_score.map_or(score, |b| b.max(score)));
    }

    best_score.map(|score| (score, Vec::new()))
}

fn score_primary_and_fallbacks(
    primary: &str,
    fallbacks: &[Option<&str>],
    fallback_bonus: i64,
    plan: &mut QueryPlan,
) -> Option<(i64, Vec<MatchRange>)> {
    match plan {
        QueryPlan::Mixed {
            exact,
            fuzzy,
            fallback_regex,
        } => {
            // Prioritize any exact hit (title first, then fallbacks) above any fuzzy hit.
            if let Some((s, ranges)) = match_exact(primary, exact, true) {
                return Some((MIXED_EXACT_TIER + TITLE_BONUS + s, ranges));
            }
            for text in fallbacks.iter().flatten() {
                if let Some((s, _)) = match_exact(text, exact, false) {
                    return Some((MIXED_EXACT_TIER + fallback_bonus + s, Vec::new()));
                }
            }
            if let Some((s, ranges)) = match_fuzzy(primary, fuzzy, true) {
                return Some((TITLE_BONUS + s, ranges));
            }
            for text in fallbacks.iter().flatten() {
                let bounded = truncate_char_boundary(text, FUZZY_FALLBACK_MAX_BYTES);
                if let Some((s, _)) = match_fuzzy(bounded, fuzzy, false) {
                    return Some((fallback_bonus + s, Vec::new()));
                }
            }
            if let Some(re) = fallback_regex {
                if let Some((s, ranges)) = match_regex(primary, re, true) {
                    return Some(((s / 100).max(1) + TITLE_BONUS / 10, ranges));
                }
                for text in fallbacks.iter().flatten() {
                    if let Some((s, _)) = match_regex(text, re, false) {
                        return Some(((s / 100).max(1), Vec::new()));
                    }
                }
            }
            None
        }
        _ => {
            if let Some((s, ranges)) = score_single_target(primary, true, TITLE_BONUS, plan) {
                return Some((s, ranges));
            }
            for text in fallbacks.iter().flatten() {
                if let Some((s, _)) = score_single_target(text, false, fallback_bonus, plan) {
                    return Some((s, Vec::new()));
                }
            }
            None
        }
    }
}

fn score_single_target(
    text: &str,
    with_ranges: bool,
    bonus: i64,
    plan: &mut QueryPlan,
) -> Option<(i64, Vec<MatchRange>)> {
    match plan {
        QueryPlan::Exact(exact) => match_exact(text, exact, with_ranges).map(|(s, r)| (s + bonus, r)),
        QueryPlan::Fuzzy(fuzzy) => {
            let target = if with_ranges {
                text
            } else {
                truncate_char_boundary(text, FUZZY_FALLBACK_MAX_BYTES)
            };
            match_fuzzy(target, fuzzy, with_ranges).map(|(s, r)| (s + bonus, r))
        }
        QueryPlan::Regex(re) => match_regex(text, re, with_ranges).map(|(s, r)| (s + bonus, r)),
        QueryPlan::Mixed {
            exact,
            fuzzy,
            fallback_regex,
        } => {
            if let Some((s, r)) = match_exact(text, exact, with_ranges) {
                return Some((MIXED_EXACT_TIER + bonus + s, r));
            }
            let target = if with_ranges {
                text
            } else {
                truncate_char_boundary(text, FUZZY_FALLBACK_MAX_BYTES)
            };
            if let Some((s, r)) = match_fuzzy(target, fuzzy, with_ranges) {
                return Some((bonus + s, r));
            }
            if let Some(re) = fallback_regex {
                if let Some((s, r)) = match_regex(text, re, with_ranges) {
                    return Some(((s / 100).max(1) + bonus / 10, r));
                }
            }
            None
        }
    }
}

fn truncate_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn match_exact(
    haystack: &str,
    needle: &ExactNeedle,
    with_ranges: bool,
) -> Option<(i64, Vec<MatchRange>)> {
    if haystack.is_ascii() {
        if let Some(needle_ascii) = needle.ascii_lower.as_deref() {
            let hay_lower = haystack.to_ascii_lowercase();
            if needle_ascii.is_empty() || hay_lower.len() < needle_ascii.len() {
                return None;
            }
            let first = hay_lower.find(needle_ascii)?;
            let mut score = 100_000_i64 - (first.min(50_000) as i64);
            if hay_lower.len() == needle_ascii.len() {
                score += 1_000_000;
            } else if first == 0 {
                score += 500_000;
            }
            score += 5_000_i64.saturating_sub(hay_lower.len() as i64);

            if !with_ranges {
                return Some((score, Vec::new()));
            }

            let mut ranges = Vec::new();
            let mut offset = first;
            while let Some(pos) = hay_lower[offset..].find(needle_ascii) {
                let start = offset + pos;
                let end = start + needle_ascii.len();
                ranges.push(MatchRange { start, end });
                offset = end;
            }
            return Some((score, merge_ranges(ranges)));
        }
    }

    if needle.chars_lower.is_empty() {
        return None;
    }

    let mut mapped: Vec<(char, usize, usize)> = Vec::with_capacity(haystack.len());
    for (byte_start, ch) in haystack.char_indices() {
        let byte_end = byte_start + ch.len_utf8();
        for lc in ch.to_lowercase() {
            mapped.push((lc, byte_start, byte_end));
        }
    }

    let m = needle.chars_lower.len();
    if mapped.len() < m {
        return None;
    }

    let mut byte_ranges = Vec::new();
    let mut i = 0;
    while i + m <= mapped.len() {
        let matched = mapped[i..i + m]
            .iter()
            .zip(needle.chars_lower.iter())
            .all(|((hc, _, _), nc)| hc == nc);
        if matched {
            byte_ranges.push((mapped[i].1, mapped[i + m - 1].2));
            if !with_ranges {
                break;
            }
            i += m;
        } else {
            i += 1;
        }
    }

    if byte_ranges.is_empty() {
        return None;
    }

    let first_byte_start = byte_ranges[0].0;
    let is_full_match = first_byte_start == 0 && byte_ranges[0].1 == haystack.len();
    let mut score = 100_000_i64 - (first_byte_start.min(50_000) as i64);
    if is_full_match {
        score += 1_000_000;
    } else if first_byte_start == 0 {
        score += 500_000;
    }
    score += 5_000_i64.saturating_sub(mapped.len() as i64);

    if !with_ranges {
        return Some((score, Vec::new()));
    }

    let ranges = byte_ranges_to_utf16(haystack, &byte_ranges);
    Some((score, merge_ranges(ranges)))
}

fn match_fuzzy(
    haystack: &str,
    fuzzy: &mut FuzzyState,
    with_ranges: bool,
) -> Option<(i64, Vec<MatchRange>)> {
    if haystack.is_empty() {
        return None;
    }

    let utf32 = Utf32Str::new(haystack, &mut fuzzy.char_buf);
    if !with_ranges {
        let score = fuzzy.pattern.score(utf32, &mut fuzzy.matcher)?;
        return Some((score as i64, Vec::new()));
    }

    fuzzy.indices_buf.clear();
    let score = fuzzy
        .pattern
        .indices(utf32, &mut fuzzy.matcher, &mut fuzzy.indices_buf)?;
    fuzzy.indices_buf.sort_unstable();
    fuzzy.indices_buf.dedup();

    let ranges = if utf32.is_ascii() {
        fuzzy
            .indices_buf
            .iter()
            .map(|&idx| MatchRange {
                start: idx as usize,
                end: idx as usize + 1,
            })
            .collect()
    } else {
        let spans = grapheme_utf16_spans(haystack);
        fuzzy
            .indices_buf
            .iter()
            .filter_map(|&idx| spans.get(idx as usize).copied())
            .map(|(start, end)| MatchRange { start, end })
            .collect()
    };

    Some((score as i64, merge_ranges(ranges)))
}

fn match_regex(
    haystack: &str,
    re: &Regex,
    with_ranges: bool,
) -> Option<(i64, Vec<MatchRange>)> {
    if !with_ranges {
        return re.is_match(haystack).then_some((100_000, Vec::new()));
    }

    let mut matches = re.find_iter(haystack).peekable();
    let first = matches.peek()?;
    let first_start = first.start();

    let mut byte_ranges = Vec::new();
    let mut match_count: i64 = 0;
    for m in matches {
        match_count += 1;
        if m.start() < m.end() {
            byte_ranges.push((m.start(), m.end()));
        }
    }

    let score = 100_000_i64 + (match_count * 100) - (first_start.min(50_000) as i64);
    let ranges = byte_ranges_to_utf16(haystack, &byte_ranges);
    Some((score, merge_ranges(ranges)))
}

/// Converts sorted, valid UTF-8 byte `(start, end)` ranges into UTF-16 code unit offsets.
fn byte_ranges_to_utf16(text: &str, byte_ranges: &[(usize, usize)]) -> Vec<MatchRange> {
    if byte_ranges.is_empty() {
        return Vec::new();
    }
    if text.is_ascii() {
        return byte_ranges
            .iter()
            .map(|&(start, end)| MatchRange { start, end })
            .collect();
    }

    // Collect every distinct byte boundary needed so we can resolve them in a single pass.
    let mut boundaries: Vec<usize> = Vec::with_capacity(byte_ranges.len() * 2);
    for &(start, end) in byte_ranges {
        boundaries.push(start);
        boundaries.push(end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut utf16_map: Vec<(usize, usize)> = Vec::with_capacity(boundaries.len());
    let mut next_b = 0;
    let mut utf16_pos = 0usize;

    for (byte_pos, ch) in text.char_indices() {
        while next_b < boundaries.len() && boundaries[next_b] == byte_pos {
            utf16_map.push((byte_pos, utf16_pos));
            next_b += 1;
        }
        utf16_pos += ch.len_utf16();
        if next_b >= boundaries.len() {
            break;
        }
    }
    while next_b < boundaries.len() && boundaries[next_b] == text.len() {
        utf16_map.push((text.len(), utf16_pos));
        next_b += 1;
    }

    let lookup = |byte_offset: usize| -> usize {
        utf16_map
            .binary_search_by_key(&byte_offset, |&(b, _)| b)
            .map_or(utf16_pos, |idx| utf16_map[idx].1)
    };

    byte_ranges
        .iter()
        .map(|&(b_start, b_end)| MatchRange {
            start: lookup(b_start),
            end: lookup(b_end),
        })
        .collect()
}

/// Computes `(utf16_start, utf16_end)` spans matching `nucleo_matcher::Utf32Str::new` grapheme indices.
fn grapheme_utf16_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut tmp_buf = Vec::new();
    let mut prev_count = 0usize;
    let mut start_utf16 = 0usize;
    let mut current_utf16 = 0usize;

    for (byte_idx, ch) in text.char_indices() {
        let end_byte = byte_idx + ch.len_utf8();
        let new_count = Utf32Str::new(&text[..end_byte], &mut tmp_buf).len();
        if byte_idx > 0 && new_count > prev_count {
            spans.push((start_utf16, current_utf16));
            start_utf16 = current_utf16;
        }
        prev_count = new_count;
        current_utf16 += ch.len_utf16();
    }

    if prev_count > 0 {
        spans.push((start_utf16, current_utf16));
    }

    spans
}

fn merge_ranges(mut ranges: Vec<MatchRange>) -> Vec<MatchRange> {
    ranges.retain(|r| r.start < r.end);
    if ranges.is_empty() {
        return ranges;
    }
    ranges.sort_unstable_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    let mut merged = Vec::with_capacity(ranges.len());
    let mut current = ranges[0];
    for r in ranges.into_iter().skip(1) {
        if r.start <= current.end {
            current.end = current.end.max(r.end);
        } else {
            merged.push(current);
            current = r;
        }
    }
    merged.push(current);
    merged
}
