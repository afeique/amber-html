//! Token-budget-aware output. See `Plans.md`.
//!
//! [`estimate_tokens`] gives an **approximate, model-agnostic** token count, and
//! [`truncate_to_tokens`] trims text to fit a budget at word boundaries. The
//! estimate intentionally avoids embedding any single model's BPE vocabulary —
//! the exact tokenizer is an open question (Plans.md) — so callers needing
//! precise counts for a specific model should re-measure with that model's
//! tokenizer. The approximation is good enough for budgeting and reporting.

/// Approximate the number of tokens in `text`, model-agnostically.
///
/// Uses the widely-cited "~4 characters per token" rule of thumb for
/// English-like text, taking the max with the word count so whitespace-heavy or
/// very short text isn't underestimated. Empty text is 0 tokens.
pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    let words = text.split_whitespace().count();
    (chars / 4).max(words)
}

/// Trim `text` to at most `max_tokens` (per [`estimate_tokens`]), cutting only
/// at whitespace so words stay intact, and return the trimmed text together
/// with its estimated token count.
///
/// Text already within budget is returned unchanged. A `max_tokens` of 0 (or a
/// budget too small to fit even the first word) yields an empty string.
pub fn truncate_to_tokens(text: &str, max_tokens: usize) -> (String, usize) {
    if estimate_tokens(text) <= max_tokens {
        return (text.to_string(), estimate_tokens(text));
    }
    if max_tokens == 0 {
        return (String::new(), 0);
    }

    // Start from a character-budget guess (~4 chars/token) at a char boundary.
    let max_chars = max_tokens.saturating_mul(4);
    let end = text
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(text.len());

    // Back off to the last whitespace so we don't cut mid-word.
    let mut out = match text[..end].rfind(char::is_whitespace) {
        Some(ws) => text[..ws].trim_end().to_string(),
        None => String::new(),
    };

    // The estimate is non-linear (max of chars/4 and word count), so guarantee
    // the budget by dropping trailing words until we're within it.
    while estimate_tokens(&out) > max_tokens {
        match out.rfind(char::is_whitespace) {
            Some(ws) => out.truncate(ws),
            None => {
                out.clear();
                break;
            }
        }
        let trimmed = out.trim_end().len();
        out.truncate(trimmed);
    }

    let count = estimate_tokens(&out);
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_empty_is_zero() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("   "), 0);
    }

    #[test]
    fn estimate_uses_max_of_chars_and_words() {
        // 11 chars / 4 = 2; 2 words → max = 2.
        assert_eq!(estimate_tokens("hello world"), 2);
        // Many short words: word count dominates the char/4 estimate.
        let text = "a a a a a a a a"; // 8 words, 15 chars → 15/4=3, max=8
        assert_eq!(estimate_tokens(text), 8);
    }

    #[test]
    fn under_budget_text_is_unchanged() {
        let text = "short enough to keep entirely";
        let (out, n) = truncate_to_tokens(text, 100);
        assert_eq!(out, text);
        assert_eq!(n, estimate_tokens(text));
    }

    #[test]
    fn over_budget_text_is_trimmed_within_budget() {
        let text = "lorem ipsum dolor sit amet ".repeat(50); // ~1350 chars
        let max = 20;
        let (out, n) = truncate_to_tokens(&text, max);
        assert!(n <= max, "result exceeds budget: {n} > {max}");
        assert!(!out.is_empty(), "should keep some content");
        // The output is a prefix of the original (only trailing content dropped).
        assert!(text.starts_with(&out), "output must be a prefix:\n{out}");
        // No mid-word cut: the trimmed text doesn't end in a partial token that
        // wasn't a whole word in the source.
        assert!(text.contains(out.split_whitespace().last().unwrap()));
    }

    #[test]
    fn zero_budget_is_empty() {
        let (out, n) = truncate_to_tokens("anything at all here", 0);
        assert!(out.is_empty());
        assert_eq!(n, 0);
    }

    #[test]
    fn budget_too_small_for_first_word_is_empty() {
        // A single long word can't fit a tiny budget without splitting it.
        let (out, _) = truncate_to_tokens("supercalifragilisticexpialidocious", 1);
        assert!(out.is_empty());
    }
}
