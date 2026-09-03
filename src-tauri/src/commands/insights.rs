use std::sync::Arc;

use chrono::Local;
use tauri::State;

use crate::insights::{self, InsightsStats};
use crate::managers::history::HistoryManager;

/// Usage statistics for the Insights page, aggregated over the whole history.
#[tauri::command]
#[specta::specta]
pub async fn get_insights(
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<InsightsStats, String> {
    let rows = history_manager
        .insight_rows()
        .map_err(|e| format!("Failed to read history: {e}"))?;
    let today = Local::now().date_naive();
    Ok(insights::compute(&rows, &Local, today))
}
