//! The `foliq` command line.
//!
//! `run` takes the arguments (like Python's `sys.argv`) and returns the exit
//! code. The `foliq` binary and the Python package (`foliq._cli`) both call it,
//! so they behave the same.
//!
//! | Command                   | Does                                                 |
//! |---------------------------|------------------------------------------------------|
//! | `foliq index [FOLDER]...` | adds folders as collections and syncs everything     |
//! | `foliq status`            | collections and their sizes                          |
//! | `foliq remove NAME`       | removes a collection from the index                  |
//! | `foliq outline [REF]`     | lists documents, or the sections of one document     |
//! | `foliq read REF`          | prints a document, a section (`#2.3`) or pages (`#p4`) |
//! | `foliq search QUERY`      | ranked chunks with citations                         |
//! | `foliq context QUERY`     | the best passages, packed into a token budget        |
//! | `foliq skill [--install]` | prints the agent skill file, or installs it          |
//!
//! Commands that read the index sync it first (it is cheap when nothing
//! changed), so answers are never stale. `--no-refresh` skips that.

mod output;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use foliq::{Error, Filter, Index, SearchOptions};

/// The agent skill file, built into the binary.
// `skills/foliq/SKILL.md` at the repository root is a copy for `npx skills add`;
// `tests/test_cli.py` checks that both stay the same.
pub const SKILL: &str = include_str!("../SKILL.md");

/// The index folder's default name.
const DEFAULT_INDEX_FOLDER: &str = ".foliq";

#[derive(Parser)]
#[command(
    name = "foliq",
    version,
    about = "Index documents and let AI agents search, read and cite them."
)]
struct Cli {
    /// The index folder. Default: the nearest `.foliq` folder in this folder or a parent, else `./.foliq`.
    #[arg(long, global = true, env = "FOLIQ_INDEX")]
    index: Option<PathBuf>,

    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,

    /// Don't sync before answering (faster, may be stale).
    #[arg(long, global = true)]
    no_refresh: bool,

    /// Answer as this principal (repeatable): only documents they may see are used.
    #[arg(long = "principal", global = true, value_name = "PRINCIPAL")]
    principals: Vec<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add folders as collections (if given) and sync the index.
    Index {
        /// Folders to add. Each becomes a collection named after the folder.
        folders: Vec<PathBuf>,
        /// The collection name (only with one folder).
        #[arg(long)]
        name: Option<String>,
        /// Who may see the documents of the added folders (repeatable). Default: everyone.
        #[arg(long = "access", value_name = "PRINCIPAL")]
        access: Vec<String>,
    },
    /// Show the collections and their sizes.
    Status,
    /// Remove a collection from the index (the files are not touched).
    Remove { name: String },
    /// List the documents, or the sections of one document.
    Outline {
        /// A document (`collection/path`). Leave out to list all documents.
        reference: Option<String>,
    },
    /// Print a document, a section (`doc.md#2.3`) or pages (`doc.pdf#p4-6`).
    Read {
        reference: String,
        /// The most tokens to print.
        #[arg(long, short)]
        budget: Option<usize>,
    },
    /// Search for chunks (BM25 keyword ranking).
    Search {
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
        /// The most results.
        #[arg(long, short = 'n', default_value_t = 10)]
        limit: usize,
        /// Only these collections (repeatable).
        #[arg(long = "collection", short)]
        collections: Vec<String>,
        /// Only paths that start with this.
        #[arg(long)]
        path: Option<String>,
    },
    /// The best passages for a question, cited and packed into a token budget.
    Context {
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
        /// The most tokens to return.
        #[arg(long, short, default_value_t = 2000)]
        budget: usize,
        /// The most passages.
        #[arg(long, short = 'n', default_value_t = 10)]
        limit: usize,
        /// Only these collections (repeatable).
        #[arg(long = "collection", short)]
        collections: Vec<String>,
        /// Only paths that start with this.
        #[arg(long)]
        path: Option<String>,
    },
    /// Print the agent skill file, or install it for Claude Code with `--install`.
    Skill {
        /// Write it to `.claude/skills/foliq/SKILL.md` in this folder.
        #[arg(long)]
        install: bool,
        /// With `--install`: write it to your home folder, for every project.
        #[arg(long, requires = "install")]
        global: bool,
    },
}

/// Runs the command line and returns the exit code (0 means success).
pub fn run(args: Vec<String>) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            // `--help` and `--version` are "errors" too, printed to stdout.
            let _ = error.print();
            return error.exit_code();
        }
    };
    match execute(cli) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("foliq: {error}");
            1
        }
    }
}

fn execute(cli: Cli) -> foliq::Result<()> {
    // `if let` is a one-case `match`: run this block only for the `skill` command.
    if let Command::Skill { install, global } = cli.command {
        if !install {
            print!("{SKILL}");
            return Ok(());
        }
        let path = install_skill(global)?;
        println!("installed {}", path.display());
        return Ok(());
    }
    let index = Index::open(cli.index.clone().unwrap_or_else(find_index_folder))?;
    let principals = (!cli.principals.is_empty()).then(|| cli.principals.clone());
    let refresh = |index: &Index| -> foliq::Result<()> {
        if !cli.no_refresh {
            index.sync()?;
        }
        Ok(())
    };
    let out = output::Printer { json: cli.json };

    match cli.command {
        Command::Skill { .. } => unreachable!("handled above"),
        Command::Index { folders, name, access } => {
            if name.is_some() && folders.len() != 1 {
                return Err(Error::Invalid("--name needs exactly one folder".into()));
            }
            for folder in &folders {
                index.add_folder(folder, name.as_deref(), access.clone())?;
            }
            if index.collections()?.is_empty() {
                return Err(Error::Invalid(
                    "nothing to index yet: run `foliq index <folder>`".into(),
                ));
            }
            let report = index.sync()?;
            out.sync_report(&report, &index.status()?);
        }
        Command::Status => out.status(&index.status()?, index.folder()),
        Command::Remove { name } => {
            if !index.remove(&name)? {
                return Err(Error::NotFound(format!("collection {name:?}")));
            }
            out.message(&format!("removed collection {name}"));
        }
        Command::Outline { reference } => {
            refresh(&index)?;
            match reference {
                Some(reference) => out.outline(&index.outline(&reference, principals.as_deref())?),
                None => {
                    let filter = Filter {
                        principals: principals.clone(),
                        ..Filter::default()
                    };
                    out.documents(&index.documents(&filter)?);
                }
            }
        }
        Command::Read { reference, budget } => {
            refresh(&index)?;
            out.passage(&index.read(&reference, budget, principals.as_deref())?);
        }
        Command::Search {
            query,
            limit,
            collections,
            path,
        } => {
            refresh(&index)?;
            let options = SearchOptions {
                limit,
                collections,
                path_prefix: path,
                principals,
            };
            out.search_results(&index.search(&query.join(" "), &options)?);
        }
        Command::Context {
            query,
            budget,
            limit,
            collections,
            path,
        } => {
            refresh(&index)?;
            let options = SearchOptions {
                limit,
                collections,
                path_prefix: path,
                principals,
            };
            out.context(&index.context(&query.join(" "), budget, &options)?);
        }
    }
    Ok(())
}

/// The nearest `.foliq` folder in the current folder or a parent, else `./.foliq`.
/// Writes the skill file where Claude Code looks for skills, and returns its path.
/// Other agents read `.agents/skills/`, which `npx skills add` covers.
fn install_skill(global: bool) -> foliq::Result<PathBuf> {
    let base = if global {
        // HOME on Linux and macOS, USERPROFILE on Windows.
        match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            Some(home) => PathBuf::from(home),
            None => return Err(Error::Invalid("can't find your home folder".to_string())),
        }
    } else {
        PathBuf::from(".")
    };
    let folder = base.join(".claude").join("skills").join("foliq");
    std::fs::create_dir_all(&folder)?;
    let path = folder.join("SKILL.md");
    std::fs::write(&path, SKILL)?;
    Ok(path)
}

fn find_index_folder() -> PathBuf {
    let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    current
        .ancestors()
        .map(|folder| folder.join(DEFAULT_INDEX_FOLDER))
        .find(|candidate| candidate.is_dir())
        .unwrap_or_else(|| Path::new(DEFAULT_INDEX_FOLDER).to_path_buf())
}
