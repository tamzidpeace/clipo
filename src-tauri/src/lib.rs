pub mod clipboard;
pub mod commands;
pub mod ipc;
pub mod models;
pub mod paste;
pub mod search;
pub mod snippets;
pub mod storage;
pub mod tray;
pub mod windows_mgr;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
#[cfg(desktop)]
use tauri_plugin_autostart::MacosLauncher;

use crate::clipboard::{
    compute_raw_snapshot_hash, create_platform_backend, evaluate_capture, AppTarget,
    ClipboardBackend, ClipboardWatcherHandle, RawClipboardSnapshot, SelfWriteGuard,
};
use crate::commands::RuntimeStatusDto;
use crate::ipc::{default_ipc_endpoint, IpcCommand, IpcServer};
use crate::models::ClipoError;
use crate::storage::StorageEngine;

/// Shared Tauri-managed application state across commands, tray, IPC server, and watchers.
pub struct AppState {
    pub storage: Arc<StorageEngine>,
    pub backend: Arc<dyn ClipboardBackend>,
    pub write_guard: Arc<SelfWriteGuard>,
    pub paused: Arc<AtomicBool>,
    pub ignore_next: Arc<AtomicBool>,
    pub exiting: Arc<AtomicBool>,
    pub prev_app: Arc<Mutex<Option<AppTarget>>>,
    pub last_popup_blur_at: Mutex<Option<Instant>>,
    pub watcher_handle: Mutex<Option<ClipboardWatcherHandle>>,
    pub ipc_server: Mutex<Option<IpcServer>>,
    pub tray_pause_item: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
    pub registered_shortcut: Mutex<Option<String>>,
}

impl AppState {
    /// Returns whether clipboard capture is currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    /// Sets the clipboard capture pause flag.
    pub fn set_paused_flag(&self, value: bool) {
        self.paused.store(value, Ordering::Release);
    }

    /// Sets whether the next external clipboard change should be skipped.
    pub fn set_ignore_next_flag(&self, value: bool) {
        self.ignore_next.store(value, Ordering::Release);
    }

    /// Returns whether the application is currently shutting down.
    pub fn is_exiting(&self) -> bool {
        self.exiting.load(Ordering::Acquire)
    }

    /// Builds a snapshot of the live runtime status and platform capabilities.
    pub fn runtime_status(&self) -> RuntimeStatusDto {
        RuntimeStatusDto {
            paused: self.is_paused(),
            ignore_next: self.ignore_next.load(Ordering::Acquire),
            capabilities: self.backend.capabilities(),
        }
    }

    /// Updates paused state, persists `advanced.turn_off`, syncs tray, and emits runtime events.
    pub fn apply_paused(
        &self,
        app: &AppHandle,
        paused: bool,
    ) -> Result<RuntimeStatusDto, ClipoError> {
        self.set_paused_flag(paused);
        let mut settings = self.storage.load_settings()?;
        settings.advanced.turn_off = paused;
        self.storage.save_settings(&settings)?;
        tray::sync_tray_state(app);
        let status = self.runtime_status();
        let _ = app.emit("clipo://settings-updated", &settings);
        let _ = app.emit("clipo://runtime-status", &status);
        Ok(status)
    }

    /// Arms single-shot `ignore_next` suppression and emits updated runtime status.
    pub fn trigger_ignore_next(&self, app: &AppHandle) -> RuntimeStatusDto {
        self.set_ignore_next_flag(true);
        let status = self.runtime_status();
        let _ = app.emit("clipo://runtime-status", &status);
        status
    }

    /// Starts or restarts the OS clipboard watcher with `poll_interval`.
    pub fn restart_watcher(
        &self,
        app: &AppHandle,
        poll_interval: Duration,
    ) -> Result<(), ClipoError> {
        let prev = self
            .watcher_handle
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());
        if let Some(prev) = prev {
            prev.stop();
        }
        if self.is_exiting() {
            return Ok(());
        }

        let storage = Arc::clone(&self.storage);
        let write_guard = Arc::clone(&self.write_guard);
        let paused = Arc::clone(&self.paused);
        let ignore_next = Arc::clone(&self.ignore_next);
        let app_handle = app.clone();

        let callback = Arc::new(move |raw: RawClipboardSnapshot| {
            handle_clipboard_snapshot(
                &app_handle,
                &storage,
                &write_guard,
                &paused,
                &ignore_next,
                raw,
            );
        });

        let handle = self.backend.start_watcher(poll_interval, callback)?;
        if let Ok(mut guard) = self.watcher_handle.lock() {
            *guard = Some(handle);
        }
        Ok(())
    }

    /// Synchronizes the global `Open` hotkey on desktop platforms.
    pub fn sync_open_shortcut(&self, app: &AppHandle, shortcut: &str) -> Result<(), ClipoError> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

            let manager = app.global_shortcut();
            let prev_registered = self
                .registered_shortcut
                .lock()
                .ok()
                .and_then(|g| g.clone());

            let _ = manager.unregister_all();

            let trimmed = shortcut.trim();
            if trimmed.is_empty() {
                if let Ok(mut guard) = self.registered_shortcut.lock() {
                    *guard = None;
                }
                return Ok(());
            }

            if let Err(err) = manager.on_shortcut(trimmed, move |app_handle, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = windows_mgr::toggle_popup(app_handle);
                }
            }) {
                if let Some(prev) = prev_registered {
                    let _ = manager.on_shortcut(
                        prev.as_str(),
                        move |app_handle, _shortcut, event| {
                            if event.state == ShortcutState::Pressed {
                                let _ = windows_mgr::toggle_popup(app_handle);
                            }
                        },
                    );
                }
                return Err(ClipoError::Validation(format!(
                    "failed to register global shortcut `{trimmed}`: {err}"
                )));
            }

            if let Ok(mut guard) = self.registered_shortcut.lock() {
                *guard = Some(trimmed.to_string());
            }
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let _ = (app, shortcut);

        Ok(())
    }

    /// Cleanly stops background watcher threads and IPC socket server on exit.
    pub fn cleanup_on_exit(&self) {
        self.exiting.store(true, Ordering::Release);
        let watcher = self
            .watcher_handle
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());
        if let Some(handle) = watcher {
            handle.stop();
        }
        let server = self
            .ipc_server
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());
        drop(server);
    }
}

fn handle_clipboard_snapshot(
    app: &AppHandle,
    storage: &StorageEngine,
    write_guard: &SelfWriteGuard,
    paused: &AtomicBool,
    ignore_next: &AtomicBool,
    raw: RawClipboardSnapshot,
) {
    if app.state::<AppState>().is_exiting() || paused.load(Ordering::Acquire) {
        return;
    }

    // Do not let Clipo's own paste writes consume `ignore_next`.
    if let Some(ref precomputed) = raw.content_hash {
        if write_guard.is_suppressed(precomputed) {
            return;
        }
    }
    if let Some(canonical_hash) = compute_raw_snapshot_hash(&raw) {
        if write_guard.is_suppressed(&canonical_hash) {
            return;
        }
    }

    if ignore_next.swap(false, Ordering::AcqRel) {
        let state = app.state::<AppState>();
        let _ = app.emit("clipo://runtime-status", state.runtime_status());
        return;
    }

    let settings = storage.load_settings().unwrap_or_default();
    let rules = storage.list_ignore_rules().unwrap_or_default();

    let Some(captured) = evaluate_capture(raw, &rules, &settings, write_guard) else {
        return;
    };

    if storage
        .insert_or_bump_capture(
            captured,
            settings.storage.max_items,
            settings.storage.sort_by,
        )
        .is_ok()
    {
        tray::sync_tray_state(app);
        let _ = app.emit("clipo://history-changed", ());
    }
}

fn handle_ipc_command(app: &AppHandle, cmd: IpcCommand) {
    let state = app.state::<AppState>();
    if state.is_exiting() {
        return;
    }
    match cmd {
        IpcCommand::Pause | IpcCommand::Resume => {
            let paused = matches!(cmd, IpcCommand::Pause);
            let _ = state.apply_paused(app, paused);
        }
        IpcCommand::IgnoreNext => {
            let _ = state.trigger_ignore_next(app);
        }
        IpcCommand::TogglePopup => {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = windows_mgr::toggle_popup(&handle);
            });
        }
    }
}

/// Starts the Tauri Clipo desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().register_uri_scheme_protocol("clipoimg", |ctx, request| {
        let state = ctx.app_handle().state::<AppState>();
        let uri = request.uri().to_string();
        windows_mgr::resolve_clipoimg_blob(state.storage.blobs_dir(), &uri).into_http_response()
    });

    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());

    let app = builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("failed to resolve app_data_dir: {e}"))?;
            let storage = Arc::new(StorageEngine::open(&data_dir)?);
            let settings = storage.load_settings().unwrap_or_default();

            let backend: Arc<dyn ClipboardBackend> = Arc::from(create_platform_backend());
            let write_guard = Arc::new(SelfWriteGuard::new());
            let paused = Arc::new(AtomicBool::new(settings.advanced.turn_off));
            let ignore_next = Arc::new(AtomicBool::new(false));
            let exiting = Arc::new(AtomicBool::new(false));

            let state = AppState {
                storage,
                backend,
                write_guard,
                paused,
                ignore_next,
                exiting,
                prev_app: Arc::new(Mutex::new(None)),
                last_popup_blur_at: Mutex::new(None),
                watcher_handle: Mutex::new(None),
                ipc_server: Mutex::new(None),
                tray_pause_item: Mutex::new(None),
                registered_shortcut: Mutex::new(None),
            };
            app.manage(state);

            let app_handle = app.handle().clone();
            let managed = app.state::<AppState>();

            if let Err(err) = tray::setup_tray(&app_handle) {
                eprintln!("clipo: tray setup warning: {err}");
            }

            if let Err(err) = managed.sync_open_shortcut(&app_handle, &settings.general.open_shortcut) {
                eprintln!("clipo: global shortcut registration warning: {err}");
            }

            let poll_interval = Duration::from_millis(settings.advanced.poll_interval_ms.max(50));
            if let Err(err) = managed.restart_watcher(&app_handle, poll_interval) {
                eprintln!("clipo: clipboard watcher startup warning: {err}");
            }

            let ipc_handle = app_handle.clone();
            let endpoint = default_ipc_endpoint();
            match IpcServer::bind(
                &endpoint,
                Arc::new(move |cmd| handle_ipc_command(&ipc_handle, cmd)),
            ) {
                Ok(server) => {
                    if let Ok(mut guard) = managed.ipc_server.lock() {
                        *guard = Some(server);
                    }
                }
                Err(err) => {
                    eprintln!(
                        "clipo: IPC server bind warning at {}: {err}",
                        endpoint.display()
                    );
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match (window.label(), event) {
            (windows_mgr::POPUP_LABEL, WindowEvent::Focused(false)) => {
                windows_mgr::on_popup_focus_lost(window.app_handle());
            }
            (windows_mgr::SETTINGS_LABEL, WindowEvent::Destroyed) => {
                windows_mgr::on_settings_destroyed(window.app_handle());
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::search_items,
            commands::select_item,
            commands::get_item_detail,
            commands::toggle_pin,
            commands::delete_item,
            commands::clear_unpinned,
            commands::save_item_as_snippet,
            commands::list_snippets,
            commands::upsert_snippet,
            commands::delete_snippet,
            commands::export_snippets,
            commands::import_snippets,
            commands::get_settings,
            commands::update_settings,
            commands::get_ignore_rules,
            commands::upsert_ignore_rule,
            commands::delete_ignore_rule,
            commands::get_storage_stats,
            commands::set_paused,
            commands::set_ignore_next,
            commands::get_runtime_status,
            commands::open_settings_window,
            commands::hide_popup,
            commands::get_platform_capabilities,
            commands::open_accessibility_settings,
            commands::check_for_updates,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Clipo application");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            let state = app_handle.state::<AppState>();
            // Stop watcher & IPC server before exit cleanup so clearing clipboard cannot re-trigger capture.
            state.cleanup_on_exit();
            if let Ok(settings) = state.storage.load_settings() {
                if settings.advanced.clear_history_on_quit {
                    let _ = state.storage.clear_unpinned_history();
                }
                if settings.advanced.clear_system_clipboard_on_quit {
                    let _ = state.backend.clear_clipboard();
                }
            }
        }
    });
}
