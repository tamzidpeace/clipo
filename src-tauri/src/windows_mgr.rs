use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use tauri::http::{header::CONTENT_TYPE, Response};
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, Position, Size, WebviewUrl,
    WebviewWindowBuilder,
};

use crate::models::{ClipoError, PopupPosition};
use crate::storage::blobs::resolve_blob_path;
use crate::AppState;

/// Window label for the pre-warmed popup window.
pub const POPUP_LABEL: &str = "popup";

/// Window label for the lazily created settings/management window.
pub const SETTINGS_LABEL: &str = "settings";

const DEFAULT_POPUP_WIDTH: u32 = 440;
const DEFAULT_POPUP_HEIGHT: u32 = 520;
const MENU_ICON_GAP_PX: i32 = 4;

/// Axis-aligned rectangle in screen pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
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
        Response::builder()
            .status(self.status)
            .header(CONTENT_TYPE, self.content_type)
            .header("Cache-Control", "public, max-age=31536000, immutable")
            .body(self.body)
            .unwrap_or_else(|_| Response::new(Vec::new()))
    }
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

    // Record the previous foreground app BEFORE focusing Clipo's popup.
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

    let settings = state.storage.load_settings().unwrap_or_default();

    let cursor = app
        .cursor_position()
        .map(|pos| (pos.x.round() as i32, pos.y.round() as i32))
        .unwrap_or((0, 0));

    let monitor = resolve_target_monitor(&popup, cursor);
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

    let popup_size = popup
        .outer_size()
        .map(|s| (s.width, s.height))
        .unwrap_or_else(|_| {
            (
                (f64::from(DEFAULT_POPUP_WIDTH) * scale_factor).round() as u32,
                (f64::from(DEFAULT_POPUP_HEIGHT) * scale_factor).round() as u32,
            )
        });

    let tray_rect = app
        .tray_by_id(crate::tray::MAIN_TRAY_ID)
        .and_then(|tray| tray.rect().ok().flatten())
        .map(|rect| tauri_rect_to_screen_rect(rect, scale_factor));

    let (x, y) = compute_popup_position(
        settings.appearance.popup_at,
        cursor,
        tray_rect,
        None,
        work_area,
        popup_size,
    );

    let _ = popup.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    popup
        .show()
        .map_err(|e| ClipoError::Internal(format!("failed to show popup: {e}")))?;
    let _ = popup.set_focus();
    let _ = app.emit("clipo://popup-opened", ());

    Ok(())
}

/// Hides the pre-warmed popup window without destroying it and notifies the UI to reset state.
pub fn hide_popup(app: &AppHandle) -> Result<(), ClipoError> {
    if let Some(popup) = app.get_webview_window(POPUP_LABEL) {
        let _ = popup.hide();
    }
    let _ = app.emit("clipo://popup-reset", ());
    Ok(())
}

/// Handles popup focus loss (`WindowEvent::Focused(false)`): hides popup and emits reset.
pub fn on_popup_focus_lost(app: &AppHandle) {
    let _ = hide_popup(app);
}

/// Opens the Settings window lazily, or focuses and switches tabs if it already exists.
pub fn open_settings_window(app: &AppHandle, tab: Option<String>) -> Result<(), ClipoError> {
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

    let url = match tab.as_deref() {
        Some(t) if !t.trim().is_empty() => format!("settings.html?tab={}", encode_query_value(t)),
        _ => "settings.html".to_string(),
    };

    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);

    let win = WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App(url.into()))
        .title("Clipo Settings")
        .inner_size(780.0, 580.0)
        .min_inner_size(680.0, 480.0)
        .resizable(true)
        .center()
        .build()
        .map_err(|e| ClipoError::Internal(format!("failed to create settings window: {e}")))?;

    let _ = win.show();
    let _ = win.set_focus();

    Ok(())
}

/// Restores accessory dock policy on macOS when the Settings window is destroyed.
pub fn on_settings_destroyed(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

fn resolve_target_monitor(popup: &tauri::WebviewWindow, cursor: (i32, i32)) -> Option<Monitor> {
    if let Ok(monitors) = popup.available_monitors() {
        for monitor in monitors {
            let pos = monitor.position();
            let size = monitor.size();
            let right = pos.x.saturating_add(size.width as i32);
            let bottom = pos.y.saturating_add(size.height as i32);
            if cursor.0 >= pos.x && cursor.0 < right && cursor.1 >= pos.y && cursor.1 < bottom {
                return Some(monitor);
            }
        }
    }

    popup
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| popup.primary_monitor().ok().flatten())
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
