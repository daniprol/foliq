//! End-to-end tests of the engine on a temporary folder.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use foliq::{Change, Index, Parsed, Parser, Result, SearchOptions, Sink, SourceEntry};

const BILLING: &str = "# Billing\n\nHow we bill customers.\n\n## Refunds\n\nRefunds are paid within 5 business days.\n\n## Invoices\n\nInvoices are sent monthly.\n";
const HANDBOOK: &str = "# Handbook\n\n## Holidays\n\nEveryone gets 25 days of holidays.\n";

fn write(root: &Path, path: &str, text: &str) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
}

/// Makes a docs folder and an index next to it, synced.
fn setup() -> (tempfile::TempDir, Index) {
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "docs/billing.md", BILLING);
    write(folder.path(), "docs/team/handbook.md", HANDBOOK);
    let index = Index::open(folder.path().join(".foliq")).unwrap();
    index.add_folder(&folder.path().join("docs"), None, Vec::new()).unwrap();
    let report = index.sync().unwrap();
    assert_eq!((report.added, report.failed.len()), (2, 0));
    (folder, index)
}

#[test]
fn search_finds_and_cites_chunks() {
    let (_folder, index) = setup();
    let results = index
        .search("how long do refunds take", &SearchOptions::default())
        .unwrap();
    let best = &results[0];
    assert_eq!(best.path, "billing.md");
    assert_eq!(best.heading_path, ["Billing", "Refunds"]);
    assert_eq!(best.citation, "docs/billing.md#1.1 (L5-7)");
}

#[test]
fn outline_and_read_by_reference() {
    let (_folder, index) = setup();
    let outline = index.outline("docs/billing.md", None).unwrap();
    let numbers: Vec<&str> = outline.sections.iter().map(|section| section.number.as_str()).collect();
    assert_eq!(numbers, ["1", "1.1", "1.2"]);

    let section = index.read("docs/billing.md#1.2", None, None).unwrap();
    assert_eq!(section.text, "## Invoices\n\nInvoices are sent monthly.");
    // A bare file name works when it is unique, and a pasted citation works too.
    assert_eq!(index.read("handbook.md", None, None).unwrap().path, "team/handbook.md");
    assert_eq!(
        index.read("docs/billing.md#1.2 (L9-11)", None, None).unwrap().text,
        section.text
    );
    // Budgets cut whole lines.
    let short = index.read("docs/billing.md", Some(8), None).unwrap();
    assert!(short.truncated && short.token_count <= 8);
}

#[test]
fn context_packs_sections_within_the_budget() {
    let (_folder, index) = setup();
    let context = index.context("refunds", 200, &SearchOptions::default()).unwrap();
    assert!(context.text.starts_with("[1] docs/billing.md#1.1 (L5-7)\n## Refunds"));
    assert!(context.token_count <= 200);
}

#[test]
fn sync_is_incremental() {
    let (folder, index) = setup();
    let docs = folder.path().join("docs");

    let again = index.sync().unwrap();
    assert_eq!(
        (again.added, again.updated, again.unchanged, again.removed),
        (0, 0, 2, 0)
    );

    // Same bytes, new modified time: nothing is re-parsed.
    let file = fs::File::options().write(true).open(docs.join("billing.md")).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    let touched = index.sync().unwrap();
    assert_eq!((touched.updated, touched.unchanged), (0, 2));

    write(
        &docs,
        "billing.md",
        &BILLING.replace("5 business days", "10 business days"),
    );
    fs::remove_file(docs.join("team/handbook.md")).unwrap();
    let changed = index.sync().unwrap();
    assert_eq!((changed.added, changed.updated, changed.removed), (0, 1, 1));
    assert!(index.search("holidays", &SearchOptions::default()).unwrap().is_empty());
}

#[test]
fn a_missing_folder_deletes_nothing() {
    let (folder, index) = setup();
    fs::rename(folder.path().join("docs"), folder.path().join("moved")).unwrap();
    let report = index.sync().unwrap();
    assert_eq!(report.removed, 0);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(index.status().unwrap()[0].documents, 2);
}

#[test]
fn access_rules_filter_before_ranking() {
    let folder = tempfile::tempdir().unwrap();
    write(
        folder.path(),
        "hr/salaries.md",
        "# Salaries\n\nThe salary bands are secret.\n",
    );
    write(folder.path(), "wiki/pay.md", "# Pay\n\nSalary is paid monthly.\n");
    let index = Index::open(folder.path().join(".foliq")).unwrap();
    index
        .add_folder(&folder.path().join("hr"), None, vec!["group:hr".into()])
        .unwrap();
    index.add_folder(&folder.path().join("wiki"), None, Vec::new()).unwrap();
    index.sync().unwrap();

    let as_ana = SearchOptions {
        principals: Some(vec!["user:ana".into()]),
        ..SearchOptions::default()
    };
    let paths: Vec<String> = index
        .search("salary", &as_ana)
        .unwrap()
        .into_iter()
        .map(|hit| hit.path)
        .collect();
    assert_eq!(paths, ["pay.md"]);
    assert!(
        index
            .read("hr/salaries.md", None, Some(&["user:ana".to_string()]))
            .is_err()
    );

    let as_hr = SearchOptions {
        principals: Some(vec!["group:hr".into()]),
        ..SearchOptions::default()
    };
    assert_eq!(index.search("salary", &as_hr).unwrap().len(), 2);
}

/// A plugin parser for `.note` files: upper-cases a title line.
struct NoteParser;

impl Parser for NoteParser {
    fn name(&self) -> &str {
        "note"
    }
    fn extensions(&self) -> Vec<String> {
        vec!["note".into()]
    }
    fn parse(&self, bytes: &[u8], _entry: &SourceEntry) -> Result<Parsed> {
        let text = String::from_utf8_lossy(bytes);
        Ok(Parsed::markdown(format!("# NOTE\n\n{text}")))
    }
}

/// A sink that remembers what it was told.
#[derive(Default)]
struct RecordingSink(Mutex<Vec<String>>);

impl Sink for RecordingSink {
    fn name(&self) -> &str {
        "recording"
    }
    fn apply(&self, changes: &[Change]) -> Result<()> {
        let mut seen = self.0.lock().unwrap();
        for change in changes {
            seen.push(match change {
                Change::Upsert { document, .. } => format!("upsert {}", document.path),
                Change::Delete { path, .. } => format!("delete {path}"),
            });
        }
        Ok(())
    }
}

#[test]
fn plugins_parse_and_receive_changes() {
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "notes/a.note", "buy milk");
    let sink = Arc::new(RecordingSink::default());
    let index = Index::builder(folder.path().join(".foliq"))
        .parser(Arc::new(NoteParser))
        .sink(sink.clone())
        .open()
        .unwrap();
    index
        .add_folder(&folder.path().join("notes"), None, Vec::new())
        .unwrap();
    index.sync().unwrap();
    assert_eq!(index.read("a.note", None, None).unwrap().title, "NOTE");
    // Non-Markdown documents are mirrored as Markdown for grep-style tools.
    let mirrored = fs::read_to_string(index.mirror_folder().join("notes/a.note.md")).unwrap();
    assert!(mirrored.contains("buy milk"));

    fs::remove_file(folder.path().join("notes/a.note")).unwrap();
    index.sync().unwrap();
    assert_eq!(*sink.0.lock().unwrap(), ["upsert a.note", "delete a.note"]);
    assert!(!index.mirror_folder().join("notes/a.note.md").exists());
}
