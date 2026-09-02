//! "Copy last transcript" prompt.
//!
//! A transcript is normally pasted into whatever has keyboard focus. When
//! nothing editable is focused (the user was on the desktop, a file browser,
//! a read-only page) the paste lands nowhere and the clipboard is restored,
//! so the text is gone from view. In that case the overlay shows a small
//! prompt for a few seconds offering to copy the transcript to the clipboard.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::clipboard::write_text_to_clipboard;
use crate::overlay;
use crate::settings::{AppSettings, ClipboardHandling, PasteMethod};

/// How long the prompt stays on screen before hiding on its own.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(8);

/// How long the "Copied" confirmation stays visible after a click.
const COPIED_LINGER: Duration = Duration::from_millis(900);

/// The transcript the prompt offers. Managed Tauri state; written right
/// before the prompt is shown and read by the copy command.
#[derive(Default)]
pub struct LastTranscript(Mutex<Option<String>>);

/// Whether to show the prompt after a paste attempt.
///
/// `target_is_text_input` is the focus check taken just before pasting
/// (`None` when the platform could not tell). A failed paste always earns the
/// prompt: the text went nowhere regardless of focus. The prompt is shown
/// even when clipboard handling already copies the transcript: it is the
/// only visible sign that nothing was pasted, and copying again is harmless.
pub fn should_offer_copy(
    settings: &AppSettings,
    target_is_text_input: Option<bool>,
    paste_failed: bool,
) -> bool {
    if !settings.copy_prompt_enabled {
        return false;
    }
    // These methods never target the focused element.
    if matches!(
        settings.paste_method,
        PasteMethod::None | PasteMethod::ExternalScript
    ) {
        return false;
    }
    paste_failed || target_is_text_input == Some(false)
}

/// Stores `text` as the offered transcript and shows the prompt on the
/// overlay, which hides itself after `PROMPT_TIMEOUT`.
pub fn offer(app: &AppHandle, text: String) {
    match app.try_state::<LastTranscript>() {
        Some(state) => match state.0.lock() {
            Ok(mut slot) => *slot = Some(text),
            Err(err) => {
                log::error!("copy prompt: transcript slot poisoned: {err}");
                return;
            }
        },
        None => {
            log::error!("copy prompt: LastTranscript state not managed");
            return;
        }
    }
    overlay::show_copy_prompt_overlay(app, PROMPT_TIMEOUT);
}

/// Copies the offered transcript to the clipboard (persistently, unlike the
/// paste path which restores the previous clipboard) and lets the prompt
/// linger briefly so the "Copied" confirmation is visible.
pub fn copy_last_transcript(app: &AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<LastTranscript>()
        .ok_or("LastTranscript state not managed")?;
    let text = state
        .0
        .lock()
        .map_err(|err| format!("transcript slot poisoned: {err}"))?
        .clone()
        .ok_or("no transcript to copy")?;
    write_text_to_clipboard(app, &text)?;
    overlay::hide_overlay_after(app, COPIED_LINGER);
    Ok(())
}

/// Hides the prompt without copying.
pub fn dismiss(app: &AppHandle) {
    overlay::hide_recording_overlay(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn settings() -> AppSettings {
        let mut settings = get_default_settings();
        settings.copy_prompt_enabled = true;
        settings.paste_method = PasteMethod::CtrlV;
        settings.clipboard_handling = ClipboardHandling::DontModify;
        settings
    }

    #[test]
    fn offers_when_focus_is_not_a_text_input() {
        assert!(should_offer_copy(&settings(), Some(false), false));
    }

    #[test]
    fn stays_quiet_when_focus_is_a_text_input_or_unknown() {
        assert!(!should_offer_copy(&settings(), Some(true), false));
        assert!(!should_offer_copy(&settings(), None, false));
    }

    #[test]
    fn offers_after_a_failed_paste_regardless_of_focus() {
        assert!(should_offer_copy(&settings(), Some(true), true));
        assert!(should_offer_copy(&settings(), None, true));
    }

    #[test]
    fn respects_the_setting() {
        let mut settings = settings();
        settings.copy_prompt_enabled = false;
        assert!(!should_offer_copy(&settings, Some(false), true));
    }

    #[test]
    fn offers_even_when_clipboard_handling_already_copies() {
        let mut settings = settings();
        settings.clipboard_handling = ClipboardHandling::CopyToClipboard;
        assert!(should_offer_copy(&settings, Some(false), false));
    }

    #[test]
    fn skips_paste_methods_that_do_not_target_the_focused_element() {
        for method in [PasteMethod::None, PasteMethod::ExternalScript] {
            let mut settings = settings();
            settings.paste_method = method;
            assert!(
                !should_offer_copy(&settings, Some(false), true),
                "{method:?}"
            );
        }
    }

    #[test]
    fn direct_typing_still_offers() {
        let mut settings = settings();
        settings.paste_method = PasteMethod::Direct;
        assert!(should_offer_copy(&settings, Some(false), false));
    }
}
