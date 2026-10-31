//! Citations: one short string that says exactly where a passage comes from.
//!
//! `contracts/acme.pdf#2.3 (p4-5, L120-141)` means: collection `contracts`,
//! file `acme.pdf`, section 2.3, pages 4 to 5, lines 120 to 141 of the text.
//! The part before the space is also a reference that `read` accepts.

/// Formats a citation. `section` and the pages are left out when unknown.
pub fn citation(
    collection: &str,
    path: &str,
    section: Option<&str>,
    pages: (Option<u32>, Option<u32>),
    lines: (usize, usize),
) -> String {
    let mut text = format!("{collection}/{path}");
    if let Some(section) = section {
        text.push('#');
        text.push_str(section);
    }
    let mut details = Vec::new();
    match pages {
        (Some(first), Some(last)) if first != last => details.push(format!("p{first}-{last}")),
        (Some(page), _) | (None, Some(page)) => details.push(format!("p{page}")),
        (None, None) => {}
    }
    if lines.0 == lines.1 {
        details.push(format!("L{}", lines.0));
    } else {
        details.push(format!("L{}-{}", lines.0, lines.1));
    }
    format!("{text} ({})", details.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sections_pages_and_lines() {
        assert_eq!(
            citation("c", "a.pdf", Some("2.3"), (Some(4), Some(5)), (120, 141)),
            "c/a.pdf#2.3 (p4-5, L120-141)"
        );
        assert_eq!(citation("c", "a.md", None, (None, None), (3, 3)), "c/a.md (L3)");
    }
}
