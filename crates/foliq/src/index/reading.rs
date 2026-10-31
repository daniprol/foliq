//! References, `outline` and `read`.
//!
//! A *reference* names a document and, optionally, a part of it:
//!
//! | Reference                 | Means                                          |
//! |---------------------------|------------------------------------------------|
//! | `notes/billing.md`        | the whole document (collection `notes`)        |
//! | `notes/billing.md#2.3`    | section 2.3 and its subsections                |
//! | `contracts/acme.pdf#p4`   | page 4                                         |
//! | `contracts/acme.pdf#p4-6` | pages 4 to 6                                   |
//! | `billing.md`              | works too, when only one document matches      |
//!
//! Every citation that foliq prints is also a valid reference, so an agent can
//! go from a search result to the full section in one step.

use foliq_core::{Document, ElementKind, Error, Filter, Result};
use serde::Serialize;

use super::Index;
use super::citation::citation;
use crate::tokens::{count_tokens, truncate_to_tokens};

/// The table of contents of a document.
#[derive(Debug, Clone, Serialize)]
pub struct Outline {
    pub collection: String,
    pub path: String,
    pub title: String,
    pub token_count: usize,
    /// The number of pages (or slides), for formats that have them.
    pub pages: Option<u32>,
    pub sections: Vec<OutlineSection>,
}

/// One line of an outline.
#[derive(Debug, Clone, Serialize)]
pub struct OutlineSection {
    /// Use it in a reference: `notes/billing.md#2.3`.
    pub number: String,
    pub title: String,
    pub level: u8,
    /// Tokens in the section, subsections included: what `read` would return.
    pub token_count: usize,
    pub page: Option<u32>,
    pub line: usize,
}

/// A piece of a document: the result of `read`, and each part of a `context`.
#[derive(Debug, Clone, Serialize)]
pub struct Passage {
    /// Where the text comes from, like `notes/billing.md#2.3 (L40-61)`.
    pub citation: String,
    pub collection: String,
    pub path: String,
    pub title: String,
    pub section: Option<String>,
    pub heading_path: Vec<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub page_start: Option<u32>,
    pub page_end: Option<u32>,
    pub text: String,
    pub token_count: usize,
    /// True when the text was cut to fit the token budget.
    pub truncated: bool,
}

/// The part of a document a reference points to.
enum Part {
    Whole,
    Section(String),
    Pages(u32, u32),
}

impl Index {
    /// The outline of a document: its sections with their numbers and sizes.
    pub fn outline(&self, reference: &str, principals: Option<&[String]>) -> Result<Outline> {
        let (document, _) = self.resolve(reference, principals)?;
        let sections = document
            .sections()
            .into_iter()
            .map(|section| OutlineSection {
                token_count: count_tokens(&document.text[section.start..section.end]),
                page: document.page_at(section.start),
                line: document.line_at(section.start),
                number: section.number,
                title: section.title,
                level: section.level,
            })
            .collect();
        Ok(Outline {
            pages: last_page(&document),
            token_count: count_tokens(&document.text),
            collection: document.collection,
            path: document.path,
            title: document.title,
            sections,
        })
    }

    /// Reads a document, a section or pages, cut to `budget` tokens when given.
    pub fn read(&self, reference: &str, budget: Option<usize>, principals: Option<&[String]>) -> Result<Passage> {
        let (document, part) = self.resolve(reference, principals)?;
        let (start, end, section) = match part {
            Part::Whole => (skip_front_matter(&document, 0), document.text.len(), None),
            Part::Section(number) => {
                let found = document.section(&number).ok_or_else(|| {
                    Error::NotFound(format!(
                        "section {number} in {}/{} (see its outline)",
                        document.collection, document.path
                    ))
                })?;
                (skip_front_matter(&document, found.start), found.end, Some(found))
            }
            Part::Pages(first, last) => {
                let (start, end) = page_range(&document, first, last).ok_or_else(|| {
                    Error::NotFound(format!(
                        "pages {first}-{last} in {}/{}",
                        document.collection, document.path
                    ))
                })?;
                (start, end, None)
            }
        };
        let full_text = document.text[start..end].trim_end();
        let (text, truncated) = match budget {
            Some(budget) => truncate_to_tokens(full_text, budget),
            None => (full_text, false),
        };
        Ok(passage(
            &document,
            start,
            start + text.len(),
            section.map(|found| (found.number, found.heading_path)),
            truncated,
        ))
    }

    /// Finds the document (and part) a reference points to, if the caller may see it.
    fn resolve(&self, reference: &str, principals: Option<&[String]>) -> Result<(Document, Part)> {
        let (target, part) = parse_reference(reference)?;

        // First try `collection/path`.
        for collection in self.collections()? {
            if let Some(path) = target.strip_prefix(&format!("{}/", collection.name))
                && let Some(document) = self.store.document(&collection.name, path)?
                && document.is_visible_to(principals)
            {
                return Ok((document, part));
            }
        }

        // Then a path, or the end of a path, in any collection.
        let filter = Filter {
            principals: principals.map(<[String]>::to_vec),
            ..Filter::default()
        };
        let matches: Vec<_> = self
            .store
            .documents(&filter)?
            .into_iter()
            .filter(|info| {
                let full = format!("{}/{}", info.collection, info.path);
                info.path == target || full.ends_with(&format!("/{target}"))
            })
            .collect();
        match matches.as_slice() {
            [] => Err(Error::NotFound(format!("no document matches {target:?}"))),
            [only] => {
                let document = self
                    .store
                    .document(&only.collection, &only.path)?
                    .ok_or_else(|| Error::NotFound(format!("no document matches {target:?}")))?;
                Ok((document, part))
            }
            many => {
                let names: Vec<String> = many
                    .iter()
                    .map(|info| format!("{}/{}", info.collection, info.path))
                    .collect();
                Err(Error::Invalid(format!(
                    "{target:?} matches several documents: {}",
                    names.join(", ")
                )))
            }
        }
    }
}

/// Builds a passage for `document.text[start..end]`.
pub(super) fn passage(
    document: &Document,
    start: usize,
    end: usize,
    section: Option<(String, Vec<String>)>,
    truncated: bool,
) -> Passage {
    let last = end.saturating_sub(1).max(start);
    let (line_start, line_end) = (document.line_at(start), document.line_at(last));
    let (page_start, page_end) = (document.page_at(start), document.page_at(last));
    let (section, heading_path) = match section {
        Some((number, path)) => (Some(number), path),
        None => (None, Vec::new()),
    };
    let text = document.text[start..end].to_string();
    Passage {
        citation: citation(
            &document.collection,
            &document.path,
            section.as_deref(),
            (page_start, page_end),
            (line_start, line_end),
        ),
        collection: document.collection.clone(),
        path: document.path.clone(),
        title: document.title.clone(),
        section,
        heading_path,
        line_start,
        line_end,
        page_start,
        page_end,
        token_count: count_tokens(&text),
        text,
        truncated,
    }
}

/// Splits `notes/a.md#2.3` into the document part and the part after `#`.
fn parse_reference(reference: &str) -> Result<(&str, Part)> {
    let reference = reference.trim();
    let (target, fragment) = match reference.rsplit_once('#') {
        Some((target, fragment)) => (target, Some(fragment)),
        None => (reference, None),
    };
    // A citation pasted whole (`a.md#2 (L3-9)`) still works: drop the details.
    let fragment = fragment.map(|fragment| fragment.split_whitespace().next().unwrap_or(""));
    let part = match fragment {
        None | Some("") => Part::Whole,
        Some(fragment) => match fragment.strip_prefix('p').or_else(|| fragment.strip_prefix('P')) {
            Some(pages) => {
                let (first, last) = pages.split_once('-').unwrap_or((pages, pages));
                match (first.parse(), last.parse()) {
                    (Ok(first), Ok(last)) if first <= last => Part::Pages(first, last),
                    _ => {
                        return Err(Error::Invalid(format!(
                            "bad page range in {reference:?}; use #p4 or #p4-6"
                        )));
                    }
                }
            }
            None => Part::Section(fragment.to_string()),
        },
    };
    let target = target.split(" (").next().unwrap_or(target).trim();
    Ok((target, part))
}

/// Moves `start` past front matter that begins there.
fn skip_front_matter(document: &Document, start: usize) -> usize {
    document
        .elements
        .iter()
        .find(|element| element.kind == ElementKind::FrontMatter && element.start <= start && start < element.end)
        .map_or(start, |front_matter| front_matter.end)
}

/// The byte range from the start of page `first` to the end of page `last`.
fn page_range(document: &Document, first: u32, last: u32) -> Option<(usize, usize)> {
    let page_starts: Vec<(u32, usize)> = document
        .elements
        .iter()
        .filter_map(|element| match element.kind {
            ElementKind::PageBreak { page } => Some((page, element.start)),
            _ => None,
        })
        .collect();
    let start = page_starts.iter().find(|(page, _)| *page == first)?.1;
    let end = page_starts
        .iter()
        .find(|(page, _)| *page > last)
        .map_or(document.text.len(), |next| next.1);
    Some((start, end))
}

/// The highest page number in a document, if it has pages.
fn last_page(document: &Document) -> Option<u32> {
    document
        .elements
        .iter()
        .filter_map(|element| match element.kind {
            ElementKind::PageBreak { page } => Some(page),
            _ => None,
        })
        .max()
}
