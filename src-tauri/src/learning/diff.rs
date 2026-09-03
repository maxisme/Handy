//! Token-level alignment of a transcript against the user's edited version.
//!
//! Tokens are compared on their match key (alphanumeric, lowercased) so that
//! punctuation and capitalisation differences do not register as changes on
//! their own. Runs of changed tokens become hunks; a hunk with both a removed
//! and an inserted side is a replacement, the only shape learning cares about.

/// Longest input, in tokens, that is diffed at all. Beyond this an edit is
/// treated as a rewrite rather than a correction.
pub const MAX_TOKENS: usize = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// Tokens removed from the original, in order. Empty for a pure insertion.
    pub removed: Vec<String>,
    /// Tokens inserted by the edit, in order. Empty for a pure deletion.
    pub inserted: Vec<String>,
}

impl Hunk {
    pub fn is_replacement(&self) -> bool {
        !self.removed.is_empty() && !self.inserted.is_empty()
    }
}

/// Lowercased alphanumeric characters of `token`, the same key the custom
/// words matcher uses.
pub fn match_key(token: &str) -> String {
    token
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn tokens(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

/// All hunks between `original` and `edited`, or `None` when either side is
/// too long to diff.
pub fn hunks(original: &str, edited: &str) -> Option<Vec<Hunk>> {
    let a = tokens(original);
    let b = tokens(edited);
    if a.len() > MAX_TOKENS || b.len() > MAX_TOKENS {
        return None;
    }
    let ka: Vec<String> = a.iter().map(|t| match_key(t)).collect();
    let kb: Vec<String> = b.iter().map(|t| match_key(t)).collect();

    // Classic LCS table over match keys.
    let (n, m) = (ka.len(), kb.len());
    let mut lcs = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if ka[i] == kb[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut out = Vec::new();
    let mut current: Option<Hunk> = None;
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        let matched = i < n && j < m && ka[i] == kb[j];
        if matched {
            if let Some(h) = current.take() {
                out.push(h);
            }
            i += 1;
            j += 1;
            continue;
        }
        let hunk = current.get_or_insert_with(|| Hunk {
            removed: Vec::new(),
            inserted: Vec::new(),
        });
        let take_from_a = j >= m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]);
        if take_from_a {
            hunk.removed.push(a[i].clone());
            i += 1;
        } else {
            hunk.inserted.push(b[j].clone());
            j += 1;
        }
    }
    if let Some(h) = current {
        out.push(h);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replacements(original: &str, edited: &str) -> Vec<(String, String)> {
        hunks(original, edited)
            .unwrap()
            .into_iter()
            .filter(Hunk::is_replacement)
            .map(|h| (h.removed.join(" "), h.inserted.join(" ")))
            .collect()
    }

    #[test]
    fn identical_text_has_no_hunks() {
        assert_eq!(hunks("we moved billing", "we moved billing"), Some(vec![]));
    }

    #[test]
    fn single_word_replacement() {
        assert_eq!(
            replacements(
                "moved billing to Charge B last week",
                "moved billing to ChargeBee last week"
            ),
            vec![("Charge B".to_string(), "ChargeBee".to_string())]
        );
    }

    #[test]
    fn punctuation_and_case_alone_are_not_changes() {
        assert_eq!(hunks("hello world", "Hello, world."), Some(vec![]));
    }

    #[test]
    fn insertion_and_deletion_are_not_replacements() {
        let h = hunks("send the report", "send the full report").unwrap();
        assert_eq!(h.len(), 1);
        assert!(!h[0].is_replacement());
        let h = hunks("send the full report", "send the report").unwrap();
        assert_eq!(h.len(), 1);
        assert!(!h[0].is_replacement());
    }

    #[test]
    fn multiple_replacements_stay_separate() {
        assert_eq!(
            replacements(
                "Ask pree yanka to move the zen tricks data",
                "Ask Priyanka to move the Zentrix data"
            ),
            vec![
                ("pree yanka".to_string(), "Priyanka".to_string()),
                ("zen tricks".to_string(), "Zentrix".to_string()),
            ]
        );
    }

    #[test]
    fn too_long_is_none() {
        let long = vec!["word"; MAX_TOKENS + 1].join(" ");
        assert_eq!(hunks(&long, "word"), None);
    }
}
