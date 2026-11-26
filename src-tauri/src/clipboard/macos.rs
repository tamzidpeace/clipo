use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBitmapImageFileType, NSBitmapImageRep,
    NSBitmapImageRepPropertyKey, NSPasteboard, NSPasteboardItem, NSPasteboardTypeFileURL,
    NSPasteboardTypeHTML, NSPasteboardTypePNG, NSPasteboardTypeRTF, NSPasteboardTypeString,
    NSPasteboardTypeTIFF, NSPasteboardWriting, NSRunningApplication, NSWorkspace,
};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGKeyCode,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString, NSURL};

use crate::clipboard::privacy::has_password_manager_marker;
use crate::clipboard::{
    AppTarget, ClipboardBackend, ClipboardWatcherHandle, PastePayload, PlatformCapabilities,
    RawClipboardSnapshot, SnapshotCallback,
};
use crate::models::{ClipoError, ItemKind};

const KVK_ANSI_V: CGKeyCode = 0x09;
const KVK_LEFT_ARROW: CGKeyCode = 0x7B;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

/// macOS `NSPasteboard` + `NSWorkspace` + `CGEvent` clipboard and auto-paste backend.
#[derive(Debug, Default)]
pub struct MacosClipboardBackend {
    write_lock: Mutex<()>,
}

impl MacosClipboardBackend {
    /// Creates a new macOS clipboard backend.
    pub fn new() -> Self {
        Self::default()
    }

    fn is_accessibility_trusted() -> bool {
        // SAFETY: `AXIsProcessTrusted` takes no arguments and returns macOS `Boolean` (0 or 1).
        unsafe { AXIsProcessTrusted() != 0 }
    }

    fn read_snapshot() -> RawClipboardSnapshot {
        autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            let pasteboard_types: Vec<String> = pb
                .types()
                .map(|arr| arr.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default();

            let frontmost = Self::read_frontmost_app();
            let is_concealed_marker = has_password_manager_marker(&pasteboard_types);
            if is_concealed_marker {
                return RawClipboardSnapshot {
                    pasteboard_types,
                    is_concealed_marker: true,
                    source_app_name: frontmost.as_ref().and_then(|a| a.app_name.clone()),
                    source_app_id: frontmost.as_ref().and_then(|a| a.app_id.clone()),
                    ..Default::default()
                };
            }

            let file_paths = Self::read_file_paths(&pb);
            let plain_text = pb
                .stringForType(unsafe { NSPasteboardTypeString })
                .map(|s| s.to_string());
            let rtf_payload = pb
                .dataForType(unsafe { NSPasteboardTypeRTF })
                .map(|d| String::from_utf8_lossy(&d.to_vec()).into_owned());
            let html_payload = pb
                .stringForType(unsafe { NSPasteboardTypeHTML })
                .map(|s| s.to_string())
                .or_else(|| {
                    pb.dataForType(unsafe { NSPasteboardTypeHTML })
                        .map(|d| String::from_utf8_lossy(&d.to_vec()).into_owned())
                });

            let image_bytes = Self::read_image_png_bytes(&pb);

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
                pasteboard_types,
                is_concealed_marker: false,
                content_hash: None,
            }
        })
    }

    fn read_frontmost_app() -> Option<AppTarget> {
        autoreleasepool(|_| {
            let workspace = NSWorkspace::sharedWorkspace();
            let app = workspace.frontmostApplication()?;
            let app_id = app.bundleIdentifier().map(|s| s.to_string());
            let app_name = app.localizedName().map(|s| s.to_string());
            let pid = app.processIdentifier();
            Some(AppTarget {
                app_id,
                app_name,
                pid: (pid > 0).then_some(pid),
            })
        })
    }

    fn read_file_paths(pb: &NSPasteboard) -> Vec<String> {
        let mut paths = Vec::new();
        let Some(items) = pb.pasteboardItems() else {
            return paths;
        };

        let file_url_type = unsafe { NSPasteboardTypeFileURL };
        for item in items.iter() {
            if let Some(url_str) = item.stringForType(file_url_type) {
                if let Ok(parsed) = url::Url::parse(&url_str.to_string()) {
                    if let Ok(path) = parsed.to_file_path() {
                        paths.push(path.to_string_lossy().into_owned());
                    }
                }
            }
        }
        paths
    }

    fn read_image_png_bytes(pb: &NSPasteboard) -> Option<Vec<u8>> {
        if let Some(png_data) = pb.dataForType(unsafe { NSPasteboardTypePNG }) {
            let bytes = png_data.to_vec();
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }

        let tiff_data = pb.dataForType(unsafe { NSPasteboardTypeTIFF })?;
        if let Some(rep) = NSBitmapImageRep::imageRepWithData(&tiff_data) {
            let empty_props: Retained<NSDictionary<NSBitmapImageRepPropertyKey, AnyObject>> =
                NSDictionary::new();
            // SAFETY: Empty NSDictionary is valid for NSBitmapImageRep PNG encoding.
            if let Some(png_data) = unsafe {
                rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty_props)
            } {
                let bytes = png_data.to_vec();
                if !bytes.is_empty() {
                    return Some(bytes);
                }
            }
        }

        let raw = tiff_data.to_vec();
        (!raw.is_empty()).then_some(raw)
    }

    fn post_key_event(
        source: &CGEventSource,
        keycode: CGKeyCode,
        key_down: bool,
        flags: CGEventFlags,
    ) -> Result<(), ClipoError> {
        let event = CGEvent::new_keyboard_event(Some(source), keycode, key_down).ok_or_else(|| {
            ClipoError::Paste(format!(
                "failed to create macOS CGEvent for keycode {keycode}"
            ))
        })?;
        CGEvent::set_flags(Some(&event), flags);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        Ok(())
    }
}

impl ClipboardBackend for MacosClipboardBackend {
    fn start_watcher(
        &self,
        poll_interval: Duration,
        on_snapshot: SnapshotCallback,
    ) -> Result<ClipboardWatcherHandle, ClipoError> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop_flag);
        let interval = poll_interval.max(Duration::from_millis(50));

        let join_handle = thread::Builder::new()
            .name("clipo-macos-clipboard".to_string())
            .spawn(move || {
                let mut last_change_count =
                    autoreleasepool(|_| NSPasteboard::generalPasteboard().changeCount());

                while !stop_clone.load(Ordering::Relaxed) {
                    let mut remaining = interval;
                    let slice = Duration::from_millis(50);
                    while remaining > Duration::ZERO && !stop_clone.load(Ordering::Relaxed) {
                        let step = remaining.min(slice);
                        thread::sleep(step);
                        remaining = remaining.saturating_sub(step);
                    }
                    if stop_clone.load(Ordering::Relaxed) {
                        break;
                    }

                    let changed = autoreleasepool(|_| {
                        let current = NSPasteboard::generalPasteboard().changeCount();
                        if current != last_change_count {
                            last_change_count = current;
                            true
                        } else {
                            false
                        }
                    });

                    if changed {
                        let snapshot = Self::read_snapshot();
                        on_snapshot(snapshot);
                    }
                }
            })
            .map_err(|err| {
                ClipoError::Clipboard(format!("failed to spawn macOS clipboard watcher: {err}"))
            })?;

        Ok(ClipboardWatcherHandle::from_parts(
            stop_flag,
            None,
            Some(join_handle),
        ))
    }

    fn write_payload(&self, payload: &PastePayload, plain_only: bool) -> Result<(), ClipoError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ClipoError::Clipboard("macOS clipboard lock poisoned".to_string()))?;

        autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            pb.clearContents();

            if plain_only {
                let text = payload.effective_plain_text().unwrap_or_default();
                let item = NSPasteboardItem::new();
                let ns_text = NSString::from_str(&text);
                if !item.setString_forType(&ns_text, unsafe { NSPasteboardTypeString }) {
                    return Err(ClipoError::Clipboard(
                        "failed to stage plain text on NSPasteboardItem".to_string(),
                    ));
                }
                let writable: &ProtocolObject<dyn NSPasteboardWriting> =
                    ProtocolObject::from_ref(&*item);
                let arr = NSArray::from_slice(&[writable]);
                if !pb.writeObjects(&arr) {
                    return Err(ClipoError::Clipboard(
                        "failed to write plain text to NSPasteboard".to_string(),
                    ));
                }
                return Ok(());
            }

            match payload.kind {
                ItemKind::Text => {
                    let item = NSPasteboardItem::new();
                    let mut wrote_any = false;

                    if let Some(ref plain) = payload.plain_text {
                        let ns_plain = NSString::from_str(plain);
                        if item.setString_forType(&ns_plain, unsafe { NSPasteboardTypeString }) {
                            wrote_any = true;
                        }
                    }
                    if let Some(ref html) = payload.html_payload {
                        let ns_html = NSString::from_str(html);
                        if item.setString_forType(&ns_html, unsafe { NSPasteboardTypeHTML }) {
                            wrote_any = true;
                        }
                    }
                    if let Some(ref rtf) = payload.rtf_payload {
                        let data = NSData::with_bytes(rtf.as_bytes());
                        if item.setData_forType(&data, unsafe { NSPasteboardTypeRTF }) {
                            wrote_any = true;
                        }
                    }

                    if !wrote_any {
                        let ns_empty = NSString::from_str("");
                        let _ = item.setString_forType(&ns_empty, unsafe { NSPasteboardTypeString });
                    }

                    let writable: &ProtocolObject<dyn NSPasteboardWriting> =
                        ProtocolObject::from_ref(&*item);
                    let arr = NSArray::from_slice(&[writable]);
                    if !pb.writeObjects(&arr) {
                        return Err(ClipoError::Clipboard(
                            "failed to write text payload to NSPasteboard".to_string(),
                        ));
                    }
                }
                ItemKind::Image => {
                    let bytes = payload.image_bytes.as_deref().ok_or_else(|| {
                        ClipoError::Clipboard("image paste payload missing bytes".to_string())
                    })?;
                    let item = NSPasteboardItem::new();
                    let data = NSData::with_bytes(bytes);
                    let pasteboard_type = if is_tiff_bytes(bytes) {
                        unsafe { NSPasteboardTypeTIFF }
                    } else {
                        unsafe { NSPasteboardTypePNG }
                    };
                    if !item.setData_forType(&data, pasteboard_type) {
                        return Err(ClipoError::Clipboard(
                            "failed to stage image data on NSPasteboardItem".to_string(),
                        ));
                    }
                    let writable: &ProtocolObject<dyn NSPasteboardWriting> =
                        ProtocolObject::from_ref(&*item);
                    let arr = NSArray::from_slice(&[writable]);
                    if !pb.writeObjects(&arr) {
                        return Err(ClipoError::Clipboard(
                            "failed to write image payload to NSPasteboard".to_string(),
                        ));
                    }
                }
                ItemKind::File => {
                    if payload.file_paths.is_empty() {
                        return Err(ClipoError::Clipboard(
                            "file paste payload missing file_paths".to_string(),
                        ));
                    }
                    let mut urls: Vec<Retained<NSURL>> =
                        Vec::with_capacity(payload.file_paths.len());
                    for path in &payload.file_paths {
                        if let Some(url) = NSURL::from_file_path(path) {
                            urls.push(url);
                        }
                    }
                    if urls.is_empty() {
                        return Err(ClipoError::Clipboard(
                            "no valid file URLs could be created for NSPasteboard".to_string(),
                        ));
                    }
                    let writables: Vec<&ProtocolObject<dyn NSPasteboardWriting>> = urls
                        .iter()
                        .map(|u| ProtocolObject::from_ref(&**u))
                        .collect();
                    let arr = NSArray::from_slice(&writables);
                    if !pb.writeObjects(&arr) {
                        return Err(ClipoError::Clipboard(
                            "failed to write file URLs to NSPasteboard".to_string(),
                        ));
                    }
                }
            }

            Ok(())
        })
    }

    fn clear_clipboard(&self) -> Result<(), ClipoError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| ClipoError::Clipboard("macOS clipboard lock poisoned".to_string()))?;

        autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            pb.clearContents();
            Ok(())
        })
    }

    fn current_frontmost_app(&self) -> Option<AppTarget> {
        Self::read_frontmost_app()
    }

    fn activate_app(&self, target: &AppTarget) -> Result<(), ClipoError> {
        autoreleasepool(|_| {
            let running: Option<Retained<NSRunningApplication>> = if let Some(pid) = target.pid {
                NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
            } else if let Some(ref bundle_id) = target.app_id {
                let ns_id = NSString::from_str(bundle_id);
                NSRunningApplication::runningApplicationsWithBundleIdentifier(&ns_id)
                    .iter()
                    .next()
            } else {
                None
            };

            if let Some(app) = running {
                let activated =
                    app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
                if !activated {
                    return Err(ClipoError::Paste(
                        "failed to activate target macOS application".to_string(),
                    ));
                }
                thread::sleep(Duration::from_millis(35));
            }
            Ok(())
        })
    }

    fn send_paste_shortcut(&self, cursor_back_chars: Option<usize>) -> Result<(), ClipoError> {
        if !Self::is_accessibility_trusted() {
            return Err(ClipoError::PermissionDenied(
                "macOS Accessibility permission (AXIsProcessTrusted) is required for auto-paste"
                    .to_string(),
            ));
        }

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok_or_else(|| {
            ClipoError::Paste("failed to create macOS CGEventSource".to_string())
        })?;

        Self::post_key_event(&source, KVK_ANSI_V, true, CGEventFlags::MaskCommand)?;
        Self::post_key_event(&source, KVK_ANSI_V, false, CGEventFlags::MaskCommand)?;

        if let Some(steps) = cursor_back_chars.filter(|&n| n > 0) {
            thread::sleep(Duration::from_millis(15));
            for _ in 0..steps {
                Self::post_key_event(&source, KVK_LEFT_ARROW, true, CGEventFlags::empty())?;
                Self::post_key_event(&source, KVK_LEFT_ARROW, false, CGEventFlags::empty())?;
            }
        }

        Ok(())
    }

    fn capabilities(&self) -> PlatformCapabilities {
        let trusted = Self::is_accessibility_trusted();
        PlatformCapabilities {
            os: "macos".to_string(),
            session_type: "quartz".to_string(),
            can_watch_clipboard: true,
            can_auto_paste: trusted,
            accessibility_granted: Some(trusted),
            notes: (!trusted).then(|| {
                "Accessibility permission not granted; falling back to Copy-only".to_string()
            }),
        }
    }
}

fn is_tiff_bytes(bytes: &[u8]) -> bool {
    bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*")
}
