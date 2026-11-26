use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, Instant};

use tauri::http::{header::CONTENT_TYPE, Response};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, Position, Size,
    WebviewUrl, WebviewWindowBuilder,
};

use crate::models::{ClipoError, PopupPosition};
use crate::storage::blobs::resolve_blob_path;
use crate::AppState;

/// Window label for the pre-warmed popup window.
pub const POPUP_LABEL: &str = "popup";

/// Window label for the lazily created settings/management window.
pub const SETTINGS_LABEL: &str = "settings";

pub const DEFAULT_POPUP_WIDTH: u32 = 440;
pub const DEFAULT_PREVIEW_EXTRA_WIDTH: u32 = 340;
pub const DEFAULT_POPUP_HEIGHT: u32 = 520;
const MENU_ICON_GAP_PX: i32 = 4;

/// Window during which a tray left-click immediately after popup blur will not reopen the popup.
pub const TRAY_BLUR_SUPPRESS_WINDOW: Duration = Duration::from_millis(250);

/// Axis-aligned rectangle in screen pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Anchored popup geometry saved when the popup opens so preview expansion can attach left or right.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopupAnchor {
    pub base_x: i32,
    pub base_y: i32,
    pub work_area: ScreenRect,
    pub scale_factor: f64,
}

/// HTTP-like response produced for `clipoimg://` protocol requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipoImgResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl ClipoImgResponse {
    fn error(status: u16, message: &'static str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: message.as_bytes().to_vec(),
        }
    }

    /// Converts into a Tauri HTTP response for `register_uri_scheme_protocol`.
    pub fn into_http_response(self) -> Response<Vec<u8>> {
        let cache_control = if self.status == 200 {
            "public, max-age=31536000, immutable"
        } else {
            "no-store"
        };
        Response::builder()
            .status(self.status)
            .header(CONTENT_TYPE, self.content_type)
            .header("Cache-Control", cache_control)
            .body(self.body)
            .unwrap_or_else(|_| Response::new(Vec::new()))
    }
}

/// Returns `true` when a tray left-click falls inside the post-blur debounce window.
pub fn should_suppress_tray_reopen(last_blur: Option<Instant>, now: Instant) -> bool {
    last_blur.is_some_and(|blurred_at| now.saturating_duration_since(blurred_at) <= TRAY_BLUR_SUPPRESS_WINDOW)
}

/// Returns the center point `(x, y)` of a `ScreenRect`.
pub fn rect_center(rect: ScreenRect) -> (i32, i32) {
    (
        rect.x.saturating_add((rect.width as i32) / 2),
        rect.y.saturating_add((rect.height as i32) / 2),
    )
}

/// Returns `true` if `point` lies inside `rect` (`[x, x + width) × [y, y + height)`).
pub fn rect_contains_point(rect: ScreenRect, point: (i32, i32)) -> bool {
    let right = rect.x.saturating_add(rect.width as i32);
    let bottom = rect.y.saturating_add(rect.height as i32);
    point.0 >= rect.x && point.0 < right && point.1 >= rect.y && point.1 < bottom
}

/// Selects the monitor index matching `mode` (`MenuIcon` -> tray center, `ActiveWindow` -> window center,
/// otherwise cursor), falling back to the cursor monitor if the preferred anchor is absent or offscreen.
pub fn select_monitor_index(
    monitors: &[ScreenRect],
    mode: PopupPosition,
    cursor: (i32, i32),
    tray_rect: Option<ScreenRect>,
    active_win: Option<ScreenRect>,
) -> Option<usize> {
    let preferred_point = match mode {
        PopupPosition::MenuIcon => tray_rect.map(rect_center),
        PopupPosition::ActiveWindow => active_win.map(rect_center),
        PopupPosition::Cursor | PopupPosition::ScreenCenter => None,
    };

    if let Some(point) = preferred_point {
        if let Some(idx) = monitors.iter().position(|&m| rect_contains_point(m, point)) {
            return Some(idx);
        }
    }

    monitors.iter().position(|&m| rect_contains_point(m, cursor))
}

/// Resolves effective physical popup dimensions, falling back to scaled defaults when unmeasured or zero-sized.
pub fn resolve_popup_size(measured: Option<(u32, u32)>, scale_factor: f64) -> (u32, u32) {
    if let Some((w, h)) = measured.filter(|&(w, h)| w > 0 && h > 0) {
        return (w, h);
    }
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    (
        (f64::from(DEFAULT_POPUP_WIDTH) * scale).round().max(1.0) as u32,
        (f64::from(DEFAULT_POPUP_HEIGHT) * scale).round().max(1.0) as u32,
    )
}

/// Computes target `(x, y)` coordinates for the popup window and clamps strictly inside `monitor_work_area`.
pub fn compute_popup_position(
    mode: PopupPosition,
    cursor: (i32, i32),
    tray_rect: Option<ScreenRect>,
    active_win: Option<ScreenRect>,
    monitor_work_area: ScreenRect,
    popup_size: (u32, u32),
) -> (i32, i32) {
    let centered_x =
        monitor_work_area.x + (monitor_work_area.width.saturating_sub(popup_size.0) as i32) / 2;
    let centered_y =
        monitor_work_area.y + (monitor_work_area.height.saturating_sub(popup_size.1) as i32) / 2;

    let (raw_x, raw_y) = match mode {
        PopupPosition::Cursor => cursor,
        PopupPosition::MenuIcon => match tray_rect {
            Some(tray) => {
                let x = tray.x + (tray.width as i32 / 2) - (popup_size.0 as i32 / 2);
                let work_mid_y = monitor_work_area.y + (monitor_work_area.height as i32 / 2);
                let y = if tray.y <= work_mid_y {
                    tray.y.saturating_add(tray.height as i32).saturating_add(MENU_ICON_GAP_PX)
                } else {
                    tray.y
                        .saturating_sub(popup_size.1 as i32)
                        .saturating_sub(MENU_ICON_GAP_PX)
                };
                (x, y)
            }
            None => cursor,
        },
        PopupPosition::ScreenCenter => (centered_x, centered_y),
        PopupPosition::ActiveWindow => match active_win {
            Some(win) => {
                let x = win.x + ((win.width as i32) - (popup_size.0 as i32)) / 2;
                let y = win.y + ((win.height as i32) - (popup_size.1 as i32)) / 2;
                (x, y)
            }
            None => (centered_x, centered_y),
        },
    };

    let max_x = monitor_work_area.x + monitor_work_area.width.saturating_sub(popup_size.0) as i32;
    let max_y = monitor_work_area.y + monitor_work_area.height.saturating_sub(popup_size.1) as i32;

    (
        raw_x.clamp(monitor_work_area.x, max_x),
        raw_y.clamp(monitor_work_area.y, max_y),
    )
}

/// Computes target `(x, y, side)` for expanding or collapsing the preview pane sidecar.
pub fn compute_preview_window_layout(
    base_x: i32,
    base_y: i32,
    work_area: ScreenRect,
    base_width_px: i32,
    extra_width_px: i32,
    open: bool,
) -> (i32, i32, &'static str) {
    if !open {
        return (base_x, base_y, "right");
    }

    let total_width = base_width_px.saturating_add(extra_width_px);
    let work_right = work_area.x.saturating_add(work_area.width as i32);
    let max_x = work_area.x.max(work_right.saturating_sub(total_width));

    let right_end = base_x.saturating_add(total_width);
    if right_end <= work_right {
        return (base_x.clamp(work_area.x, max_x), base_y, "right");
    }

    let raw_left_x = base_x.saturating_sub(extra_width_px);
    let right_overflow = right_end.saturating_sub(work_right);
    let left_underflow = work_area.x.saturating_sub(raw_left_x).max(0);

    if right_overflow < left_underflow {
        (base_x.clamp(work_area.x, max_x), base_y, "right")
    } else {
        (raw_left_x.clamp(work_area.x, max_x), base_y, "left")
    }
}

/// Resolves a `clipoimg://` (or `http://clipoimg.localhost/`) URI exclusively within `blobs_dir`.
pub fn resolve_clipoimg_blob(blobs_dir: &Path, request_uri: &str) -> ClipoImgResponse {
    let without_fragment = request_uri.split('#').next().unwrap_or(request_uri);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);

    let Some(raw_rel) = extract_clipoimg_rel(without_query) else {
        return ClipoImgResponse::error(400, "invalid clipoimg URI");
    };

    let Ok(decoded_rel) = percent_decode_utf8(raw_rel) else {
        return ClipoImgResponse::error(400, "invalid percent-encoding in clipoimg URI");
    };

    if decoded_rel.is_empty() {
        return ClipoImgResponse::error(400, "missing blob path");
    }

    let resolved_path = match resolve_blob_path(blobs_dir, &decoded_rel) {
        Ok(path) => path,
        Err(_) => return ClipoImgResponse::error(403, "forbidden blob path"),
    };

    match fs::read(&resolved_path) {
        Ok(bytes) => ClipoImgResponse {
            status: 200,
            content_type: mime_for_path(&resolved_path),
            body: bytes,
        },
        Err(err) if err.kind() == ErrorKind::NotFound => {
            ClipoImgResponse::error(404, "blob not found")
        }
        Err(err) if err.kind() == ErrorKind::PermissionDenied => {
            ClipoImgResponse::error(403, "blob unreadable")
        }
        Err(_) => ClipoImgResponse::error(500, "failed to read blob"),
    }
}

fn extract_clipoimg_rel(uri_no_query: &str) -> Option<&str> {
    if let Some(rest) = strip_prefix_ascii_ci(uri_no_query, "clipoimg://localhost/") {
        return Some(rest);
    }
    if eq_ascii_ci(uri_no_query, "clipoimg://localhost") {
        return Some("");
    }
    if let Some(rest) = strip_prefix_ascii_ci(uri_no_query, "clipoimg://") {
        return Some(rest);
    }
    if let Some(rest) = strip_prefix_ascii_ci(uri_no_query, "http://clipoimg.localhost/") {
        return Some(rest);
    }
    if let Some(rest) = strip_prefix_ascii_ci(uri_no_query, "https://clipoimg.localhost/") {
        return Some(rest);
    }
    if eq_ascii_ci(uri_no_query, "http://clipoimg.localhost")
        || eq_ascii_ci(uri_no_query, "https://clipoimg.localhost")
    {
        return Some("");
    }
    None
}

fn strip_prefix_ascii_ci<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    if value.len() < prefix.len() || !value.is_char_boundary(prefix.len()) {
        return None;
    }
    let (head, tail) = value.split_at(prefix.len());
    head.eq_ignore_ascii_case(prefix).then_some(tail)
}

fn eq_ascii_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn percent_decode_utf8(input: &str) -> Result<String, ()> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0usize;
    while idx < bytes.len() {
        if bytes[idx] == b'%' {
            if idx + 2 >= bytes.len() {
                return Err(());
            }
            let hi = hex_nibble(bytes[idx + 1]).ok_or(())?;
            let lo = hex_nibble(bytes[idx + 2]).ok_or(())?;
            out.push((hi << 4) | lo);
            idx += 3;
        } else {
            out.push(bytes[idx]);
            idx += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn mime_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);

    match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

/// Toggles the pre-warmed popup window visibility.
pub fn toggle_popup(app: &AppHandle) -> Result<(), ClipoError> {
    let Some(popup) = app.get_webview_window(POPUP_LABEL) else {
        return Err(ClipoError::NotFound("popup window not found".to_string()));
    };

    let visible = popup
        .is_visible()
        .map_err(|e| ClipoError::Internal(format!("failed to query popup visibility: {e}")))?;

    if visible {
        hide_popup(app)
    } else {
        show_popup(app)
    }
}

/// Captures the frontmost app, positions the popup inside the active monitor work area, and shows it.
pub fn show_popup(app: &AppHandle) -> Result<(), ClipoError> {
    let Some(popup) = app.get_webview_window(POPUP_LABEL) else {
        return Err(ClipoError::NotFound("popup window not found".to_string()));
    };

    let state = app.state::<AppState>();
    if let Ok(mut guard) = state.last_popup_blur_at.lock() {
        *guard = None;
    }

    // Record the previous foreground app & active window bounds BEFORE focusing Clipo's popup.
    if let Some(frontmost) = state.backend.current_frontmost_app() {
        let self_pid = std::process::id() as i32;
        let is_self = frontmost.pid == Some(self_pid)
            || frontmost
                .app_id
                .as_deref()
                .is_some_and(|id| id == "app.clipo.desktop");
        if !is_self {
            if let Ok(mut guard) = state.prev_app.lock() {
                *guard = Some(frontmost);
            }
        }
    }
    let active_win = state
        .backend
        .active_window_rect()
        .map(|(x, y, width, height)| ScreenRect {
            x,
            y,
            width,
            height,
        });

    let settings = state.storage.load_settings().unwrap_or_default();

    let cursor = app
        .cursor_position()
        .map(|pos| (pos.x.round() as i32, pos.y.round() as i32))
        .unwrap_or((0, 0));

    let raw_tray_rect = app
        .tray_by_id(crate::tray::MAIN_TRAY_ID)
        .and_then(|tray| tray.rect().ok().flatten());

    let monitor = resolve_target_monitor(
        &popup,
        settings.appearance.popup_at,
        cursor,
        raw_tray_rect.as_ref(),
        active_win,
    );
    let scale_factor = monitor.as_ref().map_or(1.0, Monitor::scale_factor);
    let work_area = monitor
        .as_ref()
        .map(monitor_work_area_rect)
        .unwrap_or(ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });

    let _ = popup.set_size(Size::Logical(LogicalSize::new(
        f64::from(DEFAULT_POPUP_WIDTH),
        f64::from(DEFAULT_POPUP_HEIGHT),
    )));
    let popup_size = resolve_popup_size(None, scale_factor);

    let tray_rect = raw_tray_rect.map(|rect| tauri_rect_to_screen_rect(rect, scale_factor));

    let (x, y) = compute_popup_position(
        settings.appearance.popup_at,
        cursor,
        tray_rect,
        active_win,
        work_area,
        popup_size,
    );

    if let Ok(mut guard) = state.popup_anchor.lock() {
        *guard = Some(PopupAnchor {
            base_x: x,
            base_y: y,
            work_area,
            scale_factor,
        });
    }

    let _ = popup.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    let _ = app.emit("clipo://popup-opened", ());
    popup
        .show()
        .map_err(|e| ClipoError::Internal(format!("failed to show popup: {e}")))?;
    let _ = popup.set_focus();

    Ok(())
}

/// Hides the pre-warmed popup window without destroying it and notifies the UI to reset state.
pub fn hide_popup(app: &AppHandle) -> Result<(), ClipoError> {
    if let Ok(mut guard) = app.state::<AppState>().popup_anchor.lock() {
        *guard = None;
    }
    if let Some(popup) = app.get_webview_window(POPUP_LABEL) {
        let _ = popup.hide();
        let _ = popup.set_size(Size::Logical(LogicalSize::new(
            f64::from(DEFAULT_POPUP_WIDTH),
            f64::from(DEFAULT_POPUP_HEIGHT),
        )));
    }
    let _ = app.emit("clipo://popup-reset", ());
    Ok(())
}

/// Expands or collapses the popup window for the Preview Pane sidecar and returns the attached side (`"left"` or `"right"`).
pub fn set_preview_open(app: &AppHandle, open: bool) -> Result<&'static str, ClipoError> {
    let Some(popup) = app.get_webview_window(POPUP_LABEL) else {
        return Err(ClipoError::NotFound("popup window not found".to_string()));
    };

    let target_width = if open {
        f64::from(DEFAULT_POPUP_WIDTH + DEFAULT_PREVIEW_EXTRA_WIDTH)
    } else {
        f64::from(DEFAULT_POPUP_WIDTH)
    };

    let anchor = app
        .state::<AppState>()
        .popup_anchor
        .lock()
        .ok()
        .and_then(|guard| *guard);

    let side = if let Some(anchor) = anchor {
        let scale = if anchor.scale_factor.is_finite() && anchor.scale_factor > 0.0 {
            anchor.scale_factor
        } else {
            1.0
        };
        let base_width_px = (f64::from(DEFAULT_POPUP_WIDTH) * scale).round() as i32;
        let extra_width_px = (f64::from(DEFAULT_PREVIEW_EXTRA_WIDTH) * scale).round() as i32;
        let (x, y, side) = compute_preview_window_layout(
            anchor.base_x,
            anchor.base_y,
            anchor.work_area,
            base_width_px,
            extra_width_px,
            open,
        );
        let _ = popup.set_size(Size::Logical(LogicalSize::new(
            target_width,
            f64::from(DEFAULT_POPUP_HEIGHT),
        )));
        let _ = popup.set_position(Position::Physical(PhysicalPosition::new(x, y)));
        side
    } else {
        let _ = popup.set_size(Size::Logical(LogicalSize::new(
            target_width,
            f64::from(DEFAULT_POPUP_HEIGHT),
        )));
        "right"
    };

    Ok(side)
}

/// Handles popup focus loss (`WindowEvent::Focused(false)`): records blur time if visible, hides popup, and emits reset.
pub fn on_popup_focus_lost(app: &AppHandle) {
    let was_visible = app
        .get_webview_window(POPUP_LABEL)
        .and_then(|popup| popup.is_visible().ok())
        .unwrap_or(false);
    if was_visible {
        let state = app.state::<AppState>();
        if let Ok(mut guard) = state.last_popup_blur_at.lock() {
            *guard = Some(Instant::now());
        };
    }
    let _ = hide_popup(app);
}

/// Opens the Settings window lazily, or focuses and switches tabs if it already exists.
pub fn open_settings_window(app: &AppHandle, tab: Option<String>) -> Result<(), ClipoError> {
    open_settings_window_with_action(app, tab, None)
}

/// Opens the Settings window lazily with an optional `action` query parameter for cold startup.
pub fn open_settings_window_with_action(
    app: &AppHandle,
    tab: Option<String>,
    action: Option<String>,
) -> Result<(), ClipoError> {
    let _ = hide_popup(app);

    if let Some(existing) = app.get_webview_window(SETTINGS_LABEL) {
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);

        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        if let Some(tab_name) = tab {
            let _ = existing.emit("clipo://settings-tab", tab_name);
        }
        return Ok(());
    }

    let mut params = Vec::new();
    if let Some(t) = tab.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        params.push(format!("tab={}", encode_query_value(t)));
    }
    if let Some(a) = action.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        params.push(format!("action={}", encode_query_value(a)));
    }
    let url = if params.is_empty() {
        "settings.html".to_string()
    } else {
        format!("settings.html?{}", params.join("&"))
    };

    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);

    let win = match WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App(url.into()))
        .title("Clipo Settings")
        .inner_size(780.0, 580.0)
        .min_inner_size(680.0, 480.0)
        .resizable(true)
        .center()
        .build()
    {
        Ok(win) => win,
        Err(e) => {
            #[cfg(target_os = "macos")]
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            return Err(ClipoError::Internal(format!(
                "failed to create settings window: {e}"
            )));
        }
    };

    let _ = win.show();
    let _ = win.set_focus();

    Ok(())
}

/// Restores accessory dock policy on macOS when the Settings window is destroyed.
pub fn on_settings_destroyed(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

fn resolve_target_monitor(
    popup: &tauri::WebviewWindow,
    mode: PopupPosition,
    cursor: (i32, i32),
    raw_tray_rect: Option<&tauri::Rect>,
    active_win: Option<ScreenRect>,
) -> Option<Monitor> {
    if let Ok(monitors) = popup.available_monitors() {
        let monitor_rects: Vec<ScreenRect> = monitors.iter().map(monitor_bounds_rect).collect();

        if mode == PopupPosition::MenuIcon {
            if let Some(tray) = raw_tray_rect {
                for (idx, m) in monitors.iter().enumerate() {
                    let tray_rect = tauri_rect_to_screen_rect(*tray, m.scale_factor());
                    if rect_contains_point(monitor_rects[idx], rect_center(tray_rect)) {
                        return Some(m.clone());
                    }
                }
            }
        }

        if let Some(idx) = select_monitor_index(&monitor_rects, mode, cursor, None, active_win) {
            return Some(monitors[idx].clone());
        }
    }

    popup
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| popup.primary_monitor().ok().flatten())
}

fn monitor_bounds_rect(monitor: &Monitor) -> ScreenRect {
    let pos = monitor.position();
    let size = monitor.size();
    ScreenRect {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
    }
}

fn monitor_work_area_rect(monitor: &Monitor) -> ScreenRect {
    let area = monitor.work_area();
    ScreenRect {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    }
}

fn tauri_rect_to_screen_rect(rect: tauri::Rect, scale_factor: f64) -> ScreenRect {
    let (x, y) = match rect.position {
        Position::Physical(p) => (p.x, p.y),
        Position::Logical(l) => (
            (l.x * scale_factor).round() as i32,
            (l.y * scale_factor).round() as i32,
        ),
    };
    let (width, height) = match rect.size {
        Size::Physical(s) => (s.width, s.height),
        Size::Logical(s) => (
            (s.width * scale_factor).round().max(0.0) as u32,
            (s.height * scale_factor).round().max(0.0) as u32,
        ),
    };
    ScreenRect {
        x,
        y,
        width,
        height,
    }
}

fn encode_query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}
