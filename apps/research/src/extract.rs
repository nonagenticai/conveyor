//! HTML -> visible text and Markdown.
//!
//! This is the reference service's **boilerplate-strip baseline**, ported from
//! `services/content_extraction.py`. The primary extractor (trafilatura) has no
//! Rust counterpart, so the baseline is always used here; the contract's
//! declared deviation and the parity test's similarity bound cover the two
//! fixtures where the reference's primary extractor won.
//!
//! Baseline: remove `script/style/noscript/nav/header/footer/aside/form`, drop
//! consent/dialog containers by class/id/role/aria, linearise table rows as
//! `cell | cell`, then join the surviving text nodes with newlines. Markdown is
//! rendered from the same stripped tree with markdownify-compatible formatting
//! (ATX headings, `*` bullets, `| a | b |` tables, `[text](href)` links).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use serde_json::Value;

const BOILERPLATE_TAGS: [&str; 8] = [
    "script", "style", "noscript", "nav", "header", "footer", "aside", "form",
];

const CONSENT_SUBSTRINGS: [&str; 3] = ["cookie", "consent", "gdpr"];
const DIALOG_ROLES: [&str; 3] = ["dialog", "alertdialog", "banner"];

/// The two fields the reference derives from one HTML document.
#[derive(Debug, Clone)]
pub struct Extraction {
    pub text_content: String,
    pub markdown_content: Option<String>,
}

pub fn extract(html: &str) -> Extraction {
    if html.is_empty() {
        return Extraction {
            text_content: String::new(),
            markdown_content: None,
        };
    }
    let document = Html::parse_document(html);
    let text_content = baseline_text(&document);

    let rendered = render_document(&document);
    let markdown_content = if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    };
    Extraction {
        text_content,
        markdown_content,
    }
}

/// The page title, matching `soup.title.string.strip()` for the captured pages.
pub fn title(document: &Html) -> Option<String> {
    let selector = selector("title");
    let element = document.select(&selector).next()?;
    let text = element.text().collect::<String>();
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Page metadata: every `<meta name|property content>`, the canonical link, and
/// the authoritative transport values (which are inserted last, as the
/// reference does, so a scraped meta tag cannot clobber them).
pub fn metadata(document: &Html, status_code: u16, final_url: &str) -> BTreeMap<String, Value> {
    let mut metadata = BTreeMap::new();

    let meta_selector = selector("meta");
    for meta in document.select(&meta_selector) {
        let name = meta.attr("name").or_else(|| meta.attr("property"));
        let content = meta.attr("content");
        if let (Some(name), Some(content)) = (name, content) {
            metadata.insert(name.to_string(), Value::String(content.to_string()));
        }
    }

    let canonical_selector = selector("link[rel=canonical]");
    if let Some(canonical) = document.select(&canonical_selector).next()
        && let Some(href) = canonical.attr("href")
    {
        metadata.insert("canonical_url".to_string(), Value::String(href.to_string()));
    }

    metadata.insert("status_code".to_string(), Value::from(status_code));
    metadata.insert(
        "final_url".to_string(),
        Value::String(final_url.to_string()),
    );
    metadata
}

/// Text of an element and its descendants, joined with `separator`.
///
/// Mirrors BeautifulSoup's `get_text(separator=..., strip=True)`: each text node
/// is stripped, empty ones are dropped, and the survivors are joined.
pub(crate) fn text_with_separator(element: &ElementRef<'_>, separator: &str) -> String {
    let mut pieces = Vec::new();
    collect_text_nodes(*element, &mut pieces);
    pieces
        .into_iter()
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>()
        .join(separator)
}

fn collect_text_nodes<'a>(element: ElementRef<'a>, out: &mut Vec<String>) {
    for child in element.children() {
        match child.value() {
            Node::Text(text) => {
                let text = text.trim();
                if !text.is_empty() {
                    out.push(text.to_string());
                }
            }
            Node::Element(_) => {
                if let Some(child) = ElementRef::wrap(child) {
                    collect_text_nodes(child, out);
                }
            }
            _ => {}
        }
    }
}

/// The boilerplate-strip baseline: one line per surviving text node, with every
/// table row collapsed to `cell | cell`.
fn baseline_text(document: &Html) -> String {
    let mut lines = Vec::new();
    collect_baseline_lines(document.root_element(), &mut lines);
    lines.join("\n")
}

fn collect_baseline_lines(element: ElementRef<'_>, lines: &mut Vec<String>) {
    if is_boilerplate(&element) {
        return;
    }

    if element.value().name() == "tr" {
        let cells: Vec<String> = element
            .child_elements()
            .filter(|child| matches!(child.value().name(), "td" | "th"))
            .map(|cell| text_with_separator(&cell, " "))
            .collect();
        if !cells.is_empty() {
            lines.push(cells.join(" | "));
            return;
        }
    }

    for child in element.children() {
        match child.value() {
            Node::Text(text) => {
                let text = text.trim();
                if !text.is_empty() {
                    lines.push(text.to_string());
                }
            }
            Node::Element(_) => {
                if let Some(child) = ElementRef::wrap(child) {
                    collect_baseline_lines(child, lines);
                }
            }
            _ => {}
        }
    }
}

fn is_boilerplate(element: &ElementRef<'_>) -> bool {
    if BOILERPLATE_TAGS.contains(&element.value().name()) {
        return true;
    }
    if element.attr("role") == Some("navigation") {
        return true;
    }
    if let Some(role) = element.attr("role")
        && DIALOG_ROLES.contains(&role.to_ascii_lowercase().as_str())
    {
        return true;
    }
    if element.attr("aria-modal") == Some("true") {
        return true;
    }
    let class_id = format!(
        "{} {}",
        element.attr("class").unwrap_or_default(),
        element.attr("id").unwrap_or_default()
    )
    .to_ascii_lowercase();
    CONSENT_SUBSTRINGS
        .iter()
        .any(|word| class_id.contains(word))
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "blockquote"
            | "article"
            | "div"
            | "section"
            | "ol"
            | "ul"
            | "li"
            | "dl"
            | "dt"
            | "dd"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "td"
            | "th"
    ) || name.starts_with('h') && name.len() == 2 && name.as_bytes()[1].is_ascii_digit()
}

fn should_remove_inside(element: &ElementRef<'_>) -> bool {
    let name = element.value().name();
    is_block(name)
        || (name.starts_with('h') && name.len() == 2 && name.as_bytes()[1].is_ascii_digit())
}

fn previous_element_sibling<'a>(element: ElementRef<'a>) -> Option<ElementRef<'a>> {
    element.prev_siblings().find_map(ElementRef::wrap)
}

fn next_element_sibling<'a>(element: ElementRef<'a>) -> Option<ElementRef<'a>> {
    element.next_siblings().find_map(ElementRef::wrap)
}

fn parent_element<'a>(element: ElementRef<'a>) -> Option<ElementRef<'a>> {
    element.parent().and_then(ElementRef::wrap)
}

/// Render the document the way markdownify did in the reference backstop.
fn render_document(document: &Html) -> String {
    let rendered = render_element(document.root_element(), false);
    let collapsed = collapse_newlines(&rendered);
    collapsed.trim().to_string()
}

fn collapse_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0usize;
    for character in text.chars() {
        if character == '\n' {
            run += 1;
            continue;
        }
        if run > 0 {
            out.push_str(if run >= 2 { "\n\n" } else { "\n" });
            run = 0;
        }
        out.push(character);
    }
    if run > 0 {
        out.push_str(if run >= 2 { "\n\n" } else { "\n" });
    }
    out
}

fn render_element(element: ElementRef<'_>, inline: bool) -> String {
    if is_boilerplate(&element) {
        return String::new();
    }

    let name = element.value().name();

    if name == "img" {
        let alt = element.attr("alt").unwrap_or_default();
        if inline {
            return alt.to_string();
        }
        let src = element.attr("src").unwrap_or_default();
        let title = element.attr("title").unwrap_or_default();
        let title_part = if title.is_empty() {
            String::new()
        } else {
            format!(" \"{}\"", title.replace('"', "\\\""))
        };
        return format!("![{alt}]({src}{title_part})");
    }

    if name == "br" {
        return if inline {
            " ".to_string()
        } else {
            "  \n".to_string()
        };
    }

    let child_inline = inline || is_heading(name) || matches!(name, "td" | "th");
    let text = render_children(element, child_inline);

    if is_heading(name) {
        let depth = name.as_bytes()[1] - b'0';
        let text = collapse_all_whitespace(text.trim());
        return format!("\n\n{} {}\n\n", "#".repeat(depth as usize), text);
    }

    match name {
        "p" => block(&text),
        "div" | "article" | "section" => block(&text),
        "table" => block(&text),
        "td" | "th" => {
            let colspan = element
                .attr("colspan")
                .and_then(|value| value.parse::<usize>().ok())
                .map(|value| value.clamp(1, 1000))
                .unwrap_or(1);
            let cell = text.trim().replace('\n', " ");
            format!(" {cell}{}", " |".repeat(colspan))
        }
        "tr" => render_row(element, &text),
        "a" => render_link(element, &text),
        "ul" | "ol" => render_list(element, &text),
        "li" => render_list_item(element, &text),
        _ => text,
    }
}

fn is_heading(name: &str) -> bool {
    name.len() == 2 && name.starts_with('h') && name.as_bytes()[1].is_ascii_digit()
}

fn block(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        String::new()
    } else {
        format!("\n\n{text}\n\n")
    }
}

fn render_children(element: ElementRef<'_>, inline: bool) -> String {
    let parent_block = should_remove_inside(&element);
    let nodes: Vec<_> = element.children().collect();
    let mut rendered = String::new();

    for (index, node) in nodes.iter().enumerate() {
        let previous_is_block = index > 0 && node_is_block(nodes[index - 1].value());
        let next_is_block = index + 1 < nodes.len() && node_is_block(nodes[index + 1].value());

        match node.value() {
            Node::Text(text) => {
                if text.trim().is_empty()
                    && ((parent_block && (index == 0 || index + 1 == nodes.len()))
                        || previous_is_block
                        || next_is_block)
                {
                    continue;
                }
                let mut text = normalize_whitespace(text);
                if previous_is_block || (parent_block && index == 0) {
                    text = text.trim_start_matches([' ', '\t', '\r', '\n']).to_string();
                }
                if next_is_block || (parent_block && index + 1 == nodes.len()) {
                    text = text.trim_end().to_string();
                }
                rendered.push_str(&text);
            }
            Node::Element(_) => {
                if let Some(child) = ElementRef::wrap(*node) {
                    rendered.push_str(&render_element(child, inline));
                }
            }
            _ => {}
        }
    }

    rendered
}

fn node_is_block(node: &scraper::node::Node) -> bool {
    node.as_element()
        .is_some_and(|element| is_block(element.name()))
}

fn normalize_whitespace(text: &str) -> String {
    static NEWLINE_RUN: OnceLock<Regex> = OnceLock::new();
    static SPACE_RUN: OnceLock<Regex> = OnceLock::new();
    let newline_run =
        NEWLINE_RUN.get_or_init(|| Regex::new(r"[\t \r\n]*[\r\n][\t \r\n]*").expect("regex"));
    let space_run = SPACE_RUN.get_or_init(|| Regex::new(r"[\t ]+").expect("regex"));
    space_run
        .replace_all(&newline_run.replace_all(text, "\n"), " ")
        .to_string()
}

fn collapse_all_whitespace(text: &str) -> String {
    static ALL_WHITESPACE: OnceLock<Regex> = OnceLock::new();
    let regex = ALL_WHITESPACE.get_or_init(|| Regex::new(r"[\t \r\n]+").expect("regex"));
    regex.replace_all(text, " ").to_string()
}

fn render_link(element: ElementRef<'_>, text: &str) -> String {
    let prefix = if text.starts_with(' ') { " " } else { "" };
    let suffix = if text.ends_with(' ') { " " } else { "" };
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let Some(href) = element.attr("href") else {
        return text.to_string();
    };
    let title = element.attr("title").unwrap_or_default();
    if title.is_empty() && text == href {
        return format!("<{href}>");
    }
    let title_part = if title.is_empty() {
        String::new()
    } else {
        format!(" \"{}\"", title.replace('"', "\\\""))
    };
    format!("{prefix}[{text}]({href}{title_part}){suffix}")
}

fn render_row(element: ElementRef<'_>, text: &str) -> String {
    let cells: Vec<ElementRef<'_>> = element.select(&selector("td, th")).collect();
    let is_first_row = previous_element_sibling(element).is_none();
    let parent = parent_element(element);
    let parent_name = parent
        .map(|parent| parent.value().name().to_string())
        .unwrap_or_default();
    let thead_has_single_row = parent
        .filter(|parent| parent.value().name() == "thead")
        .map(|parent| parent.select(&selector("tr")).count() == 1)
        .unwrap_or(false);
    let is_headrow = (!cells.is_empty() && cells.iter().all(|cell| cell.value().name() == "th"))
        || thead_has_single_row;

    let table = match parent {
        Some(parent) if parent.value().name() == "table" => Some(parent),
        Some(parent) if parent.value().name() == "tbody" => parent_element(parent),
        _ => None,
    };
    let table_has_thead = table
        .map(|table| table.select(&selector("thead")).next().is_some())
        .unwrap_or(false);
    let is_head_row_missing = (is_first_row && parent_name != "tbody")
        || (is_first_row && parent_name == "tbody" && !table_has_thead);

    let colspan: usize = if cells.is_empty() {
        1
    } else {
        cells
            .iter()
            .map(|cell| {
                cell.attr("colspan")
                    .and_then(|value| value.parse::<usize>().ok())
                    .map(|value| value.clamp(1, 1000))
                    .unwrap_or(1)
            })
            .sum()
    };

    let mut overline = String::new();
    let mut underline = String::new();
    if is_headrow && is_first_row {
        underline.push_str(&separator_row(colspan));
    } else if is_head_row_missing {
        overline.push_str(&header_row(&vec![String::new(); colspan]));
        overline.push_str(&separator_row(colspan));
    }

    format!("{overline}|{text}\n{underline}")
}

fn header_row(cells: &[String]) -> String {
    format!("| {} |\n", cells.join(" | "))
}

fn separator_row(colspan: usize) -> String {
    format!("| {} |\n", vec!["---"; colspan].join(" | "))
}

fn render_list(element: ElementRef<'_>, text: &str) -> String {
    let parent_is_li = parent_element(element)
        .map(|parent| parent.value().name() == "li")
        .unwrap_or(false);
    if parent_is_li {
        return format!("\n{}", text.trim_end());
    }

    let before_paragraph = next_element_sibling(element)
        .map(|sibling| !matches!(sibling.value().name(), "ul" | "ol"))
        .unwrap_or(false);
    format!("\n\n{}{}", text, if before_paragraph { "\n" } else { "" })
}

fn render_list_item(element: ElementRef<'_>, text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return "\n".to_string();
    }

    let bullet = match parent_element(element) {
        Some(parent) if parent.value().name() == "ol" => {
            let start = parent
                .attr("start")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1);
            let before = parent
                .child_elements()
                .take_while(|child| *child != element)
                .filter(|child| child.value().name() == "li")
                .count();
            format!("{}. ", start + before)
        }
        _ => {
            let mut depth: i32 = -1;
            let mut current = Some(element);
            while let Some(node) = current {
                if node.value().name() == "ul" {
                    depth += 1;
                }
                current = parent_element(node);
            }
            let bullets = ['*', '+', '-'];
            let bullet = bullets[depth.max(0) as usize % bullets.len()];
            format!("{bullet} ")
        }
    };

    let indent = " ".repeat(bullet.len());
    let mut lines = text.lines();
    let mut out = String::new();
    if let Some(first) = lines.next() {
        out.push_str(&format!("{bullet}{first}\n"));
    }
    for line in lines {
        out.push_str(&indent);
        out.push_str(line);
        out.push('\n');
    }
    out
}
