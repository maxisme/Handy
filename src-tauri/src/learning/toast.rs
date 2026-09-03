//! "Learned words" toast on the recording overlay.
//!
//! When a correction the user made in another app teaches Handy new words,
//! the overlay shows a small pill naming what was learned, with an Undo
//! button and a countdown. Undo takes the whole batch back the same way the
//! History toast does.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::overlay;

/// How long the toast stays on screen before the frontend dismisses it.
/// The frontend owns the real countdown so hovering the pill can pause it.
pub const TOAST_TIMEOUT: Duration = Duration::from_secs(8);

/// Backstop hide scheduled by the backend in case the frontend never
/// dismisses (for example while the pill is left hovered).
pub const SAFETY_HIDE: Duration = Duration::from_secs(60);

/// How long the "Undone" confirmation stays visible after a click.
const UNDONE_LINGER: Duration = Duration::from_millis(900);

/// The batch the toast offers to undo: `(batch_id, words)`. Managed Tauri
/// state; written right before the toast is shown and taken by the undo
/// command.
#[derive(Default)]
pub struct PendingBatch(Mutex<Option<(i64, Vec<String>)>>);

/// Stores the batch and shows the toast on the overlay.
pub fn offer(app: &AppHandle, batch_id: i64, words: Vec<String>) {
    match app.try_state::<PendingBatch>() {
        Some(state) => match state.0.lock() {
            Ok(mut slot) => *slot = Some((batch_id, words.clone())),
            Err(err) => {
                log::error!("learned toast: batch slot poisoned: {err}");
                return;
            }
        },
        None => {
            log::error!("learned toast: PendingBatch state not managed");
            return;
        }
    }
    overlay::show_learned_overlay(app, batch_id, &words, TOAST_TIMEOUT);
}

/// Overlay Undo button: takes back the pending batch and lets the toast
/// linger briefly so the "Undone" confirmation is visible. Returns the words
/// removed.
#[tauri::command]
#[specta::specta]
pub fn undo_learned_toast(app: AppHandle) -> Result<Vec<String>, String> {
    let state = app
        .try_state::<PendingBatch>()
        .ok_or("PendingBatch state not managed")?;
    let (batch_id, _) = state
        .0
        .lock()
        .map_err(|err| format!("batch slot poisoned: {err}"))?
        .take()
        .ok_or("no learned batch to undo")?;
    let words = crate::commands::learning::undo_batch(&app, batch_id)?;
    overlay::hide_overlay_after(&app, UNDONE_LINGER);
    Ok(words)
}

/// Overlay close button or countdown end: drops the pending batch and hides
/// the toast without undoing.
#[tauri::command]
#[specta::specta]
pub fn dismiss_learned_toast(app: AppHandle) {
    if let Some(state) = app.try_state::<PendingBatch>() {
        match state.0.lock() {
            Ok(mut slot) => *slot = None,
            Err(err) => log::error!("learned toast: batch slot poisoned: {err}"),
        }
    }
    overlay::hide_recording_overlay(&app);
}
