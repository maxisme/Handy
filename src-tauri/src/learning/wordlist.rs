//! The system's English word list, used to tell a coined name from an
//! ordinary word when the model cannot.

use std::collections::HashSet;
use std::sync::OnceLock;

/// Path of the word list on macOS (about 235k entries).
const SYSTEM_WORDS: &str = "/usr/share/dict/words";

/// Shortest word the list is consulted for; anything shorter is too likely
/// to be an abbreviation or a typo.
const MIN_CHARS: usize = 3;

/// Common English inflections. The list holds mostly base forms, so a word
/// counts as known when stripping one of these leaves a listed word.
const SUFFIXES: [&str; 6] = ["'s", "ies", "es", "s", "ed", "ing"];

pub struct WordList {
    words: HashSet<String>,
}

impl WordList {
    pub fn from_words<I, S>(words: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            words: words
                .into_iter()
                .map(|w| w.as_ref().trim().to_lowercase())
                .filter(|w| !w.is_empty())
                .collect(),
        }
    }

    /// The system list, read once. Empty when the file is missing, in which
    /// case nothing is ever judged coined.
    pub fn system() -> &'static WordList {
        static LIST: OnceLock<WordList> = OnceLock::new();
        LIST.get_or_init(|| {
            let text = std::fs::read_to_string(SYSTEM_WORDS).unwrap_or_default();
            WordList::from_words(text.lines())
        })
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    fn contains(&self, word: &str) -> bool {
        self.words.contains(word)
    }

    /// True for a single alphabetic token that the list does not know in any
    /// common inflection: a name or term the user coined rather than a
    /// misspelling of, or substitute for, an ordinary word.
    pub fn is_coined(&self, term: &str) -> bool {
        if self.is_empty() {
            return false;
        }
        let word = term.trim().to_lowercase();
        if word.chars().count() < MIN_CHARS || !word.chars().all(|c| c.is_alphabetic()) {
            return false;
        }
        if self.contains(&word) {
            return false;
        }
        let stem_known = SUFFIXES.iter().any(|suffix| {
            word.strip_suffix(suffix).is_some_and(|stem| {
                self.contains(stem)
                    || (*suffix == "ies" && self.contains(&format!("{stem}y")))
                    || ((*suffix == "ed" || *suffix == "ing") && self.contains(&format!("{stem}e")))
            })
        });
        !stem_known
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> WordList {
        WordList::from_words(["cluster", "notification", "Stage", "deploy", "fly", "move"])
    }

    #[test]
    fn listed_words_are_not_coined() {
        assert!(!list().is_coined("cluster"));
        assert!(!list().is_coined("Notification"));
        assert!(!list().is_coined("stage"));
    }

    #[test]
    fn inflections_of_listed_words_are_not_coined() {
        assert!(!list().is_coined("clusters"));
        assert!(!list().is_coined("deployed"));
        assert!(!list().is_coined("deploying"));
        assert!(!list().is_coined("flies"));
        assert!(!list().is_coined("moved"));
        assert!(!list().is_coined("moving"));
        assert!(!list().is_coined("cluster's"));
    }

    #[test]
    fn unknown_alphabetic_words_are_coined() {
        assert!(list().is_coined("Zentryx"));
        assert!(list().is_coined("kavuu"));
    }

    #[test]
    fn short_or_non_alphabetic_terms_are_never_coined() {
        assert!(!list().is_coined("ab"));
        assert!(!list().is_coined("K8s"));
        assert!(!list().is_coined("CI/CD"));
        assert!(!list().is_coined("two words"));
    }

    #[test]
    fn empty_list_judges_nothing_coined() {
        assert!(!WordList::from_words(Vec::<&str>::new()).is_coined("Zentryx"));
    }
}
