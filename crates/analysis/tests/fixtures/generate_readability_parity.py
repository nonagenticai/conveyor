#!/usr/bin/env python3
"""Generate `readability_parity.json` from the source service's text metrics.

The functions below are copied verbatim from the absorbed service's `content_utils` at the
captured revision — the word regex, the sentence split, the stop-word set, the syllable
heuristic and the two readability formulas, including `round(x, 2)`.

The point is *not* to re-derive the formulas in Rust and compare them with themselves. It is to
freeze the Python implementation's output — including CPython's half-to-even rounding — into a
file a Rust test reads, so a change to the tokenizer or the rounding fails against the source's
own numbers rather than against an assumption about them.

Run from this directory:  python3 generate_readability_parity.py > readability_parity.json
"""

import json
import re

WORD_RE = re.compile(r"[A-Za-z][A-Za-z'-]*")
SENTENCE_SPLIT_RE = re.compile(r"[.!?]+(?:\s|$)")

STOPWORDS = frozenset(
    """
    a an and are as at be been being but by can could did do does doing for from had has have
    having he her here hers him his how i if in into is it its just me more most my no nor not of
    off on once only or other our out over own she should so some such than that the their them
    then there these they this those to too under until up very was we were what when where which
    while who whom why will with would you your yours about above after again against all am any
    because before below between both during each few further now th s re ve ll don
    """.split()
)

VOWELS = "aeiouy"


def words(text):
    return [m.group(0).lower() for m in WORD_RE.finditer(text)]


def content_words(text):
    return [w for w in words(text) if len(w) > 1 and w not in STOPWORDS]


def phrase_in_tokens(phrase, text):
    needle = words(phrase)
    if not needle:
        return False
    haystack = words(text)
    n = len(needle)
    return any(haystack[i : i + n] == needle for i in range(len(haystack) - n + 1))


def sentence_count(text):
    if not text.strip():
        return 0
    parts = [p for p in SENTENCE_SPLIT_RE.split(text) if p.strip()]
    return max(len(parts), 1)


def syllable_count(word):
    word = word.lower()
    count = 0
    prev_vowel = False
    for ch in word:
        is_vowel = ch in VOWELS
        if is_vowel and not prev_vowel:
            count += 1
        prev_vowel = is_vowel
    if word.endswith("e") and count > 1:
        count -= 1
    return max(count, 1)


def readability_metrics(text):
    toks = words(text)
    total = len(toks)
    sentences = sentence_count(text)
    if not total or not sentences:
        return {"flesch_reading_ease": 100.0, "gunning_fog": 0.0}
    syllables = 0
    complex_words = 0
    for w in toks:
        syl = syllable_count(w)
        syllables += syl
        if syl >= 3:
            complex_words += 1
    words_per_sentence = total / sentences
    syllables_per_word = syllables / total
    fre = 206.835 - 1.015 * words_per_sentence - 84.6 * syllables_per_word
    fog = 0.4 * (words_per_sentence + 100 * (complex_words / total))
    return {"flesch_reading_ease": round(fre, 2), "gunning_fog": round(fog, 2)}


def repeated(word, count):
    return " ".join([word] * count)


TEXTS = [
    "",
    "One.",
    "cat",
    "cat cat cat.",
    repeated("word", 149) + ".",
    repeated("word", 150) + ".",
    repeated("word", 299) + ".",
    repeated("word", 300) + ".",
    " ".join(["cat cat cat cat cat cat cat cat cat cat."] * 40),
    " ".join(["water water water water water water water water water water."] * 40),
    (
        "The aforementioned notwithstanding, the municipality's comprehensive "
        "redevelopment initiative necessitates substantial interdepartmental "
        "coordination; consequently, stakeholders should reconsider their "
        "prioritisation methodology prior to implementation."
    ),
    "Don't stop using state-of-the-art tools, because they're easier to use and "
    "people love them. We can't go back now.",
    "123 456 !!! ...",
    (
        "Coffee tastes best when the beans are ground right before brewing. Fresh "
        "grounds keep their oils and aromas. Pre ground coffee goes flat quickly. "
        "A burr grinder gives an even grind. Even a cheap one beats a blade grinder."
    ),
    ("scalability " * 120).strip(),
]

PHRASES = [
    ("art", "smart marketing"),
    ("art", "modern art gallery"),
    ("cloud backup", "best cloud backup tools"),
    ("cloud backup", "backup in the cloud"),
    ("", "anything"),
    ("a b c", "a b"),
]

cases = []
for text in TEXTS:
    toks = words(text)
    metrics = readability_metrics(text)
    cases.append(
        {
            "text": text,
            "words": len(toks),
            "content_words": len(content_words(text)),
            "sentences": sentence_count(text),
            "syllables": sum(syllable_count(w) for w in toks),
            "flesch_reading_ease": metrics["flesch_reading_ease"],
            "gunning_fog": metrics["gunning_fog"],
        }
    )

phrases = [
    {"phrase": phrase, "text": text, "expected": phrase_in_tokens(phrase, text)}
    for phrase, text in PHRASES
]

print(json.dumps({"cases": cases, "phrases": phrases}, indent=2))
