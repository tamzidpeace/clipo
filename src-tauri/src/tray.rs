use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::models::{ClipoError, SortBy};
use crate::{windows_mgr, AppState};

/// Identifier of the primary system tray icon.
pub const MAIN_TRAY_ID: &str = "main-tray";

const MENU_PAUSE_ID: &str = "tray_toggle_pause";
const MENU_IGNORE_NEXT_ID: &str = "tray_ignore_next";
const MENU_CLEAR_HISTORY_ID: &str = "tray_clear_history";
const MENU_PREFERENCES_ID: &str = "tray_preferences";
const MENU_CHECK_UPDATES_ID: &str = "tray_check_updates";
const MENU_QUIT_ID: &str = "tray_quit";

const MAX_TRAY_TITLE_CHARS: usize = 24;

/// Creates and registers the system tray icon, menu, and click/menu event handlers.
pub fn setup_tray(app: &AppHandle) -> Result<(), ClipoError> {
    let state = app.state::<AppState>();
    let initial_paused = state.is_paused();
    let pause_label = if initial_paused {
        "Resume Capture"
    } else {
        "Pause Capture"
    };

    let pause_item = MenuItem::with_id(app, MENU_PAUSE_ID, pause_label, true, None::<&str>)
        .map_err(|e| ClipoError::Internal(format!("failed to create pause menu item: {e}")))?;
    let ignore_next_item = MenuItem::with_id(
        app,
        MENU_IGNORE_NEXT_ID,
        "Ignore Next Copy",
        true,
        None::<&str>,
    )
    .map_err(|e| ClipoError::Internal(format!("failed to create ignore-next menu item: {e}")))?;
    let clear_item =
        MenuItem::with_id(app, MENU_CLEAR_HISTORY_ID, "Clear History", true, None::<&str>)
            .map_err(|e| ClipoError::Internal(format!("failed to create clear menu item: {e}")))?;
    let prefs_item =
        MenuItem::with_id(app, MENU_PREFERENCES_ID, "Preferences...", true, None::<&str>)
            .map_err(|e| ClipoError::Internal(format!("failed to create prefs menu item: {e}")))?;
    let updates_item = MenuItem::with_id(
        app,
        MENU_CHECK_UPDATES_ID,
        "Check for Updates...",
        true,
        None::<&str>,
    )
    .map_err(|e| ClipoError::Internal(format!("failed to create updates menu item: {e}")))?;
    let quit_item = MenuItem::with_id(app, MENU_QUIT_ID, "Quit Clipo", true, None::<&str>)
        .map_err(|e| ClipoError::Internal(format!("failed to create quit menu item: {e}")))?;

    let sep1 = PredefinedMenuItem::separator(app)
        .map_err(|e| ClipoError::Internal(format!("failed to create separator: {e}")))?;
    let sep2 = PredefinedMenuItem::separator(app)
        .map_err(|e| ClipoError::Internal(format!("failed to create separator: {e}")))?;
    let sep3 = PredefinedMenuItem::separator(app)
        .map_err(|e| ClipoError::Internal(format!("failed to create separator: {e}")))?;

    let menu = Menu::with_items(
        app,
        &[
            &pause_item,
            &ignore_next_item,
            &sep1,
            &clear_item,
            &sep2,
            &prefs_item,
            &updates_item,
            &sep3,
            &quit_item,
        ],
    )
    .map_err(|e| ClipoError::Internal(format!("failed to build tray menu: {e}")))?;

    if let Ok(mut guard) = state.tray_pause_item.lock() {
        *guard = Some(pause_item);
    }

    let mut builder = TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .menu(&menu)
        .tooltip("Clipo")
        .show_menu_on_left_click(false)
        .on_menu_event(|app_handle, event| {
            handle_menu_event(app_handle, event.id().as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                handle_left_click(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
        #[cfg(target_os = "macos")]
        {
            builder = builder.icon_as_template(true);
        }
    }

    builder
        .build(app)
        .map_err(|e| ClipoError::Internal(format!("failed to create system tray icon: {e}")))?;

    sync_tray_state(app);
    Ok(())
}

/// Synchronizes tray visibility, menu text, and optional recent-copy title from current state.
pub fn sync_tray_state(app: &AppHandle) {
    let state = app.state::<AppState>();
    let paused = state.is_paused();
    let settings = state.storage.load_settings().unwrap_or_default();

    if let Ok(guard) = state.tray_pause_item.lock() {
        if let Some(ref item) = *guard {
            let label = if paused {
                "Resume Capture"
            } else {
                "Pause Capture"
            };
            let _ = item.set_text(label);
        }
    }

    let Some(tray) = app.tray_by_id(MAIN_TRAY_ID) else {
        return;
    };

    let _ = tray.set_visible(settings.appearance.show_menu_icon);

    if settings.appearance.show_recent_copy_next_to_menu_icon {
        let recent_title = state
            .storage
            .list_history(SortBy::LastCopied, 100)
            .ok()
            .and_then(|items| items.into_iter().max_by_key(|item| item.last_copied_at))
            .map(|item| truncate_tray_title(&item.title_preview));
        let _ = tray.set_title(recent_title);
    } else {
        let _ = tray.set_title(None::<&str>);
    }
}

fn handle_left_click(app: &AppHandle) {
    let (alt, shift) = current_modifier_keys();
    let state = app.state::<AppState>();

    if alt && shift {
        let _ = state.trigger_ignore_next(app);
    } else if alt {
        let _ = state.apply_paused(app, !state.is_paused());
    } else {
        let _ = windows_mgr::toggle_popup(app);
    }
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    match id {
        MENU_PAUSE_ID => {
            let _ = state.apply_paused(app, !state.is_paused());
        }
        MENU_IGNORE_NEXT_ID => {
            let _ = state.trigger_ignore_next(app);
        }
        MENU_CLEAR_HISTORY_ID => {
            if state.storage.clear_unpinned_history().is_ok() {
                sync_tray_state(app);
                let _ = app.emit("clipo://history-changed", ());
            }
        }
        MENU_PREFERENCES_ID => {
            let _ = windows_mgr::open_settings_window(app, Some("general".to_string()));
        }
        MENU_CHECK_UPDATES_ID => {
            let _ = windows_mgr::open_settings_window(app, Some("general".to_string()));
            let _ = app.emit("clipo://check-updates", ());
        }
        MENU_QUIT_ID => {
            app.exit(0);
        }
        _ => {}
    }
}

fn truncate_tray_title(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.chars().count() <= MAX_TRAY_TITLE_CHARS {
        trimmed.to_string()
    } else {
        let prefix: String = trimmed.chars().take(MAX_TRAY_TITLE_CHARS - 1).collect();
        format!("{prefix}…")
    }
}

/// Reads current `(alt_pressed, shift_pressed)` modifier state for Maccy tray modifier clicks.
fn current_modifier_keys() -> (bool, bool) {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::{CGEventFlags, CGEventSource, CGEventSourceStateID};
        let flags = CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState);
        let alt = flags.contains(CGEventFlags::MaskAlternate);
        let shift = flags.contains(CGEventFlags::MaskShift);
        (alt, shift)
    }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_MENU, VK_SHIFT,
        };
        // SAFETY: `GetAsyncKeyState` takes a virtual key code and has no pointer preconditions.
        let alt = unsafe { GetAsyncKeyState(i32::from(VK_MENU)) } < 0;
        // SAFETY: `GetAsyncKeyState` takes a virtual key code and has no pointer preconditions.
        let shift = unsafe { GetAsyncKeyState(i32::from(VK_SHIFT)) } < 0;
        (alt, shift)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        (false, false)
    }
}
