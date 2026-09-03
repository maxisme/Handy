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
pub mod readback;
pub mod toast;

use log::debug;

pub use check::{availability, Availability, CorrectionKind, VocabularyCheck};
pub use prefilter::Candidate;

/// One learned entry: what the speech model wrote and the spelling to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Learned {
    pub heard: String,
    pub meant: String,
}

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

/// True when `meant` is written the way English marks a proper noun inside
/// `sentence`: a capitalised word that is not in all caps, carries no digits,
/// and does not open a sentence. The on-device model calls invented names it
/// has never seen "common words"; the user's capitalisation says otherwise,
/// and the user typed it.
pub fn written_as_proper_noun(meant: &str, sentence: &str) -> bool {
    let Some(first) = meant.split_whitespace().next() else {
        return false;
    };
    let mut chars = first.chars();
    let Some(initial) = chars.next() else {
        return false;
    };
    if !initial.is_uppercase() || first.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if !chars.any(|c| c.is_lowercase()) {
        return false;
    }
    let Some(index) = sentence.find(meant) else {
        return false;
    };
    let before = sentence[..index].trim_end();
    !(before.is_empty() || before.ends_with(['.', '!', '?', ':']))
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

/// Entries to add to the dictionary after the user corrected `original` into
/// `edited`. One model call per edit. Any failure yields nothing.
pub async fn learn<C: VocabularyCheck>(
    original: &str,
    edited: &str,
    ctx: &LearnContext<'_>,
    check: &C,
) -> Vec<Learned> {
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
    let mut learned: Vec<Learned> = Vec::new();
    for (candidate, kind) in candidates.iter().zip(kinds) {
        let proper_noun =
            kind == CorrectionKind::CommonWord && written_as_proper_noun(&candidate.meant, edited);
        if proper_noun {
            debug!(
                "learning: '{}' judged CommonWord but written as a proper noun, learning it",
                candidate.meant
            );
        }
        if kind.is_vocabulary() || proper_noun {
            let word = normalize_word(&candidate.meant);
            if !word.is_empty() && !learned.iter().any(|l| l.meant == word) {
                learned.push(Learned {
                    heard: candidate.heard.clone(),
                    meant: word,
                });
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
        assert_eq!(
            learned,
            vec![Learned {
                heard: "Charge B".into(),
                meant: "ChargeBee".into()
            }]
        );
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
        assert_eq!(
            learned.iter().map(|l| l.meant.as_str()).collect::<Vec<_>>(),
            vec!["Priyanka", "Zentrix"]
        );
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

    #[tokio::test]
    async fn a_capitalised_mid_sentence_word_overrides_a_common_word_verdict() {
        let check = Scripted::new(Ok(vec![CorrectionKind::CommonWord]));
        let learned = learn(
            "Load it into kah voo.",
            "Load it into Kavuu.",
            &ctx(&[], &[]),
            &check,
        )
        .await;
        assert_eq!(
            learned.iter().map(|l| l.meant.as_str()).collect::<Vec<_>>(),
            vec!["Kavuu"]
        );
    }

    #[tokio::test]
    async fn lowercase_and_sentence_initial_words_do_not_override() {
        let check = Scripted::new(Ok(vec![CorrectionKind::CommonWord]));
        assert!(learn(
            "Load it into kah voo.",
            "Load it into kavuu.",
            &ctx(&[], &[]),
            &check
        )
        .await
        .is_empty());
        let check = Scripted::new(Ok(vec![CorrectionKind::CommonWord]));
        assert!(learn(
            "Kah voo is the service.",
            "Kavuu is the service.",
            &ctx(&[], &[]),
            &check
        )
        .await
        .is_empty());
    }

    #[test]
    fn proper_noun_shape() {
        assert!(written_as_proper_noun(
            "Ostrava",
            "The Ostrava service handles enquiries."
        ));
        assert!(written_as_proper_noun(
            "MacBook Pro",
            "My MacBook Pro needs a restart."
        ));
        assert!(!written_as_proper_noun("Ostrava", "Ostrava handles it."));
        assert!(!written_as_proper_noun("ostrava", "The ostrava service."));
        assert!(!written_as_proper_noun("SDK", "Update the SDK first."));
        assert!(!written_as_proper_noun("GPT4", "Use GPT4 for this."));
        assert!(!written_as_proper_noun(
            "Ostrava",
            "Fine. Ostrava handles it."
        ));
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
