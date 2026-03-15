use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::io::{BufRead, Write};

use anyhow::Result;
use memchr::memchr;

use crate::parser::{parse_clf_timestamp, sentinel, ts_to_string};

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Per-file merge statistics.
pub struct FileStats {
    pub lines:     u64,
    pub malformed: u64,
    pub first_ts:  i64,   // i64::MAX if no valid timestamp seen
    pub last_ts:   i64,   // i64::MIN if no valid timestamp seen
}

impl FileStats {
    fn new() -> Self {
        Self { lines: 0, malformed: 0, first_ts: i64::MAX, last_ts: i64::MIN }
    }
    fn record(&mut self, ts: i64) {
        self.lines += 1;
        if ts == i64::MAX {
            self.malformed += 1;
        } else {
            if ts < self.first_ts { self.first_ts = ts; }
            if ts > self.last_ts  { self.last_ts  = ts; }
        }
    }
}

/// Aggregate statistics returned by `merge()`.
pub struct MergeStats {
    pub per_file: Vec<FileStats>,
}

impl MergeStats {
    pub fn total_lines(&self)     -> u64 { self.per_file.iter().map(|f| f.lines).sum() }
    pub fn total_malformed(&self) -> u64 { self.per_file.iter().map(|f| f.malformed).sum() }
    pub fn first_ts(&self)        -> i64 { self.per_file.iter().map(|f| f.first_ts).min().unwrap_or(i64::MAX) }
    pub fn last_ts(&self)         -> i64 { self.per_file.iter().map(|f| f.last_ts).max().unwrap_or(i64::MIN) }

    /// Print a formatted summary to stderr.
    pub fn print(&self, file_names: &[std::path::PathBuf]) {
        let total   = self.total_lines();
        let bad     = self.total_malformed();
        let width   = file_names.iter().map(|p| p.display().to_string().len()).max().unwrap_or(0);

        eprintln!("──── merge statistics ──────────────────────────────────────");
        eprintln!("  Files    : {}", self.per_file.len());
        eprintln!("  Lines    : {}  ({} malformed)", fmt_u64(total), fmt_u64(bad));
        eprintln!("  Range    : {} → {}", ts_to_string(self.first_ts()), ts_to_string(self.last_ts()));
        eprintln!("────────────────────────────────────────────────────────────");
        for (f, name) in self.per_file.iter().zip(file_names) {
            eprintln!(
                "  {:<width$}  {:>12} lines  {} → {}{}",
                name.display(),
                fmt_u64(f.lines),
                ts_to_string(f.first_ts),
                ts_to_string(f.last_ts),
                if f.malformed > 0 { format!("  ({} malformed)", fmt_u64(f.malformed)) } else { String::new() },
            );
        }
        eprintln!("────────────────────────────────────────────────────────────");
    }
}

fn fmt_u64(n: u64) -> String {
    // Insert thousands separators for readability.
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { out.push(','); }
        out.push(c);
    }
    out.chars().rev().collect()
}

// ---------------------------------------------------------------------------
// Heap entry
// ---------------------------------------------------------------------------

struct Entry {
    ts: i64,
    /// Tie-breaker: preserve input order when timestamps are equal.
    file_idx: usize,
    line: String,
    reader: Box<dyn BufRead + Send>,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.ts == other.ts && self.file_idx == other.file_idx
    }
}
impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .ts
            .cmp(&self.ts)
            .then_with(|| self.file_idx.cmp(&other.file_idx))
    }
}

// ---------------------------------------------------------------------------
// Public merge function
// ---------------------------------------------------------------------------

/// Merge all `readers` into `out` in chronological order.
///
/// Lines whose timestamp cannot be parsed are sorted to the end
/// (sentinel = i64::MAX), matching the behaviour of mergelog 4.5.
/// Returns `MergeStats` with per-file line counts and time ranges.
pub fn merge(
    readers: Vec<(usize, Box<dyn BufRead + Send>)>,
    out: &mut impl Write,
    progress: bool,
) -> Result<MergeStats> {
    let n_files = readers.len();
    let mut stats: Vec<FileStats> = (0..n_files).map(|_| FileStats::new()).collect();
    let mut heap: BinaryHeap<Entry> = BinaryHeap::with_capacity(n_files);

    // Seed the heap with the first non-empty line of each file.
    for (file_idx, mut reader) in readers {
        let mut line = String::with_capacity(512);
        if fill_line(&mut *reader, &mut line)? {
            let ts = parse_clf_timestamp(&line).unwrap_or_else(sentinel);
            heap.push(Entry { ts, file_idx, line, reader });
        }
    }

    let mut count: u64 = 0;

    while let Some(Entry { mut line, mut reader, file_idx, ts }) = heap.pop() {
        out.write_all(line.as_bytes())?;
        stats[file_idx].record(ts);

        line.clear();
        if fill_line(&mut *reader, &mut line)? {
            let ts = parse_clf_timestamp(&line).unwrap_or_else(sentinel);
            heap.push(Entry { ts, file_idx, line, reader });
        }

        if progress {
            count += 1;
            if count % 10_000 == 0 {
                eprint!("\r{count} lines merged…");
            }
        }
    }

    if progress && count > 0 {
        eprintln!("\r{count} lines merged.");
    }

    Ok(MergeStats { per_file: stats })
}

// ---------------------------------------------------------------------------
// Line reader
// ---------------------------------------------------------------------------

/// Read one non-empty line into `buf` (which must already be cleared).
/// Uses `fill_buf` + `memchr` for SIMD-accelerated newline search.
/// Returns `true` if a line was read, `false` on EOF.
#[inline]
fn fill_line(reader: &mut dyn BufRead, buf: &mut String) -> Result<bool> {
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(false);
        }
        match memchr(b'\n', available) {
            Some(pos) => {
                let end = pos + 1;
                let s = std::str::from_utf8(&available[..end]).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                })?;
                buf.push_str(s);
                reader.consume(end);
                if buf.trim_end_matches(['\n', '\r']).is_empty() {
                    buf.clear();
                    continue; // blank line — try next
                }
                return Ok(true);
            }
            None => {
                // No newline in buffer — consume all and loop for more.
                let len = available.len();
                let s = std::str::from_utf8(available).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                })?;
                buf.push_str(s);
                reader.consume(len);
            }
        }
    }
}
