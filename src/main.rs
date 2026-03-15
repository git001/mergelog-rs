mod merge;
mod parser;
mod reader;

use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

/// Merge multiple HTTP log files into one chronologically sorted stream.
///
/// Supports plain-text and gzip-compressed files (auto-detected by .gz
/// extension).  Reads from all files simultaneously using a min-heap so
/// memory usage stays proportional to the number of files, not their size.
#[derive(Parser)]
#[command(name = "mergelog", version, about, long_about = None)]
struct Cli {
    /// Log files to merge. Use `-` to read from stdin.
    /// Compression is auto-detected via magic bytes (gz, bz2, xz, zstd).
    #[arg(required = true, value_name = "FILE")]
    files: Vec<PathBuf>,

    /// Log format used to locate the timestamp in each line
    #[arg(long, value_enum, default_value_t = Format::Clf)]
    format: Format,

    /// Print a running line count to stderr during the merge
    #[arg(long)]
    progress: bool,

    /// Print per-file statistics (line counts, time ranges, malformed lines) to stderr after the merge
    #[arg(long)]
    stats: bool,

    /// Write output to FILE instead of stdout
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Clone, ValueEnum)]
enum Format {
    /// Apache / nginx Combined Log Format  [DD/Mon/YYYY:HH:MM:SS ±HHMM]
    Clf,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Open all input files, propagating errors early.
    let readers: Vec<(usize, _)> = cli
        .files
        .iter()
        .enumerate()
        .map(|(idx, path)| reader::open(path).map(|r| (idx, r)))
        .collect::<Result<_>>()?;

    // Note: Format::Nginx uses the same parser as Clf.
    // The flag is kept for future extensibility.
    let _ = cli.format;

    // Set up output: either a file or buffered stdout.
    let stats = if let Some(out_path) = cli.output {
        let file = std::fs::File::create(&out_path)
            .with_context(|| format!("cannot create {}", out_path.display()))?;
        let mut out = BufWriter::new(file);
        let s = merge::merge(readers, &mut out, cli.progress)?;
        out.flush()?;
        s
    } else {
        let stdout = io::stdout();
        let mut out = BufWriter::new(stdout.lock());
        let s = merge::merge(readers, &mut out, cli.progress)?;
        out.flush()?;
        s
    };

    if cli.stats {
        stats.print(&cli.files);
    }

    Ok(())
}
