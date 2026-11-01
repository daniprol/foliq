//! Counting tokens, the unit that agents' budgets are measured in.
//!
//! foliq uses OpenAI's `o200k_base` tokenizer. Other models count a little
//! differently, but close enough for "fit this answer in ~4000 tokens".

use tiktoken_rs::CoreBPE;

/// The tokenizer, loaded the first time it is needed and then shared.
pub fn tokenizer() -> &'static CoreBPE {
    tiktoken_rs::o200k_base_singleton()
}

/// Words longer than this (in bytes) are counted in pieces. The tokenizer
/// takes time that grows with the square of a word's length, so one 30,000-
/// character "word" (a hash, base64, a broken file) would take a fifth of a
/// second. Real words are far shorter, so their counts don't change.
const LONGEST_WORD: usize = 200;

/// The number of tokens in `text`.
pub fn count_tokens(text: &str) -> usize {
    let mut total = 0;
    // Where the uncounted text starts, and where the current word started.
    let mut piece_start = 0;
    let mut word_start = 0;
    for (index, character) in text.char_indices() {
        if character.is_whitespace() {
            word_start = index + character.len_utf8();
        } else if index - word_start >= LONGEST_WORD {
            total += tokenizer().count_ordinary(&text[piece_start..index]);
            piece_start = index;
            word_start = index;
        }
    }
    total + tokenizer().count_ordinary(&text[piece_start..])
}

/// Counts tokens for the `text-splitter` crate, with [`count_tokens`].
pub struct TokenCounter;

impl text_splitter::ChunkSizer for TokenCounter {
    fn size(&self, chunk: &str) -> usize {
        count_tokens(chunk)
    }
}

/// The longest start of `text` that fits in `budget` tokens, cut at a line end
/// when possible. Returns the kept text and whether anything was cut.
pub fn truncate_to_tokens(text: &str, budget: usize) -> (&str, bool) {
    if count_tokens(text) <= budget {
        return (text, false);
    }
    let mut kept_end = 0;
    let mut used = 0;
    for line in text.split_inclusive('\n') {
        let line_tokens = count_tokens(line);
        if used + line_tokens > budget {
            break;
        }
        used += line_tokens;
        kept_end += line.len();
    }
    // A single line longer than the budget: cut it at a character boundary.
    if kept_end == 0 {
        let mut end = text.len().min(budget * 3);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        while end > 0 && count_tokens(&text[..end]) > budget {
            end = text[..end].char_indices().last().map_or(0, |(index, _)| index);
            end = end.saturating_sub(end / 10);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
        }
        kept_end = end;
    }
    (&text[..kept_end], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_keeps_whole_lines() {
        let text = "one two three\nfour five six\nseven eight nine\n";
        let two_lines = count_tokens("one two three\nfour five six\n");
        let (kept, cut) = truncate_to_tokens(text, two_lines);
        assert!(cut);
        assert_eq!(kept, "one two three\nfour five six\n");
        assert_eq!(truncate_to_tokens(text, 1000), (text, false));
    }

    #[test]
    fn truncation_cuts_a_long_line() {
        let text = "word ".repeat(100);
        let (kept, cut) = truncate_to_tokens(&text, 10);
        assert!(cut);
        assert!(count_tokens(kept) <= 10);
        assert!(!kept.is_empty());
    }
}
