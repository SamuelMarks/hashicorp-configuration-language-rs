//! Typo suggestion utilities utilizing the Damerau-Levenshtein distance metric.
//!
//! Provides edit distance calculation supporting insertions, deletions, substitutions,
//! and adjacent transpositions, along with candidate ranking to suggest corrections
//! for misspelled identifiers, attributes, and variables.

#![deny(missing_docs)]

/// Computes the Damerau-Levenshtein (Optimal String Alignment) distance between two strings.
///
/// The distance is the minimum number of operations (insertions, deletions, substitutions,
/// or adjacent transpositions) required to transform `source` into `target`.
///
/// Memory allocation is optimized to a rotating buffer of three rows of size `O(min(|source|, |target|))`,
/// and Unicode scalar boundaries are properly preserved without panic.
///
/// # Arguments
/// * `source` - The first string slice to compare.
/// * `target` - The second string slice to compare.
///
/// # Examples
/// ```rust
/// use hashicorp_configuration_language_rs::diagnostic::suggestion::damerau_levenshtein_distance;
///
/// assert_eq!(damerau_levenshtein_distance("", ""), 0);
/// assert_eq!(damerau_levenshtein_distance("the", "the"), 0);
/// assert_eq!(damerau_levenshtein_distance("teh", "the"), 1); // transposition
/// assert_eq!(damerau_levenshtein_distance("th", "the"), 1);  // insertion/deletion
/// assert_eq!(damerau_levenshtein_distance("tie", "the"), 1); // substitution
/// ```
#[must_use]
pub fn damerau_levenshtein_distance(source: &str, target: &str) -> usize {
    let mut source_chars: Vec<char> = source.chars().collect();
    let mut target_chars: Vec<char> = target.chars().collect();

    if source_chars.is_empty() {
        return target_chars.len();
    }
    if target_chars.is_empty() {
        return source_chars.len();
    }

    // Ensure target_chars is shorter to minimize buffer allocation
    if source_chars.len() < target_chars.len() {
        std::mem::swap(&mut source_chars, &mut target_chars);
    }

    let source_len = source_chars.len();
    let target_len = target_chars.len();

    let mut prev_prev = vec![0; target_len + 1];
    let mut prev: Vec<usize> = (0..=target_len).collect();
    let mut curr = vec![0; target_len + 1];

    for i in 1..=source_len {
        curr[0] = i;
        for j in 1..=target_len {
            let cost = usize::from(source_chars[i - 1] != target_chars[j - 1]);
            let mut dist = std::cmp::min(curr[j - 1] + 1, prev[j] + 1);
            dist = std::cmp::min(dist, prev[j - 1] + cost);

            if i > 1
                && j > 1
                && source_chars[i - 1] == target_chars[j - 2]
                && source_chars[i - 2] == target_chars[j - 1]
            {
                dist = std::cmp::min(dist, prev_prev[j - 2] + 1);
            }

            curr[j] = dist;
        }

        std::mem::swap(&mut prev_prev, &mut prev);
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[target_len]
}

/// Finds the closest candidate name to `target` within the allowed maximum edit distance.
///
/// Iterates through candidates, computing the Damerau-Levenshtein distance to `target`.
/// Returns the candidate with the smallest distance `<= max_distance`.
/// If multiple candidates share the minimal distance, the earliest encountered is returned.
///
/// # Arguments
/// * `target` - The target misspelled or query name.
/// * `candidates` - Sequence of available candidate names to compare against.
/// * `max_distance` - The maximum allowable edit distance threshold.
///
/// # Examples
/// ```rust
/// use hashicorp_configuration_language_rs::diagnostic::suggestion::suggest_closest_name;
///
/// let candidates = ["provider", "resource", "variable", "output"];
/// assert_eq!(
///     suggest_closest_name("providre", candidates, 2),
///     Some("provider")
/// );
/// assert_eq!(suggest_closest_name("xyz", candidates, 2), None);
/// ```
#[must_use]
pub fn suggest_closest_name<'a>(
    target: &str,
    candidates: impl IntoIterator<Item = &'a str>,
    max_distance: usize,
) -> Option<&'a str> {
    let mut best_candidate = None;
    let mut best_distance = usize::MAX;

    for candidate in candidates {
        let dist = damerau_levenshtein_distance(target, candidate);
        if dist <= max_distance && dist < best_distance {
            best_distance = dist;
            best_candidate = Some(candidate);
            if dist == 0 {
                break;
            }
        }
    }

    best_candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_damerau_levenshtein_empty() {
        assert_eq!(damerau_levenshtein_distance("", ""), 0);
        assert_eq!(damerau_levenshtein_distance("a", ""), 1);
        assert_eq!(damerau_levenshtein_distance("", "a"), 1);
        assert_eq!(damerau_levenshtein_distance("abc", ""), 3);
        assert_eq!(damerau_levenshtein_distance("", "abc"), 3);
    }

    #[test]
    fn test_damerau_levenshtein_identical() {
        assert_eq!(damerau_levenshtein_distance("hello", "hello"), 0);
        assert_eq!(damerau_levenshtein_distance("a", "a"), 0);
    }

    #[test]
    fn test_damerau_levenshtein_insertions_deletions() {
        assert_eq!(damerau_levenshtein_distance("cat", "cats"), 1);
        assert_eq!(damerau_levenshtein_distance("cats", "cat"), 1);
        assert_eq!(damerau_levenshtein_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn test_damerau_levenshtein_substitutions() {
        assert_eq!(damerau_levenshtein_distance("dog", "fog"), 1);
        assert_eq!(damerau_levenshtein_distance("abc", "axc"), 1);
    }

    #[test]
    fn test_damerau_levenshtein_transpositions() {
        assert_eq!(damerau_levenshtein_distance("teh", "the"), 1);
        assert_eq!(damerau_levenshtein_distance("ab", "ba"), 1);
        assert_eq!(damerau_levenshtein_distance("abcdef", "abdcfe"), 2);
    }

    #[test]
    fn test_damerau_levenshtein_unicode() {
        assert_eq!(damerau_levenshtein_distance("café", "cfaé"), 1);
        assert_eq!(damerau_levenshtein_distance("🦀🚀", "🚀🦀"), 1);
        assert_eq!(damerau_levenshtein_distance("こんにちは", "こんには"), 1);
    }

    #[test]
    fn test_suggest_closest_name() {
        let candidates = ["provider", "resource", "variable", "output"];
        assert_eq!(
            suggest_closest_name("providre", candidates, 2),
            Some("provider")
        );
        assert_eq!(
            suggest_closest_name("resourc", candidates, 2),
            Some("resource")
        );
        assert_eq!(suggest_closest_name("outpu", candidates, 1), Some("output"));
        assert_eq!(suggest_closest_name("xyz", candidates, 2), None);
        assert_eq!(
            suggest_closest_name("variable", candidates, 0),
            Some("variable")
        );
    }

    #[test]
    fn test_suggest_closest_name_empty_candidates() {
        let empty: [&str; 0] = [];
        assert_eq!(suggest_closest_name("test", empty, 2), None);
    }
}
