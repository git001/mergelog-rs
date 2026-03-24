use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::io::{BufRead, Write};

use anyhow::Result;
use memchr::memchr;

use crate::caddy;
use crate::parser::{parse_clf_timestamp, sentinel};
use crate::stats::{FileStats, MergeStats};

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

/// Min-heap by timestamp; tie-break by file index to preserve input order.
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .ts
            .cmp(&self.ts)
            .then_with(|| self.file_idx.cmp(&other.file_idx))
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Format override passed from the CLI.
///
/// - `None`        — auto-detect per file (first char `{` → Caddy, else CLF)
/// - `Some(true)`  — force all files to Caddy JSON
/// - `Some(false)` — force all files to CLF (disables Caddy auto-detection)
pub type ForceFormat = Option<bool>;

/// Merge all `readers` into `out` in chronological order.
///
/// Caddy JSON lines are converted to CLF on the fly. Lines whose timestamp
/// cannot be parsed sort to the end (sentinel = i64::MAX).
/// Returns `MergeStats` with per-file line counts and time ranges.
pub fn merge(
    readers: Vec<(usize, Box<dyn BufRead + Send>)>,
    out: &mut impl Write,
    progress: bool,
    force_format: ForceFormat,
) -> Result<MergeStats> {
    let n_files = readers.len();
    let mut stats: Vec<FileStats> = (0..n_files).map(|_| FileStats::new()).collect();
    let mut heap: BinaryHeap<Entry> = BinaryHeap::with_capacity(n_files);
    let mut file_is_caddy: Vec<bool> = vec![false; n_files];

    // Seed the heap with the first parseable line of each file.
    for (file_idx, mut reader) in readers {
        let is_caddy = detect_format(&mut *reader, force_format)?;
        file_is_caddy[file_idx] = is_caddy;
        if let Some((ts, line)) = next_entry(&mut *reader, is_caddy)? {
            stats[file_idx].record(ts);
            heap.push(Entry {
                ts,
                file_idx,
                line,
                reader,
            });
        }
    }

    let mut count: u64 = 0;

    while let Some(Entry {
        line,
        mut reader,
        file_idx,
        ts: _,
    }) = heap.pop()
    {
        out.write_all(line.as_bytes())?;

        if let Some((ts, line)) = next_entry(&mut *reader, file_is_caddy[file_idx])? {
            stats[file_idx].record(ts);
            heap.push(Entry {
                ts,
                file_idx,
                line,
                reader,
            });
        }

        if progress {
            count += 1;
            if count.is_multiple_of(10_000) {
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
// Format detection
// ---------------------------------------------------------------------------

/// Peek at the first non-whitespace byte to decide whether a file is Caddy
/// JSON (`{`) or CLF. Does not consume any bytes.
fn detect_format(reader: &mut dyn BufRead, force: ForceFormat) -> Result<bool> {
    match force {
        Some(forced) => Ok(forced),
        None => {
            let buf = reader.fill_buf()?;
            Ok(buf
                .iter()
                .find(|&&b| !b.is_ascii_whitespace())
                .map(|&b| b == b'{')
                .unwrap_or(false))
        }
    }
}

// ---------------------------------------------------------------------------
// Line reading
// ---------------------------------------------------------------------------

/// Read the next output-worthy line from `reader`.
///
/// CLF: every line is returned; malformed lines get the sentinel timestamp
/// so they sort to the end.
///
/// Caddy JSON: non-access-log entries (no `request` field, e.g. error/debug
/// messages) are skipped silently. Access entries are returned as CLF strings.
///
/// Returns `(unix_ts, output_line)` or `None` on EOF.
fn next_entry(reader: &mut dyn BufRead, is_caddy: bool) -> Result<Option<(i64, String)>> {
    let mut buf = String::with_capacity(512);
    loop {
        buf.clear();
        if !fill_line(reader, &mut buf)? {
            return Ok(None);
        }
        if is_caddy {
            if let Some((ts, clf)) = caddy::parse_caddy_line(&buf) {
                return Ok(Some((ts, clf)));
            }
            // Non-request line (error/info/debug) — skip.
        } else {
            let ts = parse_clf_timestamp(&buf).unwrap_or_else(sentinel);
            return Ok(Some((ts, buf)));
        }
    }
}

/// Read one non-empty line into `buf` (caller must clear it first).
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
                let s = std::str::from_utf8(&available[..end])
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                buf.push_str(s);
                reader.consume(end);
                if buf.trim_end_matches(['\n', '\r']).is_empty() {
                    buf.clear();
                    continue; // blank line — try next
                }
                return Ok(true);
            }
            None => {
                // No newline in buffer yet — consume all and loop for more.
                let len = available.len();
                let s = std::str::from_utf8(available)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                buf.push_str(s);
                reader.consume(len);
            }
        }
    }
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
