//! HTML pages: the main content becomes Markdown; menus, scripts and footers are dropped.

use foliq_core::{Parsed, Parser, Result, SourceEntry};
use scraper::{ElementRef, Html, Node, Selector};

use crate::markdown::MarkdownWriter;

pub struct HtmlParser;

impl Parser for HtmlParser {
    fn name(&self) -> &str {
        "html"
    }

    fn extensions(&self) -> Vec<String> {
        super::extensions(&["html", "htm", "xhtml"])
    }

    fn parse(&self, bytes: &[u8], _entry: &SourceEntry) -> Result<Parsed> {
        Ok(html_to_markdown(&super::bytes_to_text(bytes)))
    }
}

/// Tags whose content is never part of the document's text.
const SKIPPED_TAGS: &[&str] = &[
    "script", "style", "noscript", "nav", "footer", "aside", "form", "svg", "template", "iframe", "button", "select",
    "head",
];

/// Tags that start a new block (everything else is inline text, like `<a>` or `<b>`).
const BLOCK_TAGS: &[&str] = &[
    "address",
    "article",
    "blockquote",
    "body",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "main",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "ul",
    "br",
];

pub fn html_to_markdown(html: &str) -> Parsed {
    let page = Html::parse_document(html);

    // Prefer the page's main content when it is marked, so menus don't pollute search.
    let root = ["main", "article", "[role=main]", "body"]
        .iter()
        .find_map(|selector| page.select(&Selector::parse(selector).expect("valid selector")).next())
        .unwrap_or_else(|| page.root_element());

    let title = page
        .select(&Selector::parse("title").expect("valid selector"))
        .next()
        .map(|element| {
            element
                .text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|title| !title.is_empty());

    let mut writer = MarkdownWriter::new();
    write_children(root, &mut writer, 0);
    Parsed {
        text: writer.finish(),
        title,
        ..Parsed::default()
    }
}

/// Writes an element's children: inline text is gathered into paragraphs,
/// block elements are written on their own.
fn write_children(element: ElementRef, writer: &mut MarkdownWriter, list_depth: usize) {
    let mut pending_inline_text = String::new();
    for child in element.children() {
        match child.value() {
            Node::Text(text) => pending_inline_text.push_str(text),
            Node::Element(_) => {
                let child = ElementRef::wrap(child).expect("an element node");
                let tag = child.value().name();
                if SKIPPED_TAGS.contains(&tag) {
                    continue;
                }
                if BLOCK_TAGS.contains(&tag) {
                    writer.paragraph(&pending_inline_text);
                    pending_inline_text.clear();
                    write_block(child, writer, list_depth);
                } else {
                    pending_inline_text.push(' ');
                    pending_inline_text.push_str(&inline_text(child));
                    pending_inline_text.push(' ');
                }
            }
            _ => {}
        }
    }
    writer.paragraph(&pending_inline_text);
}

fn write_block(element: ElementRef, writer: &mut MarkdownWriter, list_depth: usize) {
    let tag = element.value().name();
    match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = tag[1..].parse().unwrap_or(1);
            writer.heading(level, &inline_text(element));
        }
        "p" | "summary" | "dt" | "dd" | "figcaption" | "address" => writer.paragraph(&inline_text(element)),
        "pre" => writer.code(&element.text().collect::<String>()),
        "blockquote" => {
            let text = inline_text(element);
            if !text.is_empty() {
                writer.raw(&format!("> {text}"));
            }
        }
        "ul" | "ol" => {
            for item in element.child_elements().filter(|child| child.value().name() == "li") {
                write_list_item(item, writer, list_depth);
            }
        }
        "li" => write_list_item(element, writer, list_depth),
        "table" => writer.table(&table_rows(element)),
        "hr" | "br" => {}
        _ => write_children(element, writer, list_depth),
    }
}

/// A list item: its own text as a bullet, then any nested lists one level deeper.
fn write_list_item(item: ElementRef, writer: &mut MarkdownWriter, depth: usize) {
    let mut own_text = String::new();
    let mut nested_lists = Vec::new();
    for child in item.children() {
        match child.value() {
            Node::Text(text) => own_text.push_str(text),
            Node::Element(element) if matches!(element.name(), "ul" | "ol") => {
                nested_lists.push(ElementRef::wrap(child).expect("an element node"));
            }
            Node::Element(element) if !SKIPPED_TAGS.contains(&element.name()) => {
                own_text.push(' ');
                own_text.push_str(&inline_text(ElementRef::wrap(child).expect("an element node")));
            }
            _ => {}
        }
    }
    writer.list_item(&own_text, depth);
    for list in nested_lists {
        write_block(list, writer, depth + 1);
    }
}

fn table_rows(table: ElementRef) -> Vec<Vec<String>> {
    let row_selector = Selector::parse("tr").expect("valid selector");
    let cell_selector = Selector::parse("th, td").expect("valid selector");
    table
        .select(&row_selector)
        .map(|row| row.select(&cell_selector).map(inline_text).collect::<Vec<_>>())
        .filter(|cells| !cells.is_empty())
        .collect()
}

/// All the text inside an element, on one line, without scripts or styles.
fn inline_text(element: ElementRef) -> String {
    let mut text = String::new();
    for node in element.descendants() {
        if let Node::Text(piece) = node.value() {
            let inside_skipped_tag = node.ancestors().any(|ancestor| {
                ancestor
                    .value()
                    .as_element()
                    .is_some_and(|element| SKIPPED_TAGS.contains(&element.name()))
            });
            if !inside_skipped_tag {
                text.push_str(piece);
            }
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_the_main_content() {
        let html = r#"<html><head><title>Help</title><style>p{}</style></head><body>
            <nav><a href="/">Home</a></nav>
            <main>
              <h1>Refunds</h1>
              <p>Refunds take <b>5-7</b> days.</p>
              Loose text
              <ul><li>Cards<ul><li>Visa</li></ul></li><li>Bank</li></ul>
              <table><tr><th>Plan</th><th>Days</th></tr><tr><td>Annual</td><td>30</td></tr></table>
              <script>alert(1)</script>
            </main><footer>Copyright</footer></body></html>"#;
        let parsed = html_to_markdown(html);
        assert_eq!(parsed.title.as_deref(), Some("Help"));
        assert_eq!(
            parsed.text,
            "# Refunds\n\nRefunds take 5-7 days.\n\nLoose text\n\n- Cards\n  - Visa\n- Bank\n\n| Plan | Days |\n| --- | --- |\n| Annual | 30 |\n"
        );
    }
}
