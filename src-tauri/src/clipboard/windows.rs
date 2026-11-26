use std::ffi::OsString;
use std::io::Cursor;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use image::{ImageFormat, RgbaImage};
use windows_sys::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, EnumClipboardFormats,
    GetClipboardData, GetClipboardFormatNameW, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, RemoveClipboardFormatListener, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_LEFT,
};
use windows_sys::Win32::UI::Shell::{DragQueryFileW, DROPFILES, HDROP};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows,
    GetForegroundWindow, GetMessageW, GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, PostMessageW, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW,
    ShowWindow, TranslateMessage, GWLP_USERDATA, HWND_MESSAGE, MSG, SW_RESTORE, WM_CLIPBOARDUPDATE,
    WM_CLOSE, WM_DESTROY, WNDCLASSW,
};

use crate::clipboard::privacy::has_password_manager_marker;
use crate::clipboard::{
    AppTarget, ClipboardBackend, ClipboardWatcherHandle, PastePayload, PlatformCapabilities,
    RawClipboardSnapshot, SnapshotCallback,
};
use crate::models::{ClipoError, ItemKind};

const CF_TEXT: u32 = 1;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const VK_V: VIRTUAL_KEY = 0x56;

/// Windows `HWND_MESSAGE` + `AddClipboardFormatListener` + `SendInput` backend.
#[derive(Debug, Default)]
pub struct WindowsClipboardBackend {
    write_lock: Mutex<()>,
}

impl WindowsClipboardBackend {
    /// Creates a new Windows clipboard backend.
    pub fn new() -> Self {
        Self::default()
    }

    fn to_wide_null(s: &str) -> Vec<u16> {
        OsString::from(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn register_format(name: &str) -> u32 {
        let wide = Self::to_wide_null(name);
        // SAFETY: `wide` is null-terminated UTF-16.
        unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
    }

    fn open_clipboard_with_retry() -> Result<ClipboardGuard, ClipoError> {
        for _ in 0..10 {
            // SAFETY: Null owner opens the system clipboard for reading/writing.
            if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                return Ok(ClipboardGuard);
            }
            thread::sleep(Duration::from_millis(5));
        }
        Err(ClipoError::Clipboard(
            "failed to open Windows clipboard".to_string(),
        ))
    }

    fn read_global_bytes(handle: HANDLE) -> Option<Vec<u8>> {
        if handle.is_null() {
            return None;
        }
        let hglobal = handle as HGLOBAL;
        // SAFETY: `hglobal` comes from `GetClipboardData` while clipboard is open.
        let size = unsafe { GlobalSize(hglobal) };
        if size == 0 {
            return Some(Vec::new());
        }
        let ptr = unsafe { GlobalLock(hglobal) };
        if ptr.is_null() {
            return None;
        }
        let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, size) };
        let bytes = slice.to_vec();
        unsafe { GlobalUnlock(hglobal) };
        Some(bytes)
    }

    fn write_global_bytes(format: u32, bytes: &[u8]) -> Result<(), ClipoError> {
        // SAFETY: Allocates movable global memory owned by the clipboard upon `SetClipboardData`.
        let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) };
        if hglobal.is_null() {
            return Err(ClipoError::Clipboard(
                "GlobalAlloc failed when writing Windows clipboard".to_string(),
            ));
        }

        let ptr = unsafe { GlobalLock(hglobal) };
        if ptr.is_null() {
            unsafe { GlobalFree(hglobal) };
            return Err(ClipoError::Clipboard(
                "GlobalLock failed when writing Windows clipboard".to_string(),
            ));
        }

        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
            GlobalUnlock(hglobal);
            if SetClipboardData(format, hglobal as HANDLE).is_null() {
                GlobalFree(hglobal);
                return Err(ClipoError::Clipboard(format!(
                    "SetClipboardData failed for format {format}"
                )));
            }
        }
        Ok(())
    }

    fn format_name(fmt: u32) -> String {
        match fmt {
            CF_TEXT => "CF_TEXT".to_string(),
            CF_DIB => "CF_DIB".to_string(),
            CF_UNICODETEXT => "CF_UNICODETEXT".to_string(),
            CF_HDROP => "CF_HDROP".to_string(),
            other => {
                let mut buf = [0u16; 256];
                // SAFETY: `buf` is 256 `u16` elements long.
                let len = unsafe { GetClipboardFormatNameW(other, buf.as_mut_ptr(), buf.len() as i32) };
                if len > 0 {
                    String::from_utf16_lossy(&buf[..len as usize])
                } else {
                    format!("CF_{other}")
                }
            }
        }
    }

    fn read_snapshot() -> RawClipboardSnapshot {
        let frontmost = Self::read_frontmost_app();
        let Ok(_guard) = Self::open_clipboard_with_retry() else {
            return RawClipboardSnapshot::default();
        };

        let mut formats = Vec::new();
        let mut fmt_ids = Vec::new();
        let mut current = 0u32;
        loop {
            // SAFETY: Clipboard is currently open.
            current = unsafe { EnumClipboardFormats(current) };
            if current == 0 {
                break;
            }
            fmt_ids.push(current);
            let mut name = Self::format_name(current);
            if name.eq_ignore_ascii_case("CanIncludeInClipboardHistory") {
                // SAFETY: Clipboard is open and `current` is a valid format.
                let allowed = unsafe { GetClipboardData(current) };
                let allow_flag = Self::read_global_bytes(allowed)
                    .filter(|b| b.len() >= 4)
                    .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .unwrap_or(0);
                if allow_flag != 0 {
                    name = "CanIncludeInClipboardHistory=1".to_string();
                }
            }
            formats.push(name);
        }

        let is_concealed_marker = has_password_manager_marker(&formats);
        if is_concealed_marker {
            return RawClipboardSnapshot {
                pasteboard_types: formats,
                is_concealed_marker: true,
                source_app_name: frontmost.as_ref().and_then(|a| a.app_name.clone()),
                source_app_id: frontmost.as_ref().and_then(|a| a.app_id.clone()),
                ..Default::default()
            };
        }

        let file_paths = Self::read_hdrop_paths();
        let plain_text = Self::read_unicode_text();
        let html_fmt = Self::register_format("HTML Format");
        let rtf_fmt = Self::register_format("Rich Text Format");
        let png_fmt = Self::register_format("PNG");

        let html_payload = if html_fmt != 0 && fmt_ids.contains(&html_fmt) {
            Self::read_c_string_format(html_fmt).map(|raw| extract_cf_html_fragment(&raw))
        } else {
            None
        };

        let rtf_payload = if rtf_fmt != 0 && fmt_ids.contains(&rtf_fmt) {
            Self::read_c_string_format(rtf_fmt)
        } else {
            None
        };

        let image_bytes = if png_fmt != 0 && fmt_ids.contains(&png_fmt) {
            // SAFETY: Clipboard is open.
            Self::read_global_bytes(unsafe { GetClipboardData(png_fmt) }).filter(|b| !b.is_empty())
        } else if fmt_ids.contains(&CF_DIB) {
            // SAFETY: Clipboard is open.
            Self::read_global_bytes(unsafe { GetClipboardData(CF_DIB) })
                .and_then(|dib| dib_to_png_bytes(&dib))
        } else {
            None
        };

        RawClipboardSnapshot {
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
        }
    }

    fn read_unicode_text() -> Option<String> {
        // SAFETY: Clipboard is open.
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
        let bytes = Self::read_global_bytes(handle)?;
        let mut units = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        Some(String::from_utf16_lossy(&units))
    }

    fn read_c_string_format(fmt: u32) -> Option<String> {
        // SAFETY: Clipboard is open.
        let handle = unsafe { GetClipboardData(fmt) };
        let bytes = Self::read_global_bytes(handle)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    fn read_hdrop_paths() -> Vec<String> {
        // SAFETY: Clipboard is open.
        let handle = unsafe { GetClipboardData(CF_HDROP) };
        if handle.is_null() {
            return Vec::new();
        }
        let hdrop = handle as HDROP;
        // SAFETY: `hdrop` is a valid clipboard `HDROP` while clipboard is open.
        let count = unsafe { DragQueryFileW(hdrop, u32::MAX, ptr::null_mut(), 0) };
        let mut paths = Vec::with_capacity(count as usize);
        for idx in 0..count {
            let len = unsafe { DragQueryFileW(hdrop, idx, ptr::null_mut(), 0) };
            if len == 0 {
                continue;
            }
            let mut buf = vec![0u16; len as usize + 1];
            let written = unsafe { DragQueryFileW(hdrop, idx, buf.as_mut_ptr(), buf.len() as u32) };
            if written > 0 {
                let os = OsString::from_wide(&buf[..written as usize]);
                paths.push(os.to_string_lossy().into_owned());
            }
        }
        paths
    }

    fn read_frontmost_app() -> Option<AppTarget> {
        // SAFETY: `GetForegroundWindow` returns null if no foreground window exists.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == 0 {
            return None;
        }

        let mut exe_name = None;
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if !process.is_null() {
            let mut buf = [0u16; 1024];
            let mut size = buf.len() as u32;
            if unsafe { QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut size) } != 0
                && size > 0
            {
                let full = OsString::from_wide(&buf[..size as usize]);
                exe_name = Path::new(&full)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned());
            }
            unsafe { CloseHandle(process) };
        }

        let mut title_buf = [0u16; 512];
        let title_len = unsafe { GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32) };
        let window_title = (title_len > 0)
            .then(|| String::from_utf16_lossy(&title_buf[..title_len as usize]));

        let app_name = exe_name
            .as_deref()
            .and_then(|n| n.strip_suffix(".exe").or(Some(n)))
            .map(str::to_string)
            .or(window_title);

        Some(AppTarget {
            app_id: exe_name,
            app_name,
            pid: Some(pid as i32),
        })
    }

    fn send_key(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if key_up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: Paired with successful `OpenClipboard`.
        unsafe {
            CloseClipboard();
        }
    }
}

struct WatcherCallbackState {
    on_snapshot: SnapshotCallback,
}

unsafe extern "system" fn clipboard_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLIPBOARDUPDATE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WatcherCallbackState;
            if !ptr.is_null() {
                let state = &*ptr;
                let snapshot = WindowsClipboardBackend::read_snapshot();
                (state.on_snapshot)(snapshot);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

impl ClipboardBackend for WindowsClipboardBackend {
    fn start_watcher(
        &self,
        poll_interval: Duration,
        on_snapshot: SnapshotCallback,
    ) -> Result<ClipboardWatcherHandle, ClipoError> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop_flag);
        let hwnd_raw = Arc::new(AtomicIsize::new(0));
        let hwnd_for_hook = Arc::clone(&hwnd_raw);
        let interval = poll_interval.max(Duration::from_millis(50));

        let stop_hook: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let raw = hwnd_for_hook.load(Ordering::SeqCst);
            if raw != 0 {
                // SAFETY: `raw` is a live `HWND_MESSAGE` window created by the watcher thread.
                unsafe {
                    PostMessageW(raw as HWND, WM_CLOSE, 0, 0);
                }
            }
        });

        let join_handle = thread::Builder::new()
            .name("clipo-windows-clipboard".to_string())
            .spawn(move || {
                let class_name = Self::to_wide_null("ClipoClipboardWatcherClass");
                let wnd_class = WNDCLASSW {
                    lpfnWndProc: Some(clipboard_wnd_proc),
                    lpszClassName: class_name.as_ptr(),
                    ..unsafe { std::mem::zeroed() }
                };
                unsafe { RegisterClassW(&wnd_class) };

                let hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        class_name.as_ptr(),
                        class_name.as_ptr(),
                        0,
                        0,
                        0,
                        0,
                        0,
                        HWND_MESSAGE,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null(),
                    )
                };

                if !hwnd.is_null() && unsafe { AddClipboardFormatListener(hwnd) } != 0 {
                    let state_ptr = Box::into_raw(Box::new(WatcherCallbackState {
                        on_snapshot: Arc::clone(&on_snapshot),
                    }));
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize) };
                    hwnd_raw.store(hwnd as isize, Ordering::SeqCst);

                    let mut msg: MSG = unsafe { std::mem::zeroed() };
                    while !stop_clone.load(Ordering::Relaxed) {
                        let res = unsafe { GetMessageW(&mut msg, ptr::null_mut(), 0, 0) };
                        if res <= 0 {
                            break;
                        }
                        unsafe {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }

                    hwnd_raw.store(0, Ordering::SeqCst);
                    unsafe {
                        RemoveClipboardFormatListener(hwnd);
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                        drop(Box::from_raw(state_ptr));
                    }
                    return;
                }

                if !hwnd.is_null() {
                    unsafe { DestroyWindow(hwnd) };
                }

                // Fallback sequence polling if HWND_MESSAGE listener could not be created.
                let mut last_seq = unsafe { GetClipboardSequenceNumber() };
                while !stop_clone.load(Ordering::Relaxed) {
                    thread::sleep(interval);
                    if stop_clone.load(Ordering::Relaxed) {
                        break;
                    }
                    let seq = unsafe { GetClipboardSequenceNumber() };
                    if seq != last_seq {
                        last_seq = seq;
                        on_snapshot(Self::read_snapshot());
                    }
                }
            })
            .map_err(|err| {
                ClipoError::Clipboard(format!("failed to spawn Windows clipboard watcher: {err}"))
            })?;

        Ok(ClipboardWatcherHandle::from_parts(
            stop_flag,
            Some(stop_hook),
            Some(join_handle),
        ))
    }

    fn write_payload(&self, payload: &PastePayload, plain_only: bool) -> Result<(), ClipoError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ClipoError::Clipboard("Windows clipboard lock poisoned".to_string()))?;

        let _clip = Self::open_clipboard_with_retry()?;
        // SAFETY: Clipboard is open.
        if unsafe { EmptyClipboard() } == 0 {
            return Err(ClipoError::Clipboard(
                "EmptyClipboard failed on Windows".to_string(),
            ));
        }

        if plain_only {
            let text = payload.effective_plain_text().unwrap_or_default();
            let wide = Self::to_wide_null(&text);
            let bytes: &[u8] =
                unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
            return Self::write_global_bytes(CF_UNICODETEXT, bytes);
        }

        match payload.kind {
            ItemKind::Text => {
                if let Some(ref text) = payload.plain_text {
                    let wide = Self::to_wide_null(text);
                    let bytes: &[u8] = unsafe {
                        std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2)
                    };
                    Self::write_global_bytes(CF_UNICODETEXT, bytes)?;
                }
                if let Some(ref html) = payload.html_payload {
                    let html_fmt = Self::register_format("HTML Format");
                    if html_fmt != 0 {
                        let cf_html = format_cf_html(html);
                        let mut bytes = cf_html.into_bytes();
                        bytes.push(0);
                        Self::write_global_bytes(html_fmt, &bytes)?;
                    }
                }
                if let Some(ref rtf) = payload.rtf_payload {
                    let rtf_fmt = Self::register_format("Rich Text Format");
                    if rtf_fmt != 0 {
                        let mut bytes = rtf.as_bytes().to_vec();
                        bytes.push(0);
                        Self::write_global_bytes(rtf_fmt, &bytes)?;
                    }
                }
            }
            ItemKind::Image => {
                let bytes = payload.image_bytes.as_deref().ok_or_else(|| {
                    ClipoError::Clipboard("image paste payload missing bytes".to_string())
                })?;
                let png_fmt = Self::register_format("PNG");
                if png_fmt != 0 {
                    Self::write_global_bytes(png_fmt, bytes)?;
                }
                if let Some(dib) = png_to_dib_bytes(bytes) {
                    let _ = Self::write_global_bytes(CF_DIB, &dib);
                }
            }
            ItemKind::File => {
                if payload.file_paths.is_empty() {
                    return Err(ClipoError::Clipboard(
                        "file paste payload missing file_paths".to_string(),
                    ));
                }
                let dropfiles_size = std::mem::size_of::<DROPFILES>();
                let mut wide_paths: Vec<u16> = Vec::new();
                for path in &payload.file_paths {
                    wide_paths.extend(Self::to_wide_null(path));
                }
                wide_paths.push(0);

                let total_bytes = dropfiles_size + wide_paths.len() * 2;
                let mut buf = vec![0u8; total_bytes];
                let header = DROPFILES {
                    pFiles: dropfiles_size as u32,
                    pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
                    fNC: 0,
                    fWide: 1,
                };
                unsafe {
                    ptr::copy_nonoverlapping(
                        &header as *const DROPFILES as *const u8,
                        buf.as_mut_ptr(),
                        dropfiles_size,
                    );
                    ptr::copy_nonoverlapping(
                        wide_paths.as_ptr() as *const u8,
                        buf.as_mut_ptr().add(dropfiles_size),
                        wide_paths.len() * 2,
                    );
                }
                Self::write_global_bytes(CF_HDROP, &buf)?;
            }
        }

        Ok(())
    }

    fn clear_clipboard(&self) -> Result<(), ClipoError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ClipoError::Clipboard("Windows clipboard lock poisoned".to_string()))?;
        let _clip = Self::open_clipboard_with_retry()?;
        if unsafe { EmptyClipboard() } == 0 {
            return Err(ClipoError::Clipboard(
                "EmptyClipboard failed on Windows".to_string(),
            ));
        }
        Ok(())
    }

    fn current_frontmost_app(&self) -> Option<AppTarget> {
        Self::read_frontmost_app()
    }

    fn activate_app(&self, target: &AppTarget) -> Result<(), ClipoError> {
        let Some(target_pid) = target.pid else {
            return Ok(());
        };
        struct EnumCtx {
            pid: u32,
            found: HWND,
        }
        unsafe extern "system" fn find_window_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
            let ctx = &mut *(lparam as *mut EnumCtx);
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let mut win_pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut win_pid);
            if win_pid == ctx.pid {
                ctx.found = hwnd;
                return 0;
            }
            1
        }

        let mut ctx = EnumCtx {
            pid: target_pid as u32,
            found: ptr::null_mut(),
        };
        unsafe {
            EnumWindows(Some(find_window_cb), &mut ctx as *mut EnumCtx as LPARAM);
            if !ctx.found.is_null() {
                if IsIconic(ctx.found) != 0 {
                    ShowWindow(ctx.found, SW_RESTORE);
                }
                SetForegroundWindow(ctx.found);
                thread::sleep(Duration::from_millis(35));
            }
        }
        Ok(())
    }

    fn send_paste_shortcut(&self, cursor_back_chars: Option<usize>) -> Result<(), ClipoError> {
        let mut inputs = vec![
            Self::send_key(VK_CONTROL, false),
            Self::send_key(VK_V, false),
            Self::send_key(VK_V, true),
            Self::send_key(VK_CONTROL, true),
        ];

        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent != inputs.len() as u32 {
            return Err(ClipoError::Paste(
                "SendInput failed while dispatching Ctrl+V".to_string(),
            ));
        }

        if let Some(steps) = cursor_back_chars.filter(|&n| n > 0) {
            thread::sleep(Duration::from_millis(15));
            inputs.clear();
            for _ in 0..steps {
                inputs.push(Self::send_key(VK_LEFT, false));
                inputs.push(Self::send_key(VK_LEFT, true));
            }
            let sent_left = unsafe {
                SendInput(
                    inputs.len() as u32,
                    inputs.as_ptr(),
                    std::mem::size_of::<INPUT>() as i32,
                )
            };
            if sent_left != inputs.len() as u32 {
                return Err(ClipoError::Paste(
                    "SendInput failed while dispatching caret rewind".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            os: "windows".to_string(),
            session_type: "win32".to_string(),
            can_watch_clipboard: true,
            can_auto_paste: true,
            accessibility_granted: None,
            notes: None,
        }
    }
}

fn format_cf_html(fragment: &str) -> String {
    let prefix = "Version:0.9\r\nStartHTML:00000097\r\nEndHTML:";
    let start_fragment_marker = "\r\nStartFragment:";
    let end_fragment_marker = "\r\nEndFragment:";
    let html_open = "\r\n<html><body><!--StartFragment-->";
    let html_close = "<!--EndFragment--></body></html>";

    let header_len = 97usize;
    let start_fragment = header_len + "<html><body><!--StartFragment-->".len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + "<!--EndFragment--></body></html>".len();

    format!(
        "{prefix}{end_html:08}{start_fragment_marker}{start_fragment:08}{end_fragment_marker}{end_fragment:08}{html_open}{fragment}{html_close}"
    )
}

fn extract_cf_html_fragment(raw: &str) -> String {
    if let (Some(start), Some(end)) = (
        raw.find("<!--StartFragment-->"),
        raw.find("<!--EndFragment-->"),
    ) {
        let frag_start = start + "<!--StartFragment-->".len();
        if frag_start <= end {
            return raw[frag_start..end].to_string();
        }
    }
    raw.to_string()
}

fn dib_to_png_bytes(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < 40 {
        return None;
    }
    let header_size = u32::from_le_bytes([dib[0], dib[1], dib[2], dib[3]]) as usize;
    if header_size < 40 || dib.len() < header_size {
        return None;
    }
    let width = i32::from_le_bytes([dib[4], dib[5], dib[6], dib[7]]);
    let raw_height = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
    let bpp = u16::from_le_bytes([dib[14], dib[15]]);
    let compression = u32::from_le_bytes([dib[16], dib[17], dib[18], dib[19]]);

    if width <= 0 || raw_height == 0 || (bpp != 24 && bpp != 32) || (compression != 0 && compression != 3) {
        return None;
    }

    let w = width as u32;
    let top_down = raw_height < 0;
    let h = raw_height.unsigned_abs();
    let mask_offset = if compression == 3 && header_size == 40 { 12 } else { 0 };
    let pixels = dib.get(header_size + mask_offset..)?;

    let stride = (w as usize * bpp as usize).div_ceil(32) * 4;
    if pixels.len() < stride * h as usize {
        return None;
    }

    let mut rgba = RgbaImage::new(w, h);
    for y in 0..h {
        let src_row = if top_down { y } else { h - 1 - y } as usize;
        let row_slice = &pixels[src_row * stride..];
        for x in 0..w {
            let off = x as usize * (bpp as usize / 8);
            let b = row_slice[off];
            let g = row_slice[off + 1];
            let r = row_slice[off + 2];
            let a = if bpp == 32 { row_slice[off + 3] } else { 255 };
            rgba.put_pixel(x, y, image::Rgba([r, g, b, a]));
        }
    }

    let mut out = Vec::new();
    let mut cursor = Cursor::new(&mut out);
    image::DynamicImage::ImageRgba8(rgba)
        .write_to(&mut cursor, ImageFormat::Png)
        .ok()?;
    Some(out)
}

fn png_to_dib_bytes(png_bytes: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(png_bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let stride = (w as usize) * 4;
    let mut dib = Vec::with_capacity(40 + stride * h as usize);
    dib.extend_from_slice(&40u32.to_le_bytes()); // biSize
    dib.extend_from_slice(&(w as i32).to_le_bytes()); // biWidth
    dib.extend_from_slice(&(h as i32).to_le_bytes()); // biHeight (bottom-up)
    dib.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    dib.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    dib.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    dib.extend_from_slice(&((stride * h as usize) as u32).to_le_bytes()); // biSizeImage
    dib.extend_from_slice(&[0u8; 16]);

    for y in (0..h).rev() {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            dib.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    Some(dib)
}
