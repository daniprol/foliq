//! `search` (ranked chunks) and `context` (the best passages packed into a token budget).

use std::collections::HashMap;

use foliq_core::{Document, Filter, Result, SearchRequest};
use serde::Serialize;

use super::Index;
use super::citation::citation;
use super::reading::{Passage, passage};
use crate::tokens::count_tokens;

/// Where to search, and for whom.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// The most results to return.
    pub limit: usize,
    /// Only these collections. Empty means all.
    pub collections: Vec<String>,
    /// Only documents whose path starts with this, like `contracts/2024/`.
    pub path_prefix: Option<String>,
    /// Who is asking (`["user:ana", "group:hr"]`). `None` skips access checks.
    pub principals: Option<Vec<String>>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions {
            limit: 10,
            collections: Vec::new(),
            path_prefix: None,
            principals: None,
        }
    }
}

impl SearchOptions {
    fn filter(&self) -> Filter {
        Filter {
            collections: self.collections.clone(),
            path_prefix: self.path_prefix.clone(),
            principals: self.principals.clone(),
        }
    }
}

/// One search result: a chunk with everything needed to cite it.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    /// Where the chunk comes from; also a reference for `read`.
    pub citation: String,
    pub collection: String,
    pub path: String,
    pub title: String,
    pub section: String,
    pub heading_path: Vec<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub page_start: Option<u32>,
    pub page_end: Option<u32>,
    /// Higher is better. Only comparable within one search.
    pub score: f64,
    pub text: String,
    pub token_count: usize,
}

/// The answer to `context`: passages ready to paste into a prompt.
#[derive(Debug, Clone, Serialize)]
pub struct Context {
    /// All passages, each under its `[n] citation` line.
    pub text: String,
    pub token_count: usize,
    pub passages: Vec<Passage>,
}

/// A whole section is used instead of a chunk when it fits in this share of the budget.
const SECTION_SHARE_OF_BUDGET: usize = 3;
/// Search this many chunks to fill a context, at least.
const CONTEXT_SEARCH_LIMIT: usize = 30;

impl Index {
    /// Keyword search (BM25), best results first.
    pub fn search(&self, query: &str, options: &SearchOptions) -> Result<Vec<SearchResult>> {
        let request = SearchRequest {
            query: query.to_string(),
            limit: options.limit,
            filter: options.filter(),
        };
        let hits = self.store.search(&request)?;
        Ok(hits
            .into_iter()
            .map(|hit| {
                let chunk = hit.chunk;
                SearchResult {
                    citation: citation(
                        &hit.collection,
                        &hit.path,
                        Some(&chunk.section),
                        (chunk.page_start, chunk.page_end),
                        (chunk.line_start, chunk.line_end),
                    ),
                    collection: hit.collection,
                    path: hit.path,
                    title: hit.title,
                    section: chunk.section,
                    heading_path: chunk.heading_path,
                    line_start: chunk.line_start,
                    line_end: chunk.line_end,
                    page_start: chunk.page_start,
                    page_end: chunk.page_end,
                    score: hit.score,
                    token_count: chunk.token_count,
                    text: chunk.text,
                }
            })
            .collect())
    }

    /// The best passages for a question, cited and packed into `budget` tokens.
    ///
    /// Each search hit grows to its whole section when the section is small
    /// enough, so the model sees complete thoughts instead of fragments.
    /// Overlapping passages are used once.
    pub fn context(&self, query: &str, budget: usize, options: &SearchOptions) -> Result<Context> {
        let request = SearchRequest {
            query: query.to_string(),
            limit: options.limit.max(CONTEXT_SEARCH_LIMIT),
            filter: options.filter(),
        };
        let hits = self.store.search(&request)?;

        let mut documents: HashMap<String, Document> = HashMap::new();
        let mut used_ranges: Vec<(String, usize, usize)> = Vec::new();
        let mut passages: Vec<Passage> = Vec::new();
        let mut tokens_used = 0;

        for hit in hits {
            let chunk = &hit.chunk;
            if !documents.contains_key(&chunk.document_id) {
                let Some(document) = self.store.document(&hit.collection, &hit.path)? else {
                    continue;
                };
                documents.insert(chunk.document_id.clone(), document);
            }
            let document = &documents[&chunk.document_id];

            // Try the whole section first, then the chunk alone.
            let mut candidates = Vec::new();
            if let Some(section) = document.section(&chunk.section) {
                let section_tokens = count_tokens(&document.text[section.start..section.own_end]);
                let fits_share = section_tokens <= budget / SECTION_SHARE_OF_BUDGET;
                if fits_share && section.start <= chunk.start && chunk.end <= section.own_end {
                    candidates.push((section.start, section.own_end));
                }
            }
            candidates.push((chunk.start, chunk.end));

            for (start, end) in candidates {
                let overlaps = used_ranges
                    .iter()
                    .any(|(id, used_start, used_end)| *id == document.id && start < *used_end && *used_start < end);
                if overlaps {
                    break; // Already in the context (or part of it is).
                }
                let section = (chunk.section.clone(), chunk.heading_path.clone());
                let text_end = start + document.text[start..end].trim_end().len();
                let found = passage(document, start, text_end, Some(section), false);
                // The citation line costs tokens too.
                let cost = found.token_count + count_tokens(&found.citation) + 4;
                if tokens_used + cost > budget {
                    continue; // Too big: try the smaller candidate, or the next hit.
                }
                tokens_used += cost;
                used_ranges.push((document.id.clone(), start, end));
                passages.push(found);
                break;
            }
            if passages.len() >= options.limit.max(1) || budget.saturating_sub(tokens_used) < 20 {
                break;
            }
        }

        let text = passages
            .iter()
            .enumerate()
            .map(|(number, passage)| format!("[{}] {}\n{}", number + 1, passage.citation, passage.text))
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(Context {
            token_count: count_tokens(&text),
            text,
            passages,
        })
    }
}
