use log::{debug, info};
use tauri::{AppHandle, Emitter};

use crate::learning::{self, availability, Availability, LearnContext};
use crate::settings::{get_settings, write_settings};

/// Whether learning from corrections can run with the current settings, and
/// if not, why. Drives the toggle's disabled state and its description.
#[tauri::command]
#[specta::specta]
pub fn get_learning_availability(app: AppHandle) -> Availability {
    availability(&get_settings(&app))
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
    let settings = get_settings(&app);
    if !settings.learn_from_corrections {
        return Ok(Vec::new());
    }
    let checker = match learning::check::checker(&settings) {
        Ok(checker) => checker,
        Err(reason) => {
            debug!("learning skipped: {reason:?}");
            return Ok(Vec::new());
        }
    };
    let ctx = LearnContext {
        custom_words: &settings.custom_words,
        denylist: &settings.learning_denylist,
    };
    let learned = learning::learn(&original, &edited, &ctx, &checker).await;
    if learned.is_empty() {
        return Ok(learned);
    }

    let mut settings = get_settings(&app);
    for word in &learned {
        if !settings.custom_words.iter().any(|w| w == word) {
            settings.custom_words.push(word.clone());
        }
    }
    write_settings(&app, settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "custom_words" }),
    );
    info!("learned {} custom word(s) from a correction", learned.len());
    Ok(learned)
}
