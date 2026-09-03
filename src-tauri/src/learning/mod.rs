//! Learning new vocabulary from the user's corrections.
//!
//! Given a transcript and the user's edited version, find the words that
//! changed, keep the ones that could be vocabulary, ask the configured model
//! which of those are names, products, acronyms or technical terms, and
//! return the spellings to add to the custom words list. Anything that cannot
//! be classified is dropped: a missed word costs the user one manual add,
//! while a wrong word pollutes their dictionary.

pub mod check;
pub mod diff;
pub mod prefilter;

use log::debug;

pub use check::{availability, Availability, VocabularyCheck};
pub use prefilter::Candidate;

/// What the learner already knows and must not learn again.
pub struct LearnContext<'a> {
    pub custom_words: &'a [String],
    pub denylist: &'a [String],
}

/// Normalise a learned entry the way the Custom Words input does.
pub fn normalize_word(word: &str) -> String {
    word.chars()
        .filter(|c| !matches!(c, '<' | '>' | '"' | '\''))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Candidates from an edit, before the model is consulted. Empty when the
/// edit is a rewrite, too long to diff, or contains nothing learnable.
pub fn candidates(original: &str, edited: &str, ctx: &LearnContext<'_>) -> Vec<Candidate> {
    let Some(hunks) = diff::hunks(original, edited) else {
        debug!("learning: edit too long to diff");
        return Vec::new();
    };
    if hunks.len() > prefilter::MAX_HUNKS_PER_EDIT {
        debug!(
            "learning: {} hunks, treating the edit as a rewrite",
            hunks.len()
        );
        return Vec::new();
    }
    let known = prefilter::keys(ctx.custom_words);
    let denied = prefilter::keys(ctx.denylist);
    let mut out = Vec::new();
    for hunk in hunks.iter().filter(|h| h.is_replacement()) {
        match prefilter::candidate(hunk, &known, &denied) {
            Ok(c) => out.push(c),
            Err(reason) => debug!("learning: dropped {:?}: {:?}", hunk, reason),
        }
        if out.len() == prefilter::MAX_CANDIDATES_PER_EDIT {
            break;
        }
    }
    out
}

/// Words to add to the dictionary after the user corrected `original` into
/// `edited`. One model call per edit. Any failure yields no words.
pub async fn learn<C: VocabularyCheck>(
    original: &str,
    edited: &str,
    ctx: &LearnContext<'_>,
    check: &C,
) -> Vec<String> {
    let candidates = candidates(original, edited, ctx);
    if candidates.is_empty() {
        return Vec::new();
    }
    let kinds = match check.check(&candidates, edited).await {
        Ok(kinds) => kinds,
        Err(err) => {
            debug!("learning: vocabulary check failed, learning nothing: {err}");
            return Vec::new();
        }
    };
    let mut learned = Vec::new();
    for (candidate, kind) in candidates.iter().zip(kinds) {
        if kind.is_vocabulary() {
            let word = normalize_word(&candidate.meant);
            if !word.is_empty() && !learned.contains(&word) {
                learned.push(word);
            }
        } else {
            debug!(
                "learning: '{}' judged {:?}, not learned",
                candidate.meant, kind
            );
        }
    }
    learned
}

#[cfg(test)]
mod tests {
    use super::check::CorrectionKind;
    use super::*;
    use std::future::Future;
    use std::sync::Mutex;

    /// Answers with a fixed list, recording what it was asked.
    struct Scripted {
        answer: Result<Vec<CorrectionKind>, String>,
        asked: Mutex<Vec<Vec<Candidate>>>,
    }

    impl Scripted {
        fn new(answer: Result<Vec<CorrectionKind>, String>) -> Self {
            Self {
                answer,
                asked: Mutex::new(Vec::new()),
            }
        }
        fn calls(&self) -> usize {
            self.asked.lock().unwrap().len()
        }
    }

    impl VocabularyCheck for Scripted {
        fn check(
            &self,
            candidates: &[Candidate],
            _context: &str,
        ) -> impl Future<Output = Result<Vec<CorrectionKind>, String>> + Send {
            self.asked.lock().unwrap().push(candidates.to_vec());
            let answer = self.answer.clone();
            async move { answer }
        }
    }

    fn ctx<'a>(custom: &'a [String], denied: &'a [String]) -> LearnContext<'a> {
        LearnContext {
            custom_words: custom,
            denylist: denied,
        }
    }

    #[tokio::test]
    async fn learns_only_pairs_the_model_calls_vocabulary() {
        let check = Scripted::new(Ok(vec![
            CorrectionKind::ProductOrCompany,
            CorrectionKind::Rewording,
        ]));
        let learned = learn(
            "We moved billing to Charge B and it went fine",
            "We moved billing to ChargeBee and it worked fine",
            &ctx(&[], &[]),
            &check,
        )
        .await;
        assert_eq!(learned, vec!["ChargeBee".to_string()]);
        assert_eq!(check.calls(), 1);
    }

    #[tokio::test]
    async fn one_call_carries_every_candidate() {
        let check = Scripted::new(Ok(vec![
            CorrectionKind::PersonName,
            CorrectionKind::ProjectOrService,
        ]));
        let learned = learn(
            "Ask pree yanka to move the zen tricks data",
            "Ask Priyanka to move the Zentrix data",
            &ctx(&[], &[]),
            &check,
        )
        .await;
        assert_eq!(learned, vec!["Priyanka".to_string(), "Zentrix".to_string()]);
        assert_eq!(check.asked.lock().unwrap()[0].len(), 2);
    }

    #[tokio::test]
    async fn nothing_learnable_means_no_model_call() {
        let check = Scripted::new(Ok(vec![]));
        assert!(learn(
            "I think handy is good",
            "I think Handy is good",
            &ctx(&[], &[]),
            &check
        )
        .await
        .is_empty());
        assert!(learn(
            "send the report",
            "send the full report",
            &ctx(&[], &[]),
            &check
        )
        .await
        .is_empty());
        assert_eq!(check.calls(), 0);
    }

    #[tokio::test]
    async fn model_failure_learns_nothing() {
        let check = Scripted::new(Err("offline".to_string()));
        assert!(learn("Charge B", "ChargeBee", &ctx(&[], &[]), &check)
            .await
            .is_empty());
        let check = Scripted::new(Ok(vec![]));
        assert!(learn("Charge B", "ChargeBee", &ctx(&[], &[]), &check)
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn known_and_denied_words_are_never_asked_about() {
        let custom = vec!["ChargeBee".to_string()];
        let denied = vec!["Bifrost".to_string()];
        let check = Scripted::new(Ok(vec![]));
        let learned = learn(
            "billing on Charge B, staging on by frost",
            "billing on ChargeBee, staging on Bifrost",
            &ctx(&custom, &denied),
            &check,
        )
        .await;
        assert!(learned.is_empty());
        assert_eq!(check.calls(), 0);
    }

    #[test]
    fn a_rewrite_yields_no_candidates() {
        let original = "one two three four five six seven eight nine ten eleven twelve";
        let edited = "a b c d e f g h i j k l";
        assert!(candidates(original, edited, &ctx(&[], &[])).is_empty());
    }

    #[test]
    fn at_most_four_candidates_per_edit() {
        let original = "aa1 x bb1 x cc1 x dd1 x ee1";
        let edited = "Alpha x Bravo x Charlie x Delta x Echo";
        assert_eq!(candidates(original, edited, &ctx(&[], &[])).len(), 4);
    }

    #[test]
    fn normalize_matches_custom_words_input() {
        assert_eq!(normalize_word("  <Charge\"Bee>  Pro "), "ChargeBee Pro");
    }
}
