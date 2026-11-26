use serde::{Deserialize, Serialize};

use crate::clipboard::{AppTarget, ClipboardBackend, PastePayload, SelfWriteGuard};
use crate::models::ClipoError;

/// User-selected or shortcut-overridden copy/paste execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteActionMode {
    CopyOnly,
    CopyAndPasteRich,
    CopyAndPastePlain,
}

/// Result of executing a copy or copy-and-paste action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasteOutcome {
    pub pasted: bool,
    pub copied: bool,
    pub degraded_reason: Option<String>,
}

/// Orchestrates self-write suppression, clipboard writes, focus restoration, and synthetic paste.
pub struct PasteEngine;

impl PasteEngine {
    /// Executes a copy or copy-and-paste operation against `backend`.
    ///
    /// - Records payload hashes in `write_guard` before writing so watchers suppress re-capture.
    /// - Strips rich formats when `mode == PasteActionMode::CopyAndPastePlain`.
    /// - Falls back gracefully to Copy-only (`pasted: false, degraded_reason: Some(...)`) when
    ///   `backend.capabilities().can_auto_paste` is `false`.
    pub fn execute(
        backend: &dyn ClipboardBackend,
        write_guard: &SelfWriteGuard,
        payload: &PastePayload,
        mode: PasteActionMode,
        prev_app: Option<&AppTarget>,
        cursor_back_chars: Option<usize>,
    ) -> Result<PasteOutcome, ClipoError> {
        write_guard.record_payload(payload);

        let plain_only = matches!(mode, PasteActionMode::CopyAndPastePlain);
        backend.write_payload(payload, plain_only)?;

        if mode == PasteActionMode::CopyOnly {
            return Ok(PasteOutcome {
                pasted: false,
                copied: true,
                degraded_reason: None,
            });
        }

        let caps = backend.capabilities();
        if !caps.can_auto_paste {
            return Ok(PasteOutcome {
                pasted: false,
                copied: true,
                degraded_reason: Some(caps.notes.unwrap_or_else(|| {
                    "auto-paste is unavailable on this platform or permission state".to_string()
                })),
            });
        }

        if let Some(target) = prev_app {
            backend.activate_app(target)?;
        }

        backend.send_paste_shortcut(cursor_back_chars)?;

        Ok(PasteOutcome {
            pasted: true,
            copied: true,
            degraded_reason: None,
        })
    }
}
