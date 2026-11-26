use std::collections::HashMap;
use std::env;
use std::io::Read;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use wl_clipboard_rs::copy::{
    clear as wl_clear, ClipboardType as WlCopyClipboardType, MimeSource, MimeType as WlCopyMimeType,
    Options as WlCopyOptions, Seat as WlCopySeat, Source as WlSource,
};
use wl_clipboard_rs::paste::{
    get_contents as wl_get_contents, get_mime_types_ordered as wl_get_mime_types_ordered,
    ClipboardType as WlPasteClipboardType, MimeType as WlPasteMimeType, Seat as WlPasteSeat,
};
use wl_clipboard_rs::utils::{is_primary_selection_supported, PrimarySelectionCheckError};
use wl_clipboard_rs::watch::{
    ClipboardEvent as WlClipboardEvent, ClipboardType as WlWatchClipboardType,
    Watcher as WlWatcher,
};
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{self, SelectionEventMask};
use x11rb::protocol::xproto::{
    self, Atom, AtomEnum, ConnectionExt as _, EventMask, PropMode, SelectionNotifyEvent,
    WindowClass, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use crate::clipboard::privacy::has_password_manager_marker;
use crate::clipboard::{
    AppTarget, ClipboardBackend, ClipboardWatcherHandle, PastePayload, PlatformCapabilities,
    RawClipboardSnapshot, SnapshotCallback,
};
use crate::models::{ClipoError, ItemKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxSession {
    Wayland,
    X11,
    Headless,
}

#[derive(Debug)]
struct X11OwnerHandle {
    stop_flag: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl X11OwnerHandle {
    fn stop_and_join(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for X11OwnerHandle {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// Linux clipboard backend supporting Wayland (`wl-clipboard-rs` + capability probe) and
/// X11 (`x11rb` XFixes / XTest + selection owner thread).
#[derive(Debug, Default)]
pub struct LinuxClipboardBackend {
    write_lock: Mutex<()>,
    x11_owner: Mutex<Option<X11OwnerHandle>>,
}

impl Drop for LinuxClipboardBackend {
    fn drop(&mut self) {
        self.stop_x11_owner();
    }
}

impl LinuxClipboardBackend {
    /// Creates a new Linux clipboard backend.
    pub fn new() -> Self {
        Self::default()
    }

    fn detect_session() -> LinuxSession {
        let session_type = env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let has_wayland = env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
        let has_x11 = env::var_os("DISPLAY").is_some_and(|v| !v.is_empty());

        if session_type == "wayland" || (has_wayland && session_type != "x11") {
            LinuxSession::Wayland
        } else if has_x11 || session_type == "x11" {
            LinuxSession::X11
        } else {
            LinuxSession::Headless
        }
    }

    fn command_exists(name: &str) -> bool {
        Command::new("which")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn probe_wayland_data_control() -> (bool, Option<String>) {
        match is_primary_selection_supported() {
            Ok(_) | Err(PrimarySelectionCheckError::NoSeats) => (true, None),
            Err(PrimarySelectionCheckError::MissingProtocol) => (
                false,
                Some(
                    "Wayland compositor does not expose ext-data-control or wlr-data-control; background clipboard watching is restricted"
                        .to_string(),
                ),
            ),
            Err(err) => (
                false,
                Some(format!("Wayland clipboard protocol unavailable: {err}")),
            ),
        }
    }

    fn parse_uri_list(bytes: &[u8]) -> Vec<String> {
        let text = String::from_utf8_lossy(bytes);
        let mut paths = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed == "copy" || trimmed == "cut" {
                continue;
            }
            if let Ok(parsed) = url::Url::parse(trimmed) {
                if let Ok(path) = parsed.to_file_path() {
                    paths.push(path.to_string_lossy().into_owned());
                }
            }
        }
        paths
    }

    fn stop_x11_owner(&self) {
        let prev = self.x11_owner.lock().ok().and_then(|mut g| g.take());
        if let Some(mut handle) = prev {
            handle.stop_and_join();
        }
    }
}

impl ClipboardBackend for LinuxClipboardBackend {
    fn start_watcher(
        &self,
        poll_interval: Duration,
        on_snapshot: SnapshotCallback,
    ) -> Result<ClipboardWatcherHandle, ClipoError> {
        match Self::detect_session() {
            LinuxSession::Wayland => start_wayland_watcher(on_snapshot),
            LinuxSession::X11 => start_x11_watcher(poll_interval, on_snapshot),
            LinuxSession::Headless => Err(ClipoError::Clipboard(
                "no X11 DISPLAY or WAYLAND_DISPLAY session detected".to_string(),
            )),
        }
    }

    fn write_payload(&self, payload: &PastePayload, plain_only: bool) -> Result<(), ClipoError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ClipoError::Clipboard("Linux clipboard lock poisoned".to_string()))?;
        match Self::detect_session() {
            LinuxSession::Wayland => write_wayland(payload, plain_only),
            LinuxSession::X11 => self.write_x11(payload, plain_only),
            LinuxSession::Headless => Err(ClipoError::Clipboard(
                "no X11 or Wayland display available to write clipboard".to_string(),
            )),
        }
    }

    fn clear_clipboard(&self) -> Result<(), ClipoError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ClipoError::Clipboard("Linux clipboard lock poisoned".to_string()))?;
        self.stop_x11_owner();
        match Self::detect_session() {
            LinuxSession::Wayland => {
                wl_clear(WlCopyClipboardType::Regular, WlCopySeat::All).map_err(|err| {
                    ClipoError::Clipboard(format!("failed to clear Wayland clipboard: {err}"))
                })
            }
            LinuxSession::X11 => {
                let (conn, screen_num) = x11rb::connect(None).map_err(|err| {
                    ClipoError::Clipboard(format!("failed to connect to X11 display: {err}"))
                })?;
                let clipboard_atom = intern_atom(&conn, b"CLIPBOARD")?;
                let _ = &conn.setup().roots[screen_num];
                conn.set_selection_owner(x11rb::NONE, clipboard_atom, x11rb::CURRENT_TIME)
                    .map_err(|err| {
                        ClipoError::Clipboard(format!("failed to clear X11 CLIPBOARD: {err}"))
                    })?;
                conn.flush().map_err(|err| {
                    ClipoError::Clipboard(format!("failed to flush X11 display: {err}"))
                })?;
                Ok(())
            }
            LinuxSession::Headless => Err(ClipoError::Clipboard(
                "no X11 or Wayland display available".to_string(),
            )),
        }
    }

    fn current_frontmost_app(&self) -> Option<AppTarget> {
        if Self::detect_session() == LinuxSession::X11 {
            return read_x11_frontmost_app();
        }
        None
    }

    fn activate_app(&self, target: &AppTarget) -> Result<(), ClipoError> {
        if Self::detect_session() != LinuxSession::X11 {
            return Ok(());
        }
        if let Some(pid) = target.pid {
            if let Ok(output) = Command::new("xdotool")
                .args(["search", "--pid", &pid.to_string(), "windowactivate"])
                .output()
            {
                if output.status.success() {
                    thread::sleep(Duration::from_millis(35));
                }
            }
        }
        Ok(())
    }

    fn send_paste_shortcut(&self, cursor_back_chars: Option<usize>) -> Result<(), ClipoError> {
        match Self::detect_session() {
            LinuxSession::X11 => send_x11_paste_shortcut(cursor_back_chars),
            LinuxSession::Wayland => {
                if Self::command_exists("wtype") {
                    let status = Command::new("wtype")
                        .args(["-M", "ctrl", "-k", "v", "-m", "ctrl"])
                        .status()
                        .map_err(|err| {
                            ClipoError::Paste(format!("failed to invoke wtype: {err}"))
                        })?;
                    if !status.success() {
                        return Err(ClipoError::Paste("wtype failed to send Ctrl+V".to_string()));
                    }
                    if let Some(steps) = cursor_back_chars.filter(|&n| n > 0) {
                        thread::sleep(Duration::from_millis(15));
                        for _ in 0..steps {
                            let _ = Command::new("wtype").args(["-k", "Left"]).status();
                        }
                    }
                    Ok(())
                } else {
                    Err(ClipoError::PermissionDenied(
                        "Wayland session lacks synthetic keyboard tool (`wtype`)".to_string(),
                    ))
                }
            }
            LinuxSession::Headless => Err(ClipoError::Paste(
                "cannot dispatch paste shortcut in headless session".to_string(),
            )),
        }
    }

    fn capabilities(&self) -> PlatformCapabilities {
        match Self::detect_session() {
            LinuxSession::Wayland => {
                let (can_watch, watch_note) = Self::probe_wayland_data_control();
                let has_wtype = Self::command_exists("wtype");
                let mut notes = Vec::new();
                if let Some(note) = watch_note {
                    notes.push(note);
                }
                if !has_wtype {
                    notes.push(
                        "Wayland restricts synthetic Ctrl+V without `wtype`; Copy-only fallback active"
                            .to_string(),
                    );
                }
                PlatformCapabilities {
                    os: "linux".to_string(),
                    session_type: "wayland".to_string(),
                    can_watch_clipboard: can_watch,
                    can_auto_paste: has_wtype,
                    accessibility_granted: None,
                    notes: (!notes.is_empty()).then(|| notes.join("; ")),
                }
            }
            LinuxSession::X11 => {
                let xtest_ok = check_x11_xtest_available();
                let xdotool_ok = Self::command_exists("xdotool");
                let can_paste = xtest_ok || xdotool_ok;
                PlatformCapabilities {
                    os: "linux".to_string(),
                    session_type: "x11".to_string(),
                    can_watch_clipboard: true,
                    can_auto_paste: can_paste,
                    accessibility_granted: None,
                    notes: (!can_paste).then(|| {
                        "Neither X11 XTEST extension nor xdotool is available for auto-paste"
                            .to_string()
                    }),
                }
            }
            LinuxSession::Headless => PlatformCapabilities {
                os: "linux".to_string(),
                session_type: "headless".to_string(),
                can_watch_clipboard: false,
                can_auto_paste: false,
                accessibility_granted: None,
                notes: Some("No X11 or Wayland display session detected".to_string()),
            },
        }
    }
}

impl LinuxClipboardBackend {
    fn write_x11(&self, payload: &PastePayload, plain_only: bool) -> Result<(), ClipoError> {
        self.stop_x11_owner();
        let entries = build_mime_entries(payload, plain_only)?;
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop_flag);
        let (ready_tx, ready_rx) = sync_channel::<Result<(), ClipoError>>(1);

        let join_handle = thread::Builder::new()
            .name("clipo-x11-selection-owner".to_string())
            .spawn(move || {
                serve_x11_selection(entries, stop_clone, ready_tx);
            })
            .map_err(|err| {
                ClipoError::Clipboard(format!("failed to start X11 selection owner thread: {err}"))
            })?;

        match ready_rx.recv() {
            Ok(Ok(())) => {
                if let Ok(mut guard) = self.x11_owner.lock() {
                    *guard = Some(X11OwnerHandle {
                        stop_flag,
                        join_handle: Some(join_handle),
                    });
                }
                Ok(())
            }
            Ok(Err(err)) => {
                let _ = join_handle.join();
                Err(err)
            }
            Err(_) => {
                let _ = join_handle.join();
                Err(ClipoError::Clipboard(
                    "X11 selection owner thread exited before acquiring CLIPBOARD".to_string(),
                ))
            }
        }
    }
}

fn build_mime_entries(
    payload: &PastePayload,
    plain_only: bool,
) -> Result<Vec<(String, Vec<u8>)>, ClipoError> {
    let mut entries = Vec::new();
    if plain_only {
        let text = payload.effective_plain_text().unwrap_or_default();
        let bytes = text.into_bytes();
        entries.push(("UTF8_STRING".to_string(), bytes.clone()));
        entries.push(("text/plain;charset=utf-8".to_string(), bytes.clone()));
        entries.push(("text/plain".to_string(), bytes));
        return Ok(entries);
    }

    match payload.kind {
        ItemKind::Text => {
            if let Some(ref text) = payload.plain_text {
                let bytes = text.as_bytes().to_vec();
                entries.push(("UTF8_STRING".to_string(), bytes.clone()));
                entries.push(("text/plain;charset=utf-8".to_string(), bytes.clone()));
                entries.push(("text/plain".to_string(), bytes));
            }
            if let Some(ref html) = payload.html_payload {
                entries.push(("text/html".to_string(), html.as_bytes().to_vec()));
            }
            if let Some(ref rtf) = payload.rtf_payload {
                entries.push(("text/rtf".to_string(), rtf.as_bytes().to_vec()));
                entries.push(("application/rtf".to_string(), rtf.as_bytes().to_vec()));
            }
        }
        ItemKind::Image => {
            let bytes = payload.image_bytes.clone().ok_or_else(|| {
                ClipoError::Clipboard("image paste payload missing bytes".to_string())
            })?;
            entries.push(("image/png".to_string(), bytes));
        }
        ItemKind::File => {
            if payload.file_paths.is_empty() {
                return Err(ClipoError::Clipboard(
                    "file paste payload missing file_paths".to_string(),
                ));
            }
            let uri_list: String = payload
                .file_paths
                .iter()
                .filter_map(|p| url::Url::from_file_path(p).ok().map(|u| u.to_string()))
                .collect::<Vec<_>>()
                .join("\r\n");
            entries.push(("text/uri-list".to_string(), uri_list.into_bytes()));
        }
    }

    Ok(entries)
}

fn write_wayland(payload: &PastePayload, plain_only: bool) -> Result<(), ClipoError> {
    let entries = build_mime_entries(payload, plain_only)?;
    let sources: Vec<MimeSource> = entries
        .into_iter()
        .map(|(mime, bytes)| MimeSource {
            source: WlSource::Bytes(bytes.into_boxed_slice()),
            mime_type: if mime == "text/plain" {
                WlCopyMimeType::Text
            } else {
                WlCopyMimeType::Specific(mime)
            },
        })
        .collect();

    let mut opts = WlCopyOptions::new();
    opts.clipboard(WlCopyClipboardType::Regular);
    opts.copy_multi(sources)
        .map_err(|err| ClipoError::Clipboard(format!("Wayland clipboard copy failed: {err}")))
}

fn start_wayland_watcher(on_snapshot: SnapshotCallback) -> Result<ClipboardWatcherHandle, ClipoError> {
    let mut watcher =
        WlWatcher::new(WlWatchClipboardType::Regular, WlPasteSeat::Unspecified).map_err(|err| {
            ClipoError::Clipboard(format!("failed to start Wayland clipboard watcher: {err}"))
        })?;
    let cancel_handle = watcher.cancel_handle();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_flag);
    let stop_hook: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        cancel_handle.cancel();
    });

    let join_handle = thread::Builder::new()
        .name("clipo-wayland-clipboard".to_string())
        .spawn(move || {
            let mut is_initial_event = true;
            while !stop_clone.load(Ordering::Acquire) {
                match watcher.next_event() {
                    Ok(Some(WlClipboardEvent::Changed { mime_types, .. })) => {
                        if is_initial_event {
                            is_initial_event = false;
                            continue;
                        }
                        if has_password_manager_marker(&mime_types) {
                            on_snapshot(RawClipboardSnapshot {
                                pasteboard_types: mime_types,
                                is_concealed_marker: true,
                                ..Default::default()
                            });
                            continue;
                        }
                        if let Some(snapshot) = read_wayland_snapshot_from_paste(&mime_types) {
                            on_snapshot(snapshot);
                        }
                    }
                    Ok(Some(WlClipboardEvent::Cleared { .. })) => {
                        is_initial_event = false;
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        })
        .map_err(|err| {
            ClipoError::Clipboard(format!("failed to spawn Wayland watcher thread: {err}"))
        })?;

    Ok(ClipboardWatcherHandle::from_parts(
        stop_flag,
        Some(stop_hook),
        Some(join_handle),
    ))
}

fn read_wayland_snapshot_from_paste(initial_mimes: &[String]) -> Option<RawClipboardSnapshot> {
    let mimes = wl_get_mime_types_ordered(WlPasteClipboardType::Regular, WlPasteSeat::Unspecified)
        .unwrap_or_else(|_| initial_mimes.to_vec());

    if has_password_manager_marker(&mimes) {
        return Some(RawClipboardSnapshot {
            pasteboard_types: mimes,
            is_concealed_marker: true,
            ..Default::default()
        });
    }

    let read_mime = |mime: &str| -> Option<Vec<u8>> {
        let (mut pipe, _) = wl_get_contents(
            WlPasteClipboardType::Regular,
            WlPasteSeat::Unspecified,
            WlPasteMimeType::Specific(mime),
        )
        .ok()?;
        let mut buf = Vec::new();
        pipe.read_to_end(&mut buf).ok()?;
        Some(buf)
    };

    let file_paths = if mimes.iter().any(|m| m == "text/uri-list") {
        read_mime("text/uri-list")
            .map(|b| LinuxClipboardBackend::parse_uri_list(&b))
            .unwrap_or_default()
    } else if mimes.iter().any(|m| m == "x-special/gnome-copied-files") {
        read_mime("x-special/gnome-copied-files")
            .map(|b| LinuxClipboardBackend::parse_uri_list(&b))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let image_bytes = mimes
        .iter()
        .find(|m| m.eq_ignore_ascii_case("image/png"))
        .and_then(|m| read_mime(m))
        .filter(|b| !b.is_empty());

    const PLAIN_TEXT_MIMES: &[&str] = &[
        "text/plain;charset=utf-8",
        "text/plain; charset=utf-8",
        "UTF8_STRING",
        "text/plain",
        "STRING",
        "TEXT",
    ];
    let plain_text = PLAIN_TEXT_MIMES
        .iter()
        .find_map(|candidate| {
            mimes
                .iter()
                .find(|offered| offered.eq_ignore_ascii_case(candidate))
                .and_then(|offered| read_mime(offered))
        })
        .map(|b| String::from_utf8_lossy(&b).into_owned());

    let html_payload = mimes
        .iter()
        .find(|m| m.eq_ignore_ascii_case("text/html"))
        .and_then(|m| read_mime(m))
        .map(|b| String::from_utf8_lossy(&b).into_owned());

    let rtf_payload = mimes
        .iter()
        .find(|m| m.eq_ignore_ascii_case("text/rtf") || m.eq_ignore_ascii_case("application/rtf"))
        .and_then(|m| read_mime(m))
        .map(|b| String::from_utf8_lossy(&b).into_owned());

    Some(RawClipboardSnapshot {
        plain_text,
        rtf_payload,
        html_payload,
        image_bytes,
        image_width: None,
        image_height: None,
        file_paths,
        source_app_name: None,
        source_app_id: None,
        pasteboard_types: mimes,
        is_concealed_marker: false,
        content_hash: None,
    })
}

fn start_x11_watcher(
    poll_interval: Duration,
    on_snapshot: SnapshotCallback,
) -> Result<ClipboardWatcherHandle, ClipoError> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_flag);
    let interval = poll_interval.max(Duration::from_millis(50));

    let join_handle = thread::Builder::new()
        .name("clipo-x11-clipboard".to_string())
        .spawn(move || {
            let Ok((conn, screen_num)) = x11rb::connect(None) else {
                return;
            };
            let Ok(clipboard_atom) = intern_atom(&conn, b"CLIPBOARD") else {
                return;
            };
            let screen = &conn.setup().roots[screen_num];
            let Ok(win) = conn.generate_id() else {
                return;
            };
            if conn
                .create_window(
                    x11rb::COPY_DEPTH_FROM_PARENT,
                    win,
                    screen.root,
                    0,
                    0,
                    1,
                    1,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    0,
                    &Default::default(),
                )
                .is_err()
            {
                return;
            }

            let xfixes_enabled = xfixes::query_version(&conn, 5, 0)
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some()
                && xfixes::select_selection_input(
                    &conn,
                    win,
                    clipboard_atom,
                    SelectionEventMask::SET_SELECTION_OWNER,
                )
                .is_ok();
            let _ = conn.flush();
            let mut last_owner = conn
                .get_selection_owner(clipboard_atom)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| r.owner)
                .unwrap_or(x11rb::NONE);

            while !stop_clone.load(Ordering::Acquire) {
                if xfixes_enabled {
                    match conn.poll_for_event() {
                        Ok(Some(Event::XfixesSelectionNotify(ev))) => {
                            if ev.selection == clipboard_atom {
                                if let Some(snapshot) = read_x11_snapshot(&conn, win, clipboard_atom)
                                {
                                    on_snapshot(snapshot);
                                }
                            }
                        }
                        Ok(Some(_)) => {}
                        Ok(None) => thread::sleep(Duration::from_millis(40)),
                        Err(_) => break,
                    }
                } else {
                    thread::sleep(interval);
                    if stop_clone.load(Ordering::Acquire) {
                        break;
                    }
                    if let Some(reply) = conn
                        .get_selection_owner(clipboard_atom)
                        .ok()
                        .and_then(|c| c.reply().ok())
                    {
                        if reply.owner != last_owner {
                            last_owner = reply.owner;
                            if reply.owner != x11rb::NONE {
                                if let Some(snapshot) = read_x11_snapshot(&conn, win, clipboard_atom)
                                {
                                    on_snapshot(snapshot);
                                }
                            }
                        }
                    }
                }
            }

            let _ = conn.destroy_window(win);
            let _ = conn.flush();
        })
        .map_err(|err| {
            ClipoError::Clipboard(format!("failed to spawn X11 clipboard watcher: {err}"))
        })?;

    Ok(ClipboardWatcherHandle::from_parts(
        stop_flag,
        None,
        Some(join_handle),
    ))
}

fn read_x11_snapshot(
    conn: &RustConnection,
    win: xproto::Window,
    clipboard_atom: Atom,
) -> Option<RawClipboardSnapshot> {
    let frontmost = read_x11_frontmost_app_with_conn(conn);
    let targets_atom = intern_atom(conn, b"TARGETS").ok()?;
    let prop_atom = intern_atom(conn, b"CLIPO_SEL_PROP").ok()?;

    let raw_targets = request_x11_selection(conn, win, clipboard_atom, targets_atom, prop_atom)?;
    let mut target_atoms = Vec::new();
    for chunk in raw_targets.chunks_exact(4) {
        let atom = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if atom != 0 {
            target_atoms.push(atom);
        }
    }

    let mut formats = Vec::with_capacity(target_atoms.len());
    let mut atom_by_name: HashMap<String, Atom> = HashMap::new();
    for atom in target_atoms {
        if let Ok(cookie) = conn.get_atom_name(atom) {
            if let Ok(reply) = cookie.reply() {
                let name = String::from_utf8_lossy(&reply.name).into_owned();
                atom_by_name.insert(name.to_ascii_lowercase(), atom);
                formats.push(name);
            }
        }
    }

    let is_concealed_marker = has_password_manager_marker(&formats);
    if is_concealed_marker {
        return Some(RawClipboardSnapshot {
            pasteboard_types: formats,
            is_concealed_marker: true,
            source_app_name: frontmost.as_ref().and_then(|a| a.app_name.clone()),
            source_app_id: frontmost.as_ref().and_then(|a| a.app_id.clone()),
            ..Default::default()
        });
    }

    let read_target = |names: &[&str]| -> Option<Vec<u8>> {
        for name in names {
            if let Some(&atom) = atom_by_name.get(&name.to_ascii_lowercase()) {
                if let Some(data) = request_x11_selection(conn, win, clipboard_atom, atom, prop_atom) {
                    return Some(data);
                }
            }
        }
        None
    };

    let file_paths = read_target(&["text/uri-list", "x-special/gnome-copied-files"])
        .map(|b| LinuxClipboardBackend::parse_uri_list(&b))
        .unwrap_or_default();

    let image_bytes = read_target(&["image/png"]).filter(|b| !b.is_empty());
    let plain_text = read_target(&["UTF8_STRING", "text/plain;charset=utf-8", "text/plain", "STRING"])
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let html_payload =
        read_target(&["text/html"]).map(|b| String::from_utf8_lossy(&b).into_owned());
    let rtf_payload = read_target(&["text/rtf", "application/rtf"])
        .map(|b| String::from_utf8_lossy(&b).into_owned());

    Some(RawClipboardSnapshot {
        plain_text,
        rtf_payload,
        html_payload,
        image_bytes,
        image_width: None,
        image_height: None,
        file_paths,
        source_app_name: frontmost.as_ref().and_then(|a| a.app_name.clone()),
        source_app_id: frontmost.as_ref().and_then(|a| a.app_id.clone()),
        pasteboard_types: formats,
        is_concealed_marker: false,
        content_hash: None,
    })
}

fn request_x11_selection(
    conn: &RustConnection,
    win: xproto::Window,
    selection: Atom,
    target: Atom,
    property: Atom,
) -> Option<Vec<u8>> {
    conn.convert_selection(win, selection, target, property, x11rb::CURRENT_TIME)
        .ok()?;
    conn.flush().ok()?;

    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        if let Ok(Some(Event::SelectionNotify(ev))) = conn.poll_for_event() {
            if ev.selection == selection && ev.target == target {
                if ev.property == x11rb::NONE {
                    return None;
                }
                let reply = conn
                    .get_property(true, win, property, AtomEnum::ANY, 0, u32::MAX / 4)
                    .ok()?
                    .reply()
                    .ok()?;
                return Some(reply.value);
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
    None
}

struct X11SelectionSetup {
    conn: RustConnection,
    win: xproto::Window,
    targets_atom: Atom,
    atom_type: Atom,
    offered_atoms: Vec<Atom>,
    target_map: HashMap<Atom, Vec<u8>>,
}

fn setup_x11_selection(entries: Vec<(String, Vec<u8>)>) -> Result<X11SelectionSetup, ClipoError> {
    let (conn, screen_num) = x11rb::connect(None)
        .map_err(|err| ClipoError::Clipboard(format!("X11 connect failed: {err}")))?;
    let screen = &conn.setup().roots[screen_num];
    let win = conn
        .generate_id()
        .map_err(|err| ClipoError::Clipboard(format!("X11 window ID failed: {err}")))?;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        win,
        screen.root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &Default::default(),
    )
    .map_err(|err| ClipoError::Clipboard(format!("X11 create_window failed: {err}")))?;

    let clipboard_atom = intern_atom(&conn, b"CLIPBOARD")?;
    let targets_atom = intern_atom(&conn, b"TARGETS")?;
    let atom_type: Atom = AtomEnum::ATOM.into();

    let mut target_map: HashMap<Atom, Vec<u8>> = HashMap::new();
    let mut offered_atoms: Vec<Atom> = vec![targets_atom];
    for (mime, bytes) in entries {
        let atom = intern_atom(&conn, mime.as_bytes())?;
        offered_atoms.push(atom);
        target_map.insert(atom, bytes);
    }

    conn.set_selection_owner(win, clipboard_atom, x11rb::CURRENT_TIME)
        .map_err(|err| ClipoError::Clipboard(format!("X11 set_selection_owner failed: {err}")))?;
    conn.flush()
        .map_err(|err| ClipoError::Clipboard(format!("X11 flush failed: {err}")))?;
    // Synchronous X11 roundtrip ensures the server has processed SetSelectionOwner
    // before write_x11 returns and PasteEngine dispatches Ctrl+V.
    let _ = conn
        .get_selection_owner(clipboard_atom)
        .ok()
        .and_then(|c| c.reply().ok());

    Ok(X11SelectionSetup {
        conn,
        win,
        targets_atom,
        atom_type,
        offered_atoms,
        target_map,
    })
}

fn serve_x11_selection(
    entries: Vec<(String, Vec<u8>)>,
    stop_flag: Arc<AtomicBool>,
    ready_tx: SyncSender<Result<(), ClipoError>>,
) {
    let X11SelectionSetup {
        conn,
        win,
        targets_atom,
        atom_type,
        offered_atoms,
        target_map,
    } = match setup_x11_selection(entries) {
        Ok(setup) => {
            let _ = ready_tx.send(Ok(()));
            setup
        }
        Err(err) => {
            let _ = ready_tx.send(Err(err));
            return;
        }
    };

    while !stop_flag.load(Ordering::Acquire) {
        match conn.poll_for_event() {
            Ok(Some(Event::SelectionClear(_))) => break,
            Ok(Some(Event::SelectionRequest(req))) => {
                let mut prop = req.property;
                if prop == x11rb::NONE {
                    prop = req.target;
                }
                let served = if req.target == targets_atom {
                    let bytes: Vec<u8> = offered_atoms
                        .iter()
                        .flat_map(|a| a.to_ne_bytes())
                        .collect();
                    conn.change_property(
                        PropMode::REPLACE,
                        req.requestor,
                        prop,
                        atom_type,
                        32,
                        offered_atoms.len() as u32,
                        &bytes,
                    )
                    .is_ok()
                } else if let Some(data) = target_map.get(&req.target) {
                    conn.change_property(
                        PropMode::REPLACE,
                        req.requestor,
                        prop,
                        req.target,
                        8,
                        data.len() as u32,
                        data,
                    )
                    .is_ok()
                } else {
                    false
                };

                let notify = SelectionNotifyEvent {
                    response_type: xproto::SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: req.time,
                    requestor: req.requestor,
                    selection: req.selection,
                    target: req.target,
                    property: if served { prop } else { x11rb::NONE },
                };
                let _ = conn.send_event(false, req.requestor, EventMask::NO_EVENT, notify);
                let _ = conn.flush();
            }
            Ok(Some(_)) => {}
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => break,
        }
    }

    let _ = conn.destroy_window(win);
    let _ = conn.flush();
}

fn read_x11_frontmost_app() -> Option<AppTarget> {
    let (conn, _) = x11rb::connect(None).ok()?;
    read_x11_frontmost_app_with_conn(&conn)
}

fn read_x11_frontmost_app_with_conn(conn: &RustConnection) -> Option<AppTarget> {
    let root = conn.setup().roots.first()?.root;
    let active_atom = intern_atom(conn, b"_NET_ACTIVE_WINDOW").ok()?;
    let active_reply = conn
        .get_property(false, root, active_atom, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    if active_reply.value.len() < 4 {
        return None;
    }
    let active_win = u32::from_ne_bytes([
        active_reply.value[0],
        active_reply.value[1],
        active_reply.value[2],
        active_reply.value[3],
    ]);
    if active_win == 0 {
        return None;
    }

    let wm_class_atom: Atom = AtomEnum::WM_CLASS.into();
    let wm_class = conn
        .get_property(false, active_win, wm_class_atom, AtomEnum::STRING, 0, 256)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| {
            let parts: Vec<String> = r
                .value
                .split(|&b| b == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect();
            (parts.first().cloned(), parts.last().cloned())
        });

    let pid = intern_atom(conn, b"_NET_WM_PID")
        .ok()
        .and_then(|atom| {
            conn.get_property(false, active_win, atom, AtomEnum::CARDINAL, 0, 1)
                .ok()?
                .reply()
                .ok()
        })
        .and_then(|r| {
            (r.value.len() >= 4).then(|| {
                i32::from_ne_bytes([r.value[0], r.value[1], r.value[2], r.value[3]])
            })
        });

    let app_id = wm_class.as_ref().and_then(|(inst, cls)| cls.clone().or_else(|| inst.clone()));
    let app_name = wm_class
        .as_ref()
        .and_then(|(inst, cls)| cls.clone().or_else(|| inst.clone()));

    Some(AppTarget {
        app_id,
        app_name,
        pid,
    })
}

fn check_x11_xtest_available() -> bool {
    let Ok((conn, _)) = x11rb::connect(None) else {
        return false;
    };
    conn.xtest_get_version(2, 1)
        .ok()
        .and_then(|c| c.reply().ok())
        .is_some()
}

fn send_x11_paste_shortcut(cursor_back_chars: Option<usize>) -> Result<(), ClipoError> {
    if let Ok((conn, screen_num)) = x11rb::connect(None) {
        if conn
            .xtest_get_version(2, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some()
        {
            let root = conn.setup().roots[screen_num].root;
            if let (Some(ctrl), Some(v_key)) = (
                lookup_x11_keycode(&conn, 0xFFE3), // XK_Control_L
                lookup_x11_keycode(&conn, 0x0076), // XK_v
            ) {
                let _ = conn.xtest_fake_input(KEY_PRESS_EVENT, ctrl, 0, root, 0, 0, 0);
                let _ = conn.xtest_fake_input(KEY_PRESS_EVENT, v_key, 0, root, 0, 0, 0);
                let _ = conn.xtest_fake_input(KEY_RELEASE_EVENT, v_key, 0, root, 0, 0, 0);
                let _ = conn.xtest_fake_input(KEY_RELEASE_EVENT, ctrl, 0, root, 0, 0, 0);

                if let Some(steps) = cursor_back_chars.filter(|&n| n > 0) {
                    let _ = conn.flush();
                    thread::sleep(Duration::from_millis(15));
                    if let Some(left_key) = lookup_x11_keycode(&conn, 0xFF51) {
                        for _ in 0..steps {
                            let _ = conn.xtest_fake_input(
                                KEY_PRESS_EVENT,
                                left_key,
                                0,
                                root,
                                0,
                                0,
                                0,
                            );
                            let _ = conn.xtest_fake_input(
                                KEY_RELEASE_EVENT,
                                left_key,
                                0,
                                root,
                                0,
                                0,
                                0,
                            );
                        }
                    }
                }

                conn.flush().map_err(|err| {
                    ClipoError::Paste(format!("X11 XTEST flush failed: {err}"))
                })?;
                return Ok(());
            }
        }
    }

    if LinuxClipboardBackend::command_exists("xdotool") {
        let status = Command::new("xdotool")
            .args(["key", "--clearmodifiers", "ctrl+v"])
            .status()
            .map_err(|err| ClipoError::Paste(format!("failed to execute xdotool: {err}")))?;
        if !status.success() {
            return Err(ClipoError::Paste("xdotool ctrl+v failed".to_string()));
        }
        if let Some(steps) = cursor_back_chars.filter(|&n| n > 0) {
            thread::sleep(Duration::from_millis(15));
            for _ in 0..steps {
                let _ = Command::new("xdotool").args(["key", "Left"]).status();
            }
        }
        return Ok(());
    }

    Err(ClipoError::Paste(
        "X11 auto-paste unavailable (neither XTEST nor xdotool succeeded)".to_string(),
    ))
}

fn lookup_x11_keycode(conn: &RustConnection, target_keysym: u32) -> Option<u8> {
    let setup = conn.setup();
    let min_code = setup.min_keycode;
    let count = setup.max_keycode.saturating_sub(min_code) + 1;
    let mapping = conn.get_keyboard_mapping(min_code, count).ok()?.reply().ok()?;
    let per = mapping.keysyms_per_keycode as usize;
    if per == 0 {
        return None;
    }
    for (idx, chunk) in mapping.keysyms.chunks_exact(per).enumerate() {
        if chunk.iter().any(|&ks| ks == target_keysym) {
            return Some(min_code + idx as u8);
        }
    }
    None
}

fn intern_atom(conn: &RustConnection, name: &[u8]) -> Result<Atom, ClipoError> {
    conn.intern_atom(false, name)
        .map_err(|err| ClipoError::Clipboard(format!("X11 intern_atom send failed: {err}")))?
        .reply()
        .map(|r| r.atom)
        .map_err(|err| ClipoError::Clipboard(format!("X11 intern_atom reply failed: {err}")))
}
