//! The `foliq` binary. All the work happens in the library (`src/lib.rs`), so the
//! Python package can run the same CLI.

fn main() {
    std::process::exit(foliq_cli::run(std::env::args().collect()));
}
