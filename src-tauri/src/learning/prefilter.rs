//! Deterministic rules that decide which replacements are worth asking the
//! model about. Everything the dictionary documentation says is never learned
//! is dropped here, so the model only sees plausible vocabulary and a
//! rewritten sentence costs one dropped edit instead of a batch of calls.

use super::diff::{match_key, Hunk};

/// Most tokens on either side of a candidate.
pub const MAX_TOKENS_PER_SIDE: usize = 4;
/// Most characters in a learned entry, matching the Custom Words input.
pub const MAX_MEANT_CHARS: usize = 50;
/// Most candidates taken from a single edit.
pub const MAX_CANDIDATES_PER_EDIT: usize = 4;
/// More hunks than this and the edit is a rewrite, not a set of corrections.
pub const MAX_HUNKS_PER_EDIT: usize = 6;

/// Speech fillers that never count as vocabulary.
const FILLER_WORDS: &[&str] = &[
    "um", "uh", "uhm", "umm", "uhh", "uhhh", "er", "erm", "ehh", "ehm", "ahm", "hmm", "hm", "mmm",
    "like", "ah", "oh",
];

/// Everyday function words. A candidate made only of these is never
/// vocabulary, whatever the model might say.
const FUNCTION_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "nor", "so", "yet", "for", "of", "to", "in", "on", "at",
    "by", "with", "from", "into", "onto", "over", "under", "about", "after", "before", "between",
    "through", "during", "without", "within", "up", "down", "out", "off", "than", "then", "as",
    "if", "because", "while", "when", "where", "why", "how", "what", "which", "who", "whom",
    "whose", "that", "this", "these", "those", "it", "its", "i", "me", "my", "mine", "we", "us",
    "our", "ours", "you", "your", "yours", "he", "him", "his", "she", "her", "hers", "they",
    "them", "their", "theirs", "is", "am", "are", "was", "were", "be", "been", "being", "do",
    "does", "did", "done", "have", "has", "had", "will", "would", "shall", "should", "can",
    "could", "may", "might", "must", "not", "no", "yes", "very", "just", "also", "too", "only",
    "even", "still", "there", "here", "now", "some", "any", "all", "each", "every", "both", "few",
    "many", "much", "more", "most", "other", "another", "such", "own", "same", "ok", "okay",
    "please", "thanks",
];

/// One replacement the model will be asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// What the speech model wrote.
    pub heard: String,
    /// What the user changed it to, with surrounding punctuation trimmed.
    pub meant: String,
}

/// Why a replacement was not turned into a candidate. Reported in debug logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dropped {
    TooLong,
    Filler,
    FunctionWords,
    AlreadyKnown,
    Denied,
}

/// Trim punctuation from the ends of a phrase while keeping inner marks such
/// as `R&D`, `CI/CD` or `GPT-4`.
fn trim_outer_punctuation(phrase: &str) -> String {
    phrase
        .trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '&' && c != '/' && c != '-' && c != '\''
        })
        .trim_matches(|c: char| c == '-' || c == '/' || c == '\'')
        .to_string()
}

fn phrase_key(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|t| match_key(t))
        .collect::<Vec<_>>()
        .concat()
}

fn all_in(list: &[&str], tokens: &[String]) -> bool {
    tokens.iter().all(|t| list.contains(&match_key(t).as_str()))
}

fn any_in(list: &[&str], tokens: &[String]) -> bool {
    tokens.iter().any(|t| list.contains(&match_key(t).as_str()))
}

/// Apply the rules to one replacement hunk.
pub fn candidate(
    hunk: &Hunk,
    known_keys: &[String],
    denied_keys: &[String],
) -> Result<Candidate, Dropped> {
    if hunk.removed.len() > MAX_TOKENS_PER_SIDE || hunk.inserted.len() > MAX_TOKENS_PER_SIDE {
        return Err(Dropped::TooLong);
    }
    let meant = trim_outer_punctuation(&hunk.inserted.join(" "));
    let heard = trim_outer_punctuation(&hunk.removed.join(" "));
    if meant.chars().count() > MAX_MEANT_CHARS || meant.is_empty() || heard.is_empty() {
        return Err(Dropped::TooLong);
    }
    if any_in(FILLER_WORDS, &hunk.inserted) {
        return Err(Dropped::Filler);
    }
    if all_in(FUNCTION_WORDS, &hunk.inserted) {
        return Err(Dropped::FunctionWords);
    }
    let key = phrase_key(&hunk.inserted);
    if known_keys.contains(&key) {
        return Err(Dropped::AlreadyKnown);
    }
    if denied_keys.contains(&key) {
        return Err(Dropped::Denied);
    }
    Ok(Candidate { heard, meant })
}

/// Match keys for a list of words or phrases, for `known_keys` and
/// `denied_keys`.
pub fn keys(words: &[String]) -> Vec<String> {
    words
        .iter()
        .map(|w| {
            w.split_whitespace()
                .map(match_key)
                .collect::<Vec<_>>()
                .concat()
        })
        .filter(|k| !k.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(removed: &str, inserted: &str) -> Hunk {
        Hunk {
            removed: removed.split_whitespace().map(str::to_string).collect(),
            inserted: inserted.split_whitespace().map(str::to_string).collect(),
        }
    }

    #[test]
    fn plain_replacement_becomes_a_candidate() {
        let c = candidate(&hunk("Charge B,", "ChargeBee,"), &[], &[]).unwrap();
        assert_eq!(
            c,
            Candidate {
                heard: "Charge B".into(),
                meant: "ChargeBee".into()
            }
        );
    }

    #[test]
    fn inner_punctuation_survives_trimming() {
        assert_eq!(
            candidate(&hunk("R and D", "R&D."), &[], &[]).unwrap().meant,
            "R&D"
        );
        assert_eq!(
            candidate(&hunk("GPT four", "GPT-4,"), &[], &[])
                .unwrap()
                .meant,
            "GPT-4"
        );
    }

    #[test]
    fn joins_and_splits_are_candidates() {
        // Case and punctuation-only edits never produce a hunk (the diff
        // compares per-token keys), so a hunk whose joined keys match is a
        // spacing change: spelled-out acronyms and compound names.
        assert_eq!(
            candidate(&hunk("c i c d", "CI/CD,"), &[], &[])
                .unwrap()
                .meant,
            "CI/CD"
        );
        assert_eq!(
            candidate(&hunk("mac book", "MacBook"), &[], &[])
                .unwrap()
                .meant,
            "MacBook"
        );
        assert_eq!(
            candidate(&hunk("a p i", "API"), &[], &[]).unwrap().meant,
            "API"
        );
    }

    #[test]
    fn long_sides_are_dropped() {
        assert_eq!(
            candidate(&hunk("a b c d e", "x"), &[], &[]).unwrap_err(),
            Dropped::TooLong
        );
        assert_eq!(
            candidate(&hunk("x", "a b c d e"), &[], &[]).unwrap_err(),
            Dropped::TooLong
        );
    }

    #[test]
    fn fillers_and_function_words_are_dropped() {
        assert_eq!(
            candidate(&hunk("so", "um"), &[], &[]).unwrap_err(),
            Dropped::Filler
        );
        assert_eq!(
            candidate(&hunk("went", "and then the"), &[], &[]).unwrap_err(),
            Dropped::FunctionWords
        );
        assert!(candidate(&hunk("went to", "walked to"), &[], &[]).is_ok());
    }

    #[test]
    fn known_and_denied_words_are_dropped() {
        let known = keys(&["ChargeBee".to_string()]);
        let denied = keys(&["Bifrost".to_string()]);
        assert_eq!(
            candidate(&hunk("Charge B", "chargebee"), &known, &[]).unwrap_err(),
            Dropped::AlreadyKnown
        );
        assert_eq!(
            candidate(&hunk("by frost", "Bifrost"), &[], &denied).unwrap_err(),
            Dropped::Denied
        );
    }

    #[test]
    fn keys_join_multiword_entries() {
        assert_eq!(
            keys(&["MacBook Pro".to_string(), " ".to_string()]),
            vec!["macbookpro".to_string()]
        );
    }
}
