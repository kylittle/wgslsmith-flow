use clap::Parser;
use insert_ub::cli::{self, Options};

fn main() -> eyre::Result<()> {
    cli::run(Options::parse())
}
