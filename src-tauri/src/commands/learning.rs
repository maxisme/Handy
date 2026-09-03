use std::sync::Arc;

use log::{debug, info};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::learning::{self, availability, Availability, LearnContext};
use crate::managers::history::{HistoryEntry, HistoryManager, LearnedWord};
use crate::settings::{get_settings, write_settings};

/// Whether learning from corrections can run with the current settings, and
/// if not, why. Drives the toggle's disabled state and its description.
#[tauri::command]
#[specta::specta]
pub fn get_learning_availability(app: AppHandle) -> Availability {
    availability(&get_settings(&app))
}

/// What a saved edit produced: the updated entry and anything learned from it.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct SaveEditResult {
    pub entry: HistoryEntry,
    pub learned: Vec<String>,
    pub batch_id: Option<i64>,
}

/// Learn from one correction and, if anything was learned, add it to
/// `custom_words` and record the batch. Returns the batch id and the words.
async fn learn_and_apply(
    app: &AppHandle,
    original: &str,
    edited: &str,
    source: &str,
    history_id: Option<i64>,
) -> Result<Option<(i64, Vec<String>)>, String> {
    let settings = get_settings(app);
    if !settings.learn_from_corrections {
        return Ok(None);
    }
    let checker = match learning::check::checker(&settings) {
        Ok(checker) => checker,
        Err(reason) => {
            debug!("learning skipped: {reason:?}");
            return Ok(None);
        }
    };
    let ctx = LearnContext {
        custom_words: &settings.custom_words,
        denylist: &settings.learning_denylist,
    };
    let learned = learning::learn(original, edited, &ctx, &checker).await;
    if learned.is_empty() {
        return Ok(None);
    }

    let mut settings = get_settings(app);
    for entry in &learned {
        if !settings.custom_words.contains(&entry.meant) {
            settings.custom_words.push(entry.meant.clone());
        }
    }
    write_settings(app, settings);
    emit_custom_words_changed(app);

    let pairs: Vec<(String, String)> = learned
        .iter()
        .map(|l| (l.heard.clone(), l.meant.clone()))
        .collect();
    let history = app.state::<Arc<HistoryManager>>();
    let batch_id = history
        .record_learned_batch(&pairs, source, history_id)
        .map_err(|e| e.to_string())?;
    let words: Vec<String> = learned.into_iter().map(|l| l.meant).collect();
    info!(
        "learned {} custom word(s) from a {} correction",
        words.len(),
        source
    );
    Ok(Some((batch_id, words)))
}

fn emit_custom_words_changed(app: &AppHandle) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "custom_words" }),
    );
}

/// Remove learned words from `custom_words` and make sure they are never
/// learned automatically again.
fn forget_words(app: &AppHandle, words: &[String]) {
    if words.is_empty() {
        return;
    }
    let mut settings = get_settings(app);
    settings
        .custom_words
        .retain(|w| !words.iter().any(|x| x.eq_ignore_ascii_case(w)));
    for word in words {
        if !settings
            .learning_denylist
            .iter()
            .any(|d| d.eq_ignore_ascii_case(word))
        {
            settings.learning_denylist.push(word.clone());
        }
    }
    write_settings(app, settings);
    emit_custom_words_changed(app);
}

/// Learn custom words from one correction: `original` is what Handy produced,
/// `edited` is what the user changed it to. Returns the words added to
/// `custom_words`, which may be empty. Does nothing unless the setting is on
/// and a post-processing model is ready.
#[tauri::command]
#[specta::specta]
pub async fn learn_from_correction(
    app: AppHandle,
    original: String,
    edited: String,
) -> Result<Vec<String>, String> {
    Ok(learn_and_apply(&app, &original, &edited, "external", None)
        .await?
        .map(|(_, words)| words)
        .unwrap_or_default())
}

/// Store the user's edit of a history entry and learn from the difference
/// between what they saw and what they typed.
#[tauri::command]
#[specta::specta]
pub async fn save_history_edit(
    app: AppHandle,
    id: i64,
    edited_text: String,
) -> Result<SaveEditResult, String> {
    let history = app.state::<Arc<HistoryManager>>();
    let before = history
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {id} not found"))?;
    let original = before
        .edited_text
        .clone()
        .or(before.post_processed_text.clone())
        .unwrap_or(before.transcription_text.clone());
    let entry = history
        .save_edit(id, edited_text.clone())
        .map_err(|e| e.to_string())?;

    let learned = learn_and_apply(&app, &original, &edited_text, "history", Some(id)).await?;
    let (batch_id, words) = match learned {
        Some((batch_id, words)) => (Some(batch_id), words),
        None => (None, Vec::new()),
    };
    Ok(SaveEditResult {
        entry,
        learned: words,
        batch_id,
    })
}

/// Take back everything one batch taught. The words leave `custom_words`
/// and go on the deny list. Returns the words removed.
pub fn undo_batch(app: &AppHandle, batch_id: i64) -> Result<Vec<String>, String> {
    let history = app.state::<Arc<HistoryManager>>();
    let words = history
        .undo_learned_batch(batch_id)
        .map_err(|e| e.to_string())?;
    forget_words(app, &words);
    Ok(words)
}

/// Take back everything one edit taught.
#[tauri::command]
#[specta::specta]
pub async fn undo_learned_batch(app: AppHandle, batch_id: i64) -> Result<Vec<String>, String> {
    undo_batch(&app, batch_id)
}

/// Learn from a correction the user made in another app after a paste.
/// `original` is the text Handy pasted; `edited` is what that span became.
pub async fn learn_from_readback(
    app: &AppHandle,
    original: &str,
    edited: &str,
    history_id: Option<i64>,
) -> Result<Option<(i64, Vec<String>)>, String> {
    learn_and_apply(app, original, edited, "readback", history_id).await
}

/// Learned words still in the dictionary, newest first.
#[tauri::command]
#[specta::specta]
pub async fn get_learned_words(app: AppHandle) -> Result<Vec<LearnedWord>, String> {
    let history = app.state::<Arc<HistoryManager>>();
    history.learned_words().map_err(|e| e.to_string())
}

/// Remove one learned word from the dictionary and never learn it again.
#[tauri::command]
#[specta::specta]
pub async fn remove_learned_word(app: AppHandle, word: String) -> Result<(), String> {
    let history = app.state::<Arc<HistoryManager>>();
    history
        .forget_learned_word(&word)
        .map_err(|e| e.to_string())?;
    forget_words(&app, &[word]);
    Ok(())
}
