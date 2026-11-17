//! The one Markdown renderer for a published output.
//!
//! The public page (`conveyor_pack::outputs`) and the operator's preview
//! (`documents.preview`) call this same function, so what an author previews is what the public
//! page serves. Author input is untrusted: raw HTML is displayed as text, and a link or image
//! whose URL names a non-http(s)/mailto scheme is made inert.

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

/// Render a Markdown body to HTML with raw HTML neutralised and unsafe URL schemes refused.
pub fn render_markdown(markdown: &str) -> String {
    let parser = Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    );
    let events = parser.map(|event| match event {
        // An author's raw HTML is a design note, not markup for this site. Display it as text;
        // the HTML writer escapes it. Dropping it would silently lose the author's words.
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) if !safe_href(&dest_url) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from("#"),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) if !safe_href(&dest_url) => Event::Start(Tag::Image {
            link_type,
            dest_url: CowStr::from("#"),
            title,
            id,
        }),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// A URL is safe when it is relative, a fragment, or an `http`, `https` or `mailto` URL.
///
/// The scheme is read before the first `/`, `?` or `#`; anything else is refused, including
/// `javascript:`, `data:`, `vbscript:` and `file:` with any leading whitespace or mixed case.
pub fn safe_href(raw: &str) -> bool {
    let value = raw.trim();
    if value.is_empty() || value.starts_with('#') || value.starts_with('/') {
        return true;
    }
    if value.starts_with("./") || value.starts_with("../") {
        return true;
    }
    let scheme_end = value.find(['/', '?', '#']).unwrap_or(value.len());
    let head = &value[..scheme_end];
    let Some(colon) = head.find(':') else {
        return true; // a relative URL with no scheme
    };
    let scheme = head[..colon].to_ascii_lowercase();
    matches!(scheme.as_str(), "http" | "https" | "mailto")
}

#[cfg(test)]
mod tests {
    use super::{render_markdown, safe_href};

    #[test]
    fn raw_markup_is_text_and_dangerous_schemes_are_inert() {
        let html =
            render_markdown("before <script>alert(1)</script> <img src=x onerror=alert(1)> after");
        assert!(!html.contains("<script>"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");

        let html = render_markdown("[x](javascript:alert(1)) [y](JaVaScRiPt:alert(1))");
        assert!(!html.contains("javascript:"), "{html}");
        assert_eq!(html.matches("href=\"#\"").count(), 2, "{html}");
    }

    #[test]
    fn safe_schemes_survive() {
        let html = render_markdown(
            "[docs](https://example.com/a) [mail](mailto:a@b.c) [rel](./a.md) [frag](#x)",
        );
        assert!(html.contains("https://example.com/a"), "{html}");
        assert!(html.contains("mailto:a@b.c"), "{html}");
        assert!(html.contains("./a.md"), "{html}");
        assert!(html.contains("#x"), "{html}");
    }

    #[test]
    fn safe_href_reads_the_scheme_before_the_path() {
        assert!(safe_href("https://example.com/a:b"));
        assert!(!safe_href("data:text/html,<script>"));
        assert!(!safe_href(" file:///etc/passwd"));
    }
}
