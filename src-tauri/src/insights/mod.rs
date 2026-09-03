//! Aggregates the transcription history into the numbers shown on the
//! Insights page: words dictated, speaking rate, fixes, per-category usage
//! and the daily streak.
//!
//! `compute` is pure over the rows the history manager hands it, so the whole
//! page can be checked with in-memory data. Day boundaries follow the caller's
//! time zone; the app passes `chrono::Local`.

pub mod category;

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Days, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use specta::Type;

pub use category::UsageCategory;

/// The columns of one history entry the insights need.
#[derive(Clone, Debug, Default)]
pub struct InsightRow {
    pub timestamp: i64,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_requested: bool,
    pub duration_ms: Option<i64>,
    pub dictionary_fixes: i64,
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CategoryUsage {
    pub category: UsageCategory,
    pub dictations: u32,
    pub words: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct AppUsage {
    pub name: String,
    pub dictations: u32,
    pub words: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct DayActivity {
    /// Local calendar day as `YYYY-MM-DD`.
    pub date: String,
    pub dictations: u32,
    pub words: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct InsightsStats {
    pub total_words: u32,
    pub total_dictations: u32,
    pub words_this_month: u32,
    pub words_previous_month: u32,
    /// Spoken words per minute over the dictations that recorded a duration.
    pub words_per_minute: Option<f64>,
    pub timed_dictations: u32,
    /// Custom-word corrections applied by the fuzzy matcher.
    pub dictionary_fixes: u32,
    /// Words changed by post-processing where it ran.
    pub post_process_fixes: u32,
    pub categories: Vec<CategoryUsage>,
    /// Dictations with no app recorded, which the categories exclude. Every
    /// entry saved before app attribution shipped counts here.
    pub unattributed: u32,
    pub total_apps: u32,
    pub top_apps: Vec<AppUsage>,
    pub current_streak: u32,
    pub longest_streak: u32,
    pub active_today: bool,
    pub activity: Vec<DayActivity>,
}

const TOP_APPS: usize = 8;

pub fn word_count(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

/// Number of custom-word corrections between the raw transcript and the
/// corrected one: each run of changed tokens is one fix.
pub fn count_dictionary_fixes(raw: &str, corrected: &str) -> u32 {
    if raw == corrected {
        return 0;
    }
    match crate::learning::diff::hunks(raw, corrected) {
        Some(hunks) => hunks.len() as u32,
        // Too long to diff token by token; the texts differ, so count one.
        None => 1,
    }
}

/// Words a post-processing pass changed: the longer side of every hunk.
fn count_post_process_fixes(raw: &str, processed: &str) -> u32 {
    if raw == processed {
        return 0;
    }
    match crate::learning::diff::hunks(raw, processed) {
        Some(hunks) => hunks
            .iter()
            .map(|h| h.removed.len().max(h.inserted.len()) as u32)
            .sum(),
        None => word_count(raw).abs_diff(word_count(processed)).max(1),
    }
}

fn previous_month(today: NaiveDate) -> (i32, u32) {
    if today.month() == 1 {
        (today.year() - 1, 12)
    } else {
        (today.year(), today.month() - 1)
    }
}

pub fn compute<Tz: TimeZone>(rows: &[InsightRow], tz: &Tz, today: NaiveDate) -> InsightsStats {
    let mut total_words = 0u32;
    let mut words_this_month = 0u32;
    let mut words_previous_month = 0u32;
    let mut timed_words = 0u64;
    let mut timed_ms = 0u64;
    let mut timed_dictations = 0u32;
    let mut dictionary_fixes = 0u32;
    let mut post_process_fixes = 0u32;
    let mut unattributed = 0u32;
    let mut by_category: HashMap<UsageCategory, (u32, u32)> = HashMap::new();
    let mut by_app: HashMap<String, (String, u32, u32)> = HashMap::new();
    let mut by_day: BTreeMap<NaiveDate, (u32, u32)> = BTreeMap::new();

    let this_month = (today.year(), today.month());
    let prev_month = previous_month(today);

    for row in rows {
        let words = word_count(&row.transcription_text);
        total_words += words;

        if let Some(day) = tz
            .timestamp_opt(row.timestamp, 0)
            .single()
            .map(|dt| dt.date_naive())
        {
            let month = (day.year(), day.month());
            if month == this_month {
                words_this_month += words;
            } else if month == prev_month {
                words_previous_month += words;
            }
            let entry = by_day.entry(day).or_default();
            entry.0 += 1;
            entry.1 += words;
        }

        if let Some(ms) = row.duration_ms.filter(|ms| *ms > 0) {
            timed_words += words as u64;
            timed_ms += ms as u64;
            timed_dictations += 1;
        }

        dictionary_fixes += row.dictionary_fixes.max(0) as u32;
        if row.post_process_requested {
            if let Some(processed) = &row.post_processed_text {
                post_process_fixes += count_post_process_fixes(&row.transcription_text, processed);
            }
        }

        // A row with nothing known about its destination is not "Other", it
        // is unmeasured. Counting it as a category would report a breakdown
        // the data cannot support.
        let has_app = [
            row.app_id.as_deref(),
            row.app_name.as_deref(),
            row.window_title.as_deref(),
        ]
        .iter()
        .any(|field| field.is_some_and(|value| !value.trim().is_empty()));

        if has_app {
            let category = category::classify(
                row.app_id.as_deref(),
                row.app_name.as_deref(),
                row.window_title.as_deref(),
            );
            let entry = by_category.entry(category).or_default();
            entry.0 += 1;
            entry.1 += words;
        } else {
            unattributed += 1;
        }

        let display_name = row
            .app_name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .or(row.app_id.as_deref());
        if let Some(name) = display_name {
            let key = row
                .app_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .unwrap_or(name)
                .to_lowercase();
            let entry = by_app
                .entry(key)
                .or_insert_with(|| (name.to_string(), 0, 0));
            entry.1 += 1;
            entry.2 += words;
        }
    }

    let words_per_minute = (timed_ms > 0)
        .then(|| timed_words as f64 / (timed_ms as f64 / 60_000.0))
        .filter(|wpm| wpm.is_finite());

    let categories = UsageCategory::ALL
        .iter()
        .map(|category| {
            let (dictations, words) = by_category.get(category).copied().unwrap_or_default();
            CategoryUsage {
                category: *category,
                dictations,
                words,
            }
        })
        .collect();

    let total_apps = by_app.len() as u32;
    let mut top_apps: Vec<AppUsage> = by_app
        .into_values()
        .map(|(name, dictations, words)| AppUsage {
            name,
            dictations,
            words,
        })
        .collect();
    top_apps.sort_by(|a, b| b.dictations.cmp(&a.dictations).then(a.name.cmp(&b.name)));
    top_apps.truncate(TOP_APPS);

    let (current_streak, longest_streak) = streaks(&by_day, today);
    let active_today = by_day.contains_key(&today);

    let activity = by_day
        .iter()
        .map(|(day, (dictations, words))| DayActivity {
            date: day.format("%Y-%m-%d").to_string(),
            dictations: *dictations,
            words: *words,
        })
        .collect();

    InsightsStats {
        total_words,
        total_dictations: rows.len() as u32,
        words_this_month,
        words_previous_month,
        words_per_minute,
        timed_dictations,
        dictionary_fixes,
        post_process_fixes,
        categories,
        unattributed,
        total_apps,
        top_apps,
        current_streak,
        longest_streak,
        active_today,
        activity,
    }
}

/// `(current, longest)` runs of consecutive active days. The current streak
/// is still alive on a day with no dictation yet, so it is counted back from
/// yesterday when today is empty.
fn streaks(by_day: &BTreeMap<NaiveDate, (u32, u32)>, today: NaiveDate) -> (u32, u32) {
    let mut longest = 0u32;
    let mut run = 0u32;
    let mut previous: Option<NaiveDate> = None;
    for day in by_day.keys() {
        run = match previous.and_then(|p| p.checked_add_days(Days::new(1))) {
            Some(next) if next == *day => run + 1,
            _ => 1,
        };
        longest = longest.max(run);
        previous = Some(*day);
    }

    let mut cursor = if by_day.contains_key(&today) {
        Some(today)
    } else {
        today
            .checked_sub_days(Days::new(1))
            .filter(|yesterday| by_day.contains_key(yesterday))
    };
    let mut current = 0u32;
    while let Some(day) = cursor {
        if !by_day.contains_key(&day) {
            break;
        }
        current += 1;
        cursor = day.checked_sub_days(Days::new(1));
    }
    (current, longest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
    }

    fn at(date: NaiveDate, hour: u32) -> i64 {
        date.and_hms_opt(hour, 0, 0)
            .expect("valid time")
            .and_utc()
            .timestamp()
    }

    fn row(date: NaiveDate, text: &str) -> InsightRow {
        InsightRow {
            timestamp: at(date, 10),
            transcription_text: text.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn empty_history_is_all_zeroes() {
        let stats = compute(&[], &Utc, day(2026, 9, 3));
        assert_eq!(stats.total_words, 0);
        assert_eq!(stats.total_dictations, 0);
        assert_eq!(stats.words_per_minute, None);
        assert_eq!(stats.current_streak, 0);
        assert_eq!(stats.longest_streak, 0);
        assert!(!stats.active_today);
        assert!(stats.activity.is_empty());
        assert_eq!(stats.total_apps, 0);
        assert_eq!(stats.categories.len(), UsageCategory::ALL.len());
        assert!(stats.categories.iter().all(|c| c.dictations == 0));
        assert_eq!(stats.unattributed, 0);
    }

    #[test]
    fn words_and_months_are_totalled_per_local_day() {
        let today = day(2026, 9, 3);
        let rows = [
            row(day(2026, 9, 1), "one two three"),
            row(day(2026, 9, 3), "four five"),
            row(day(2026, 8, 30), "six"),
            row(day(2026, 7, 4), "seven eight"),
        ];
        let stats = compute(&rows, &Utc, today);
        assert_eq!(stats.total_words, 8);
        assert_eq!(stats.total_dictations, 4);
        assert_eq!(stats.words_this_month, 5);
        assert_eq!(stats.words_previous_month, 1);
        assert!(stats.active_today);
        assert_eq!(
            stats
                .activity
                .iter()
                .map(|d| (d.date.as_str(), d.dictations, d.words))
                .collect::<Vec<_>>(),
            vec![
                ("2026-07-04", 1, 2),
                ("2026-08-30", 1, 1),
                ("2026-09-01", 1, 3),
                ("2026-09-03", 1, 2),
            ]
        );
    }

    #[test]
    fn previous_month_wraps_the_year() {
        let rows = [row(day(2025, 12, 31), "a b c"), row(day(2026, 1, 2), "d")];
        let stats = compute(&rows, &Utc, day(2026, 1, 15));
        assert_eq!(stats.words_this_month, 1);
        assert_eq!(stats.words_previous_month, 3);
    }

    #[test]
    fn words_per_minute_uses_only_timed_dictations() {
        let mut timed = row(
            day(2026, 9, 3),
            "one two three four five six seven eight nine ten",
        );
        timed.duration_ms = Some(4_000);
        let mut zero = row(day(2026, 9, 3), "ignored words here");
        zero.duration_ms = Some(0);
        let untimed = row(day(2026, 9, 3), "also ignored");
        let stats = compute(&[timed, zero, untimed], &Utc, day(2026, 9, 3));
        assert_eq!(stats.timed_dictations, 1);
        assert_eq!(stats.words_per_minute, Some(150.0));
    }

    #[test]
    fn fixes_come_from_the_dictionary_counter_and_post_processing_diffs() {
        let mut a = row(day(2026, 9, 3), "we shipped handy today");
        a.dictionary_fixes = 2;
        a.post_process_requested = true;
        a.post_processed_text = Some("We shipped Handy today.".to_string());
        let mut b = row(day(2026, 9, 3), "meet at five pm tomorrow ok");
        b.post_process_requested = true;
        b.post_processed_text = Some("Meet at 5pm tomorrow.".to_string());
        let mut c = row(day(2026, 9, 3), "not requested but present");
        c.post_processed_text = Some("completely different text".to_string());
        let stats = compute(&[a, b, c], &Utc, day(2026, 9, 3));
        assert_eq!(stats.dictionary_fixes, 2);
        // a: punctuation and case only; b: "five pm" -> "5pm" (2) and "ok" dropped (1).
        assert_eq!(stats.post_process_fixes, 3);
    }

    #[test]
    fn dictionary_fix_counter_counts_runs_of_changed_tokens() {
        assert_eq!(count_dictionary_fixes("same text", "same text"), 0);
        assert_eq!(
            count_dictionary_fixes(
                "charge b is great and open a i",
                "ChargeBee is great and OpenAI"
            ),
            2
        );
    }

    #[test]
    fn categories_and_apps_are_tallied() {
        let mut slack = row(day(2026, 9, 3), "hello team");
        slack.app_id = Some("com.tinyspeck.slackmacgap".into());
        slack.app_name = Some("Slack".into());
        let mut slack_again = slack.clone();
        slack_again.transcription_text = "one more".into();
        let mut chrome = row(day(2026, 9, 3), "draft an email please");
        chrome.app_id = Some("com.google.Chrome".into());
        chrome.app_name = Some("Google Chrome".into());
        chrome.window_title = Some("Inbox - Gmail".into());
        let unknown = row(day(2026, 9, 3), "x");

        let stats = compute(
            &[slack, slack_again, chrome, unknown],
            &Utc,
            day(2026, 9, 3),
        );

        let usage = |category: UsageCategory| {
            stats
                .categories
                .iter()
                .find(|c| c.category == category)
                .map(|c| (c.dictations, c.words))
                .expect("category present")
        };
        assert_eq!(usage(UsageCategory::WorkMessages), (2, 4));
        assert_eq!(usage(UsageCategory::Emails), (1, 4));
        // The row with no app at all is unmeasured, not "Other".
        assert_eq!(usage(UsageCategory::Other), (0, 0));
        assert_eq!(stats.unattributed, 1);
        assert_eq!(usage(UsageCategory::AiPrompts), (0, 0));

        assert_eq!(stats.total_apps, 2);
        assert_eq!(
            stats
                .top_apps
                .iter()
                .map(|a| (a.name.as_str(), a.dictations, a.words))
                .collect::<Vec<_>>(),
            vec![("Slack", 2, 4), ("Google Chrome", 1, 4)]
        );
    }

    #[test]
    fn rows_without_an_app_are_unattributed_but_still_count_as_words() {
        let rows = [
            row(day(2026, 9, 3), "one two three"),
            InsightRow {
                app_name: Some("   ".to_string()),
                ..row(day(2026, 9, 3), "four five")
            },
        ];
        let stats = compute(&rows, &Utc, day(2026, 9, 3));
        assert_eq!(stats.unattributed, 2);
        assert_eq!(stats.total_words, 5);
        assert_eq!(stats.total_dictations, 2);
        assert!(stats.categories.iter().all(|c| c.dictations == 0));
        assert_eq!(stats.total_apps, 0);
    }

    #[test]
    fn streaks_count_consecutive_days() {
        let today = day(2026, 9, 3);
        let rows = [
            row(day(2026, 8, 10), "a"),
            row(day(2026, 8, 11), "a"),
            row(day(2026, 8, 12), "a"),
            row(day(2026, 8, 13), "a"),
            row(day(2026, 9, 2), "a"),
            row(day(2026, 9, 3), "a"),
        ];
        let stats = compute(&rows, &Utc, today);
        assert_eq!(stats.current_streak, 2);
        assert_eq!(stats.longest_streak, 4);
    }

    #[test]
    fn streak_survives_an_empty_today_but_not_an_empty_yesterday() {
        let today = day(2026, 9, 3);
        let alive = [row(day(2026, 9, 1), "a"), row(day(2026, 9, 2), "a")];
        let stats = compute(&alive, &Utc, today);
        assert_eq!(stats.current_streak, 2);
        assert!(!stats.active_today);

        let broken = [row(day(2026, 8, 31), "a"), row(day(2026, 9, 1), "a")];
        let stats = compute(&broken, &Utc, today);
        assert_eq!(stats.current_streak, 0);
        assert_eq!(stats.longest_streak, 2);
    }
}
