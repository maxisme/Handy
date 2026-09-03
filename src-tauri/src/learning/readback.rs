//! Learning from corrections made in the app Handy pasted into.
//!
//! Right after a paste lands, the focused text field is captured through the
//! accessibility layer along with the text before and after the pasted span.
//! The field is polled until focus leaves it, a new recording starts, or a
//! time limit passes; the span between the same anchors is then diffed against
//! what was pasted, and the learning engine decides what, if anything, to add.
//! Text outside the anchors is never inspected. Every failure is silent: the
//! user asked for dictation, not for a lecture about accessibility.

/// What surrounded the pasted text when it was captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteSnapshot {
    pub prefix: String,
    pub pasted: String,
    pub suffix: String,
}

/// Byte offset in `text` of the character at UTF-16 offset `utf16`, clamped
/// to the end of the string.
fn byte_index_for_utf16(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= utf16 {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// Locate `pasted` in `value`. When the caret position is known the occurrence
/// ending nearest before it wins, since the caret sits at the end of a fresh
/// paste; otherwise the last occurrence. A trailing-space variant of the paste
/// is tried too, because Handy can append one on paste.
pub fn snapshot(value: &str, pasted: &str, caret_utf16: Option<usize>) -> Option<PasteSnapshot> {
    let needle = if value.contains(pasted) {
        pasted
    } else {
        pasted.trim_end()
    };
    if needle.is_empty() {
        return None;
    }
    let caret = caret_utf16.map(|c| byte_index_for_utf16(value, c));
    let mut best: Option<usize> = None;
    let mut from = 0;
    while let Some(pos) = value[from..].find(needle) {
        let start = from + pos;
        let end = start + needle.len();
        match caret {
            Some(c) if end <= c => best = Some(start),
            Some(_) => {
                if best.is_none() {
                    best = Some(start);
                }
                break;
            }
            None => best = Some(start),
        }
        from = end;
    }
    let start = best?;
    let end = start + needle.len();
    Some(PasteSnapshot {
        prefix: value[..start].to_string(),
        pasted: needle.to_string(),
        suffix: value[end..].to_string(),
    })
}

/// The text now sitting between the snapshot's anchors, or `None` when the
/// anchors no longer match: the user edited outside the pasted span, and
/// nothing inside it can be trusted.
pub fn current_span(value: &str, snap: &PasteSnapshot) -> Option<String> {
    let inner = value.strip_prefix(snap.prefix.as_str())?;
    let inner = inner.strip_suffix(snap.suffix.as_str())?;
    Some(inner.to_string())
}

/// Largest field the read-back will look at.
pub const MAX_FIELD_BYTES: usize = 200_000;

#[cfg(target_os = "macos")]
pub use session::{finish_now, start};

#[cfg(not(target_os = "macos"))]
pub fn start(_app: &tauri::AppHandle, _pasted: String, _history_id: Option<i64>) {}

#[cfg(not(target_os = "macos"))]
pub fn finish_now() {}

#[cfg(target_os = "macos")]
mod session {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use log::{debug, info};
    use tauri::AppHandle;

    use super::{current_span, snapshot, MAX_FIELD_BYTES};
    use crate::commands::learning::learn_from_readback;
    use crate::focus::FocusedTextField;
    use crate::learning::{availability, toast, Availability};
    use crate::secure_input;
    use crate::settings::{get_settings, OverlayStyle};

    /// Time for the paste to land before the field is read.
    const SETTLE: Duration = Duration::from_millis(400);
    /// How often the field is checked for a focus change.
    const POLL: Duration = Duration::from_secs(2);
    /// Longest a session waits for focus to leave the field.
    const LIMIT: Duration = Duration::from_secs(90);

    /// Stop flag of the session in progress, if any.
    static ACTIVE: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

    fn replace_active(flag: Option<Arc<AtomicBool>>) {
        if let Ok(mut slot) = ACTIVE.lock() {
            if let Some(previous) = slot.replace(flag.unwrap_or_default()) {
                previous.store(true, Ordering::SeqCst);
            }
        }
    }

    /// Ask the running session, if any, to take its final reading now. Called
    /// when a new recording starts so the toast never competes with it.
    pub fn finish_now() {
        replace_active(None);
    }

    /// Begin watching the focused field for corrections to `pasted`.
    pub fn start(app: &AppHandle, pasted: String, history_id: Option<i64>) {
        let settings = get_settings(app);
        if !settings.learn_from_corrections || !settings.auto_learn_from_apps {
            return;
        }
        if settings.overlay_style == OverlayStyle::None {
            debug!("readback: overlay is off, not watching");
            return;
        }
        if !matches!(availability(&settings), Availability::Ready { .. }) {
            debug!("readback: no model ready, not watching");
            return;
        }
        if secure_input::is_enabled_now() {
            debug!("readback: secure input is on, not watching");
            return;
        }
        let denylist = settings.auto_learn_app_denylist.clone();
        let stop = Arc::new(AtomicBool::new(false));
        replace_active(Some(Arc::clone(&stop)));
        let app = app.clone();
        std::thread::Builder::new()
            .name("readback".into())
            .spawn(move || run(app, pasted, history_id, denylist, stop))
            .ok();
    }

    fn run(
        app: AppHandle,
        pasted: String,
        history_id: Option<i64>,
        denylist: Vec<String>,
        stop: Arc<AtomicBool>,
    ) {
        std::thread::sleep(SETTLE);
        let Some(field) = FocusedTextField::capture() else {
            debug!("readback: no readable text field has focus");
            return;
        };
        if let Some(bundle) = field.bundle_id() {
            if denylist.iter().any(|d| d.eq_ignore_ascii_case(&bundle)) {
                debug!("readback: {bundle} is excluded");
                return;
            }
        }
        let Some(value) = field.value() else {
            debug!("readback: field value unreadable");
            return;
        };
        if value.len() > MAX_FIELD_BYTES {
            debug!("readback: field too large ({} bytes)", value.len());
            return;
        }
        let caret = field.selection_utf16().map(|(loc, len)| loc + len);
        let Some(snap) = snapshot(&value, &pasted, caret) else {
            debug!("readback: pasted text not found in the field");
            return;
        };
        debug!(
            "readback: watching a {}-char span in {} (pid {})",
            snap.pasted.chars().count(),
            field.bundle_id().unwrap_or_default(),
            field.pid().unwrap_or_default()
        );

        let started = Instant::now();
        while started.elapsed() < LIMIT && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(POLL);
            if !field.is_focused() {
                debug!("readback: focus left the field");
                break;
            }
        }

        let Some(value) = field.value() else {
            debug!("readback: field gone before the final reading");
            return;
        };
        let Some(edited) = current_span(&value, &snap) else {
            debug!("readback: text outside the pasted span changed, ignoring");
            return;
        };
        if edited.trim() == snap.pasted.trim() {
            debug!("readback: span unchanged");
            return;
        }
        let original = snap.pasted.clone();
        tauri::async_runtime::spawn(async move {
            match learn_from_readback(&app, &original, &edited, history_id).await {
                Ok(Some((batch_id, words))) => {
                    info!("readback: learned {} word(s)", words.len());
                    toast::offer(&app, batch_id, words);
                }
                Ok(None) => debug!("readback: nothing learned"),
                Err(err) => debug!("readback: learning failed: {err}"),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_anchors_around_the_paste_at_the_caret() {
        let value = "Intro. We moved billing to Charge B last week. Outro.";
        let pasted = "We moved billing to Charge B last week.";
        let caret = "Intro. We moved billing to Charge B last week."
            .encode_utf16()
            .count();
        let snap = snapshot(value, pasted, Some(caret)).unwrap();
        assert_eq!(snap.prefix, "Intro. ");
        assert_eq!(snap.pasted, pasted);
        assert_eq!(snap.suffix, " Outro.");
    }

    #[test]
    fn snapshot_prefers_the_occurrence_ending_at_the_caret() {
        let value = "hello hello hello";
        let caret = "hello hello".encode_utf16().count();
        let snap = snapshot(value, "hello", Some(caret)).unwrap();
        assert_eq!(snap.prefix, "hello ");
        assert_eq!(snap.suffix, " hello");
        let snap = snapshot(value, "hello", None).unwrap();
        assert_eq!(snap.prefix, "hello hello ");
    }

    #[test]
    fn snapshot_tolerates_a_trailing_space_that_did_not_land() {
        let snap = snapshot("Say ChargeBee", "Say ChargeBee ", None).unwrap();
        assert_eq!(snap.pasted, "Say ChargeBee");
        assert!(snapshot("nothing here", "absent", None).is_none());
    }

    #[test]
    fn snapshot_handles_multibyte_text_before_the_caret() {
        let value = "Résumé — naïve café: Charge B now";
        let caret = value.encode_utf16().count();
        let snap = snapshot(value, "Charge B now", Some(caret)).unwrap();
        assert_eq!(snap.prefix, "Résumé — naïve café: ");
    }

    #[test]
    fn current_span_follows_edits_inside_the_anchors_only() {
        let snap = snapshot("A. Charge B here. Z.", "Charge B here.", None).unwrap();
        assert_eq!(
            current_span("A. ChargeBee here. Z.", &snap).as_deref(),
            Some("ChargeBee here.")
        );
        assert_eq!(
            current_span("A. ChargeBee here. Z. More typed.", &snap),
            None
        );
        assert_eq!(current_span("Changed. ChargeBee here. Z.", &snap), None);
        assert_eq!(current_span("A.  Z.", &snap).as_deref(), Some(""));
        assert_eq!(current_span("A. Z.", &snap), None);
    }

    #[test]
    fn current_span_with_an_empty_suffix_includes_text_typed_after() {
        let snap = snapshot("A. Charge B here.", "Charge B here.", None).unwrap();
        assert_eq!(
            current_span("A. ChargeBee here. And more.", &snap).as_deref(),
            Some("ChargeBee here. And more.")
        );
    }
}
