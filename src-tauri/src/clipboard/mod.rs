pub mod ignore;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod privacy;
#[cfg(target_os = "windows")]
pub mod windows;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::models::{AppSettings, CapturedPayload, ClipoError, IgnoreRule, ItemKind};
use crate::storage::blobs::sha256_hex;

pub use ignore::{should_ignore_kind, validate_ignore_regex, IgnoreMatcher};
pub use privacy::has_password_manager_marker;

/// Default time window during which Clipo suppresses re-capturing its own clipboard writes.
pub const DEFAULT_SELF_WRITE_TTL: Duration = Duration::from_secs(3);

/// Callback invoked by an OS clipboard watcher whenever a clipboard change is observed.
pub type SnapshotCallback = Arc<dyn Fn(RawClipboardSnapshot) + Send + Sync>;

/// Raw OS clipboard capture prior to privacy filtering, ignore matching, and classification.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawClipboardSnapshot {
    pub plain_text: Option<String>,
    pub rtf_payload: Option<String>,
    pub html_payload: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub file_paths: Vec<String>,
    pub source_app_name: Option<String>,
    pub source_app_id: Option<String>,
    pub pasteboard_types: Vec<String>,
    pub is_concealed_marker: bool,
    pub content_hash: Option<String>,
}

/// Target application captured prior to opening the popup, used to restore focus for auto-paste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppTarget {
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub pid: Option<i32>,
}

/// Normalized clipboard payload ready to be written to the OS clipboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastePayload {
    pub kind: ItemKind,
    pub plain_text: Option<String>,
    pub rtf_payload: Option<String>,
    pub html_payload: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
    pub file_paths: Vec<String>,
}

impl PastePayload {
    /// Computes the canonical SHA-256 content hash matching `StorageEngine`.
    pub fn content_hash(&self) -> String {
        compute_payload_hash(
            self.kind,
            self.plain_text.as_deref(),
            self.image_bytes.as_deref(),
            &self.file_paths,
        )
    }

    /// Computes the SHA-256 hash for plain-text fallback when pasting without formatting.
    pub fn plain_only_hash(&self) -> Option<String> {
        self.effective_plain_text()
            .map(|text| sha256_hex(text.as_bytes()))
    }

    /// Returns plain text or derived text representation (e.g. newline-joined file paths).
    pub fn effective_plain_text(&self) -> Option<String> {
        if let Some(ref text) = self.plain_text {
            return Some(text.clone());
        }
        if !self.file_paths.is_empty() {
            return Some(self.file_paths.join("\n"));
        }
        derive_plain_fallback(self.html_payload.as_deref(), self.rtf_payload.as_deref())
    }
}

/// Platform clipboard and synthetic paste capability status reported to UI and paste engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCapabilities {
    pub os: String,
    pub session_type: String,
    pub can_watch_clipboard: bool,
    pub can_auto_paste: bool,
    pub accessibility_granted: Option<bool>,
    pub notes: Option<String>,
}

/// Lifecycle handle for a background OS clipboard watcher thread.
pub struct ClipboardWatcherHandle {
    stop_flag: Arc<AtomicBool>,
    stop_hook: Option<Arc<dyn Fn() + Send + Sync>>,
    join_handle: Option<JoinHandle<()>>,
}

impl ClipboardWatcherHandle {
    /// Constructs a watcher handle from its stop flag, optional wake hook, and worker thread handle.
    pub fn from_parts(
        stop_flag: Arc<AtomicBool>,
        stop_hook: Option<Arc<dyn Fn() + Send + Sync>>,
        join_handle: Option<JoinHandle<()>>,
    ) -> Self {
        Self {
            stop_flag,
            stop_hook,
            join_handle,
        }
    }

    /// Signals the watcher thread to stop and waits for it to exit cleanly.
    pub fn stop(mut self) {
        self.signal_and_join();
    }

    fn signal_and_join(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(ref hook) = self.stop_hook {
            hook();
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ClipboardWatcherHandle {
    fn drop(&mut self) {
        self.signal_and_join();
    }
}

/// Cross-platform OS clipboard and synthetic input abstraction.
pub trait ClipboardBackend: Send + Sync {
    /// Starts a background clipboard watcher invoking `on_snapshot` on each clipboard change.
    fn start_watcher(
        &self,
        poll_interval: Duration,
        on_snapshot: SnapshotCallback,
    ) -> Result<ClipboardWatcherHandle, ClipoError>;

    /// Writes `payload` to the OS clipboard, stripping rich formatting when `plain_only` is `true`.
    fn write_payload(&self, payload: &PastePayload, plain_only: bool) -> Result<(), ClipoError>;

    /// Clears the current system clipboard contents.
    fn clear_clipboard(&self) -> Result<(), ClipoError>;

    /// Captures the currently focused frontmost application, if determinable.
    fn current_frontmost_app(&self) -> Option<AppTarget>;

    /// Restores focus to `target` prior to dispatching a paste shortcut.
    fn activate_app(&self, target: &AppTarget) -> Result<(), ClipoError>;

    /// Dispatches OS paste shortcut (`Cmd+V` or `Ctrl+V`) and optional left-arrow caret shifts.
    fn send_paste_shortcut(&self, cursor_back_chars: Option<usize>) -> Result<(), ClipoError>;

    /// Reports current OS session capabilities and permissions.
    fn capabilities(&self) -> PlatformCapabilities;
}

/// Thread-safe guard that suppresses re-capturing Clipo's own clipboard writes within a short TTL.
#[derive(Debug)]
pub struct SelfWriteGuard {
    ttl: Duration,
    recent_hashes: Mutex<HashMap<String, Instant>>,
}

impl Default for SelfWriteGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl SelfWriteGuard {
    /// Creates a guard with the default 3-second suppression TTL.
    pub fn new() -> Self {
        Self::with_ttl(DEFAULT_SELF_WRITE_TTL)
    }

    /// Creates a guard with a custom suppression TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            recent_hashes: Mutex::new(HashMap::new()),
        }
    }

    /// Records a SHA-256 content hash for suppression over the configured TTL window.
    pub fn record_write(&self, hash: &str) {
        if hash.is_empty() {
            return;
        }
        let now = Instant::now();
        if let Ok(mut guard) = self.recent_hashes.lock() {
            prune_expired(&mut guard, now, self.ttl);
            guard.insert(hash.to_string(), now);
        }
    }

    /// Records both the primary canonical hash and plain-text fallback hash for `payload`.
    pub fn record_payload(&self, payload: &PastePayload) {
        self.record_write(&payload.content_hash());
        if let Some(plain_hash) = payload.plain_only_hash() {
            self.record_write(&plain_hash);
        }
    }

    /// Returns `true` if `hash` was recently written within the suppression TTL.
    pub fn is_suppressed(&self, hash: &str) -> bool {
        if hash.is_empty() {
            return false;
        }
        let now = Instant::now();
        let Ok(mut guard) = self.recent_hashes.lock() else {
            return false;
        };
        prune_expired(&mut guard, now, self.ttl);
        guard.contains_key(hash)
    }
}

fn prune_expired(entries: &mut HashMap<String, Instant>, now: Instant, ttl: Duration) {
    entries.retain(|_, &mut recorded_at| now.saturating_duration_since(recorded_at) <= ttl);
}

/// Computes the canonical SHA-256 content hash matching `StorageEngine`'s dedup keys.
pub fn compute_payload_hash(
    kind: ItemKind,
    plain_text: Option<&str>,
    image_bytes: Option<&[u8]>,
    file_paths: &[String],
) -> String {
    match kind {
        ItemKind::Text => sha256_hex(plain_text.unwrap_or_default().as_bytes()),
        ItemKind::Image => sha256_hex(image_bytes.unwrap_or_default()),
        ItemKind::File => {
            let mut canonical = file_paths.to_vec();
            canonical.sort();
            sha256_hex(canonical.join("\n").as_bytes())
        }
    }
}

/// Computes the canonical SHA-256 content hash for a normalized `CapturedPayload`.
pub fn compute_capture_hash(payload: &CapturedPayload) -> String {
    compute_payload_hash(
        payload.kind,
        payload.plain_text.as_deref(),
        payload.image_bytes.as_deref(),
        &payload.file_paths,
    )
}

/// Classifies a raw OS snapshot into a `CapturedPayload` according to Clipo's multi-format rules.
///
/// - Non-empty `file_paths` classifies as `ItemKind::File`.
/// - Non-empty `image_bytes` classifies as `ItemKind::Image` when `plain_text` is absent,
///   whitespace, or a single filename/token (even with an `<img>` HTML wrapper), and as
///   `ItemKind::Text` when standalone multi-word prose is present.
/// - Otherwise non-empty text/RTF/HTML classifies as `ItemKind::Text`.
pub fn classify_and_build_capture(raw: RawClipboardSnapshot) -> Option<CapturedPayload> {
    let concealed = raw.is_concealed_marker || has_password_manager_marker(&raw.pasteboard_types);

    if !raw.file_paths.is_empty() {
        return Some(CapturedPayload {
            kind: ItemKind::File,
            plain_text: None,
            rtf_payload: None,
            html_payload: None,
            image_bytes: None,
            image_width: None,
            image_height: None,
            file_paths: raw.file_paths,
            source_app_name: raw.source_app_name,
            source_app_id: raw.source_app_id,
            pasteboard_types: raw.pasteboard_types,
            is_concealed_or_transient: concealed,
        });
    }

    let has_image = raw.image_bytes.as_ref().is_some_and(|b| !b.is_empty());
    if has_image
        && !has_standalone_multiword_text(raw.plain_text.as_deref(), raw.html_payload.as_deref())
    {
        return Some(CapturedPayload {
            kind: ItemKind::Image,
            plain_text: None,
            rtf_payload: None,
            html_payload: None,
            image_bytes: raw.image_bytes,
            image_width: raw.image_width,
            image_height: raw.image_height,
            file_paths: Vec::new(),
            source_app_name: raw.source_app_name,
            source_app_id: raw.source_app_id,
            pasteboard_types: raw.pasteboard_types,
            is_concealed_or_transient: concealed,
        });
    }

    let rtf_payload = raw.rtf_payload.filter(|s| !s.is_empty());
    let html_payload = raw.html_payload.filter(|s| !s.is_empty());
    let plain_text = match raw.plain_text {
        Some(text) if !text.is_empty() => Some(text),
        _ => derive_plain_fallback(html_payload.as_deref(), rtf_payload.as_deref()),
    };

    if plain_text.is_none() && rtf_payload.is_none() && html_payload.is_none() {
        return None;
    }

    Some(CapturedPayload {
        kind: ItemKind::Text,
        plain_text,
        rtf_payload,
        html_payload,
        image_bytes: None,
        image_width: None,
        image_height: None,
        file_paths: Vec::new(),
        source_app_name: raw.source_app_name,
        source_app_id: raw.source_app_id,
        pasteboard_types: raw.pasteboard_types,
        is_concealed_or_transient: concealed,
    })
}

/// Runs the complete capture filter pipeline: pause flag, privacy markers, self-write suppression,
/// ignore rules, storage kind flags, and multi-format classification.
pub fn evaluate_capture(
    raw: RawClipboardSnapshot,
    rules: &[IgnoreRule],
    settings: &AppSettings,
    write_guard: &SelfWriteGuard,
) -> Option<CapturedPayload> {
    if settings.advanced.turn_off {
        return None;
    }

    if raw.is_concealed_marker || has_password_manager_marker(&raw.pasteboard_types) {
        return None;
    }

    if let Some(ref precomputed_hash) = raw.content_hash {
        if write_guard.is_suppressed(precomputed_hash) {
            return None;
        }
    }

    if IgnoreMatcher::should_ignore(
        raw.source_app_id.as_deref(),
        &raw.pasteboard_types,
        raw.plain_text.as_deref(),
        rules,
        settings.advanced.ignore_all_apps_except_listed,
        &settings.storage,
    ) {
        return None;
    }

    let captured = classify_and_build_capture(raw)?;

    if should_ignore_kind(captured.kind, &settings.storage) {
        return None;
    }

    if captured.kind == ItemKind::Text
        && IgnoreMatcher::should_ignore(
            captured.source_app_id.as_deref(),
            &captured.pasteboard_types,
            captured.plain_text.as_deref(),
            rules,
            settings.advanced.ignore_all_apps_except_listed,
            &settings.storage,
        )
    {
        return None;
    }

    let canonical_hash = compute_capture_hash(&captured);
    if write_guard.is_suppressed(&canonical_hash) {
        return None;
    }

    Some(captured)
}

/// Instantiates the active OS clipboard backend for the current target platform.
pub fn create_platform_backend() -> Box<dyn ClipboardBackend> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosClipboardBackend::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsClipboardBackend::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxClipboardBackend::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Box::new(UnsupportedClipboardBackend)
    }
}

fn has_standalone_multiword_text(plain_text: Option<&str>, html_payload: Option<&str>) -> bool {
    let Some(text) = plain_text else {
        if let Some(html) = html_payload {
            let lower = html.to_ascii_lowercase();
            if lower.contains("<img") {
                return false;
            }
            let visible = strip_html_tags(html);
            let trimmed = visible.trim();
            return trimmed.split_whitespace().count() > 1;
        }
        return false;
    };

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    if !trimmed.contains(char::is_whitespace) {
        return false;
    }

    if !trimmed.contains('\n')
        && !trimmed.contains('\t')
        && has_image_extension(trimmed)
        && trimmed.split_whitespace().count() <= 4
        && !trimmed.ends_with(['.', '!', '?'])
    {
        return false;
    }

    true
}

fn has_image_extension(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".tif", ".tiff", ".heic",
        ".avif", ".ico",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

fn derive_plain_fallback(html: Option<&str>, rtf: Option<&str>) -> Option<String> {
    if let Some(html_str) = html.filter(|s| !s.is_empty()) {
        let stripped = strip_html_tags(html_str);
        let trimmed = stripped.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
        return Some(html_str.to_string());
    }

    rtf.filter(|s| !s.is_empty()).map(str::to_string)
}

fn strip_html_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
struct UnsupportedClipboardBackend;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl ClipboardBackend for UnsupportedClipboardBackend {
    fn start_watcher(
        &self,
        _poll_interval: Duration,
        _on_snapshot: SnapshotCallback,
    ) -> Result<ClipboardWatcherHandle, ClipoError> {
        Err(ClipoError::Clipboard(
            "clipboard watcher unsupported on this OS".to_string(),
        ))
    }

    fn write_payload(&self, _payload: &PastePayload, _plain_only: bool) -> Result<(), ClipoError> {
        Err(ClipoError::Clipboard(
            "clipboard write unsupported on this OS".to_string(),
        ))
    }

    fn clear_clipboard(&self) -> Result<(), ClipoError> {
        Err(ClipoError::Clipboard(
            "clipboard clear unsupported on this OS".to_string(),
        ))
    }

    fn current_frontmost_app(&self) -> Option<AppTarget> {
        None
    }

    fn activate_app(&self, _target: &AppTarget) -> Result<(), ClipoError> {
        Ok(())
    }

    fn send_paste_shortcut(&self, _cursor_back_chars: Option<usize>) -> Result<(), ClipoError> {
        Err(ClipoError::Paste(
            "synthetic paste unsupported on this OS".to_string(),
        ))
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            os: std::env::consts::OS.to_string(),
            session_type: "unknown".to_string(),
            can_watch_clipboard: false,
            can_auto_paste: false,
            accessibility_granted: None,
            notes: Some("unsupported target OS".to_string()),
        }
    }
}
