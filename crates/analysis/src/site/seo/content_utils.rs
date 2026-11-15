//! Text extraction and tokenization for the SEO audit groups.
//!
//! The audit groups reason over the page's *prose* rather than its markup. These helpers turn a
//! parsed document into visible text and normalized tokens, deterministically, and without
//! mutating anything — every group receives the same parse tree.
//!
//! The tokenizer is **not** the base checks'. `on_page::check_keyword_alignment` splits on
//! non-word characters and drops short words; the source's audit groups count with
//! `[A-Za-z][A-Za-z'-]*`, which keeps `don't` and `state-of-the-art` as single tokens. A word
//! count that used the wrong tokenizer would move every threshold in this file, so the two
//! stay separate on purpose.
//!
//! Ported from the source service's `content_utils` at the captured revision; every rule (the
//! exclusion sets, the stop-word list, the syllable heuristic, the two readability formulas and
//! their rounding) is the source's, not a re-derivation.

use std::sync::OnceLock;

use regex::Regex;
use scraper::{ElementRef, Html, Node};

/// Tags whose text is not human-visible page copy.
const NON_TEXT_PARENTS: &[&str] = &["script", "style", "noscript", "template"];
/// Boilerplate regions excluded from the *main* content used for readability and keyword density.
const BOILERPLATE_TAGS: &[&str] = &["nav", "header", "footer", "aside", "form"];

fn word_re() -> &'static Regex {
    static WORD: OnceLock<Regex> = OnceLock::new();
    WORD.get_or_init(|| Regex::new(r"[A-Za-z][A-Za-z'-]*").expect("static pattern"))
}

fn sentence_split_re() -> &'static Regex {
    static SENTENCE: OnceLock<Regex> = OnceLock::new();
    SENTENCE.get_or_init(|| Regex::new(r"[.!?]+(?:\s|$)").expect("static pattern"))
}

fn whitespace_re() -> &'static Regex {
    static WHITESPACE: OnceLock<Regex> = OnceLock::new();
    WHITESPACE.get_or_init(|| Regex::new(r"\s+").expect("static pattern"))
}

/// Compact English stop-word set — the source's, verbatim.
pub const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "being", "but", "by", "can", "could", "did",
    "do", "does", "doing", "for", "from", "had", "has", "have", "having", "he", "her", "here",
    "hers", "him", "his", "how", "i", "if", "in", "into", "is", "it", "its", "just", "me", "more",
    "most", "my", "no", "nor", "not", "of", "off", "on", "once", "only", "or", "other", "our",
    "out", "over", "own", "she", "should", "so", "some", "such", "than", "that", "the", "their",
    "them", "then", "there", "these", "they", "this", "those", "to", "too", "under", "until", "up",
    "very", "was", "we", "were", "what", "when", "where", "which", "while", "who", "whom", "why",
    "will", "with", "would", "you", "your", "yours", "about", "above", "after", "again", "against",
    "all", "am", "any", "because", "before", "below", "between", "both", "during", "each", "few",
    "further", "now", "th", "s", "re", "ve", "ll", "don",
];

/// The page's primary content text (chrome stripped), whitespace-collapsed.
///
/// Prefers an explicit `<main>`/`<article>` region; otherwise the body with
/// `nav`/`header`/`footer`/`aside`/`form` excluded. This is the text readability and keyword
/// density reason over — the article, not the site furniture.
pub fn main_text(html: &Html) -> String {
    let root = first_named(html, "main")
        .or_else(|| first_named(html, "article"))
        .or_else(|| first_named(html, "body"));
    match root {
        Some(root) => collect_text(&root, NON_TEXT_PARENTS, BOILERPLATE_TAGS),
        None => collect_text(&html.root_element(), NON_TEXT_PARENTS, BOILERPLATE_TAGS),
    }
}

fn first_named<'a>(html: &'a Html, name: &str) -> Option<ElementRef<'a>> {
    html.select(&scraper::Selector::parse(name).expect("static selector"))
        .next()
}

/// All descendant text of `root`, joined with a single space and whitespace-collapsed.
///
/// The join is load-bearing: the source walked text nodes and joined them with `" "`, so
/// `<p>Hello</p><p>world</p>` is two words. `element.text()` concatenates without a separator
/// (that is BeautifulSoup's `get_text()` default) and produces `Helloworld` — one word. Use
/// this for prose; use [`super::text`] for a single element's label.
fn collect_text(root: &ElementRef<'_>, excluded: &[&str], also_excluded: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for node in root.descendants() {
        let Node::Text(text) = node.value() else {
            continue;
        };
        if node
            .ancestors()
            .any(|ancestor| is_excluded(ancestor.value(), excluded, also_excluded))
        {
            continue;
        }
        parts.push(text.to_string());
    }
    collapse(&parts.join(" "))
}

fn is_excluded(node: &Node, first: &[&str], second: &[&str]) -> bool {
    match node {
        Node::Element(element) => {
            let name = element.name();
            first.contains(&name) || second.contains(&name)
        }
        _ => false,
    }
}

/// `re.sub(r"\s+", " ", text).strip()`.
fn collapse(text: &str) -> String {
    whitespace_re().replace_all(text, " ").trim().to_owned()
}

/// An element's text with each text node trimmed, empties dropped, joined by a space.
///
/// This is BeautifulSoup's `get_text(" ", strip=True)` — the source's accessor for titles, H1s,
/// anchor labels and headings. It is *not* [`main_text`] (which collapses runs) and not
/// [`super::text`] (which concatenates): `<a>click <b>here</b></a>` is `"click here"` here and
/// `"click here"`-as-one-token nowhere else.
pub fn spaced_text(element: &ElementRef<'_>) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for node in element.descendants() {
        if let Node::Text(text) = node.value() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed);
            }
        }
    }
    parts.join(" ")
}

/// Lowercased alphabetic word tokens from a text blob.
pub fn words(text: &str) -> Vec<String> {
    word_re()
        .find_iter(text)
        .map(|m| m.as_str().to_ascii_lowercase())
        .collect()
}

/// Word tokens with stop-words and one-character tokens removed (for keyword term frequency).
pub fn content_words(text: &str) -> Vec<String> {
    words(text)
        .into_iter()
        .filter(|word| word.chars().count() > 1 && !STOPWORDS.contains(&word.as_str()))
        .collect()
}

/// True if `phrase` occurs as whole word(s) in `text` (word-boundary match).
///
/// Tokenizes both sides the same way, then looks for the phrase's tokens as a contiguous run, so
/// `"art"` does not match inside `"smart"`.
pub fn phrase_in_tokens(phrase: &str, text: &str) -> bool {
    let needle = words(phrase);
    if needle.is_empty() {
        return false;
    }
    let haystack = words(text);
    let n = needle.len();
    if n > haystack.len() {
        return false;
    }
    (0..=haystack.len() - n).any(|i| haystack[i..i + n] == needle[..])
}

/// Approximate sentence count (at least one when any text is present).
pub fn sentence_count(text: &str) -> usize {
    if text.trim().is_empty() {
        return 0;
    }
    let parts = sentence_split_re()
        .split(text)
        .filter(|part| !part.trim().is_empty())
        .count();
    parts.max(1)
}

const VOWELS: &[char] = &['a', 'e', 'i', 'o', 'u', 'y'];

/// Heuristic syllable count for an English word (vowel-group counting).
///
/// Counts contiguous vowel runs, then discounts a silent trailing `e`. Every word counts as at
/// least one. Stable and dependency-free — good enough for aggregate readability ratios.
pub fn syllable_count(word: &str) -> usize {
    let word = word.to_ascii_lowercase();
    let mut count = 0usize;
    let mut previous_was_vowel = false;
    for character in word.chars() {
        let is_vowel = VOWELS.contains(&character);
        if is_vowel && !previous_was_vowel {
            count += 1;
        }
        previous_was_vowel = is_vowel;
    }
    if word.ends_with('e') && count > 1 {
        count -= 1;
    }
    count.max(1)
}

/// The two readability scores, rounded to two decimals as the source rounds them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadabilityMetrics {
    pub flesch_reading_ease: f64,
    pub gunning_fog: f64,
}

/// Flesch Reading Ease and Gunning Fog for a text blob.
///
/// - Flesch Reading Ease = `206.835 - 1.015*(words/sentence) - 84.6*(syllables/word)`
/// - Gunning Fog = `0.4*((words/sentence) + 100*(complex_words/words))`, complex = >=3 syllables
///
/// Neutral values (100 FRE, 0 fog) when there is no measurable text.
pub fn readability_metrics(text: &str) -> ReadabilityMetrics {
    let tokens = words(text);
    let total = tokens.len();
    let sentences = sentence_count(text);
    if total == 0 || sentences == 0 {
        return ReadabilityMetrics {
            flesch_reading_ease: 100.0,
            gunning_fog: 0.0,
        };
    }

    let mut syllables = 0usize;
    let mut complex_words = 0usize;
    for word in &tokens {
        let count = syllable_count(word);
        syllables += count;
        if count >= 3 {
            complex_words += 1;
        }
    }

    let words_per_sentence = total as f64 / sentences as f64;
    let syllables_per_word = syllables as f64 / total as f64;
    let flesch = 206.835 - 1.015 * words_per_sentence - 84.6 * syllables_per_word;
    let fog = 0.4 * (words_per_sentence + 100.0 * (complex_words as f64 / total as f64));
    ReadabilityMetrics {
        flesch_reading_ease: round_2dp(flesch),
        gunning_fog: round_2dp(fog),
    }
}

/// Two-decimal rounding that matches CPython's `round(x, 2)`.
///
/// Rust's fixed-precision float formatting rounds the *exact binary value* with ties to even,
/// which is the algorithm CPython's `round` implements for floats. Multiplying by 100 and
/// rounding would introduce its own error and disagree on boundary values.
pub fn round_2dp(value: f64) -> f64 {
    format!("{value:.2}")
        .parse::<f64>()
        .expect("a two-decimal float parses")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(body: &str) -> Html {
        Html::parse_document(body)
    }

    #[test]
    fn main_text_joins_text_nodes_with_a_space() {
        let html = doc("<body><p>Hello</p><p>world</p></body>");
        assert_eq!(main_text(&html), "Hello world");
    }

    #[test]
    fn main_text_prefers_main_and_drops_chrome() {
        let html = doc(
            "<body><nav>Home About</nav><main><h1>Title</h1><p>Body copy.</p></main>\
             <footer>Legal</footer></body>",
        );
        assert_eq!(main_text(&html), "Title Body copy.");
    }

    #[test]
    fn main_text_drops_non_text_tags() {
        let html = doc("<body><main><p>Keep</p><script>var x = 1;</script></main></body>");
        assert_eq!(main_text(&html), "Keep");
    }

    #[test]
    fn words_splits_on_the_source_regex() {
        assert_eq!(
            words("Don't stop — state-of-the-art 42 ideas"),
            ["don't", "stop", "state-of-the-art", "ideas"]
        );
    }

    #[test]
    fn content_words_drop_stopwords_and_single_characters() {
        assert_eq!(
            content_words("the art of a smart market"),
            ["art", "smart", "market"]
        );
    }

    #[test]
    fn phrase_in_tokens_uses_word_boundaries() {
        assert!(!phrase_in_tokens("art", "smart marketing"));
        assert!(phrase_in_tokens("art", "modern art gallery"));
        assert!(phrase_in_tokens("cloud backup", "best cloud backup tools"));
        assert!(!phrase_in_tokens("cloud backup", "backup in the cloud"));
        assert!(!phrase_in_tokens("", "anything"));
        assert!(!phrase_in_tokens("a b c", "a b"));
    }

    #[test]
    fn sentence_count_is_at_least_one_for_text() {
        assert_eq!(sentence_count(""), 0);
        assert_eq!(sentence_count("   "), 0);
        assert_eq!(sentence_count("One"), 1);
        assert_eq!(sentence_count("One. Two! Three?"), 3);
    }

    #[test]
    fn syllable_count_discounts_a_silent_trailing_e() {
        assert_eq!(syllable_count("cat"), 1);
        assert_eq!(syllable_count("make"), 1);
        assert_eq!(syllable_count("water"), 2);
        assert_eq!(syllable_count("readability"), 5);
    }

    #[test]
    fn readability_metrics_are_neutral_without_text() {
        let metrics = readability_metrics("");
        assert_eq!(metrics.flesch_reading_ease, 100.0);
        assert_eq!(metrics.gunning_fog, 0.0);
    }

    #[test]
    fn rounding_matches_cpython_on_a_boundary_case() {
        // Python: round(2.675, 2) == 2.67, because 2.675 is below the exact decimal.
        assert_eq!(round_2dp(2.675), 2.67);
        // Python: round(2.685, 2) == 2.69 on this platform's double.
        assert_eq!(round_2dp(2.685), 2.69);
    }
}
