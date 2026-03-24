use crate::parser::ts_to_string;

// ---------------------------------------------------------------------------
// Per-file statistics
// ---------------------------------------------------------------------------

pub struct FileStats {
    pub lines: u64,
    pub malformed: u64,
    pub first_ts: i64, // i64::MAX if no valid timestamp seen
    pub last_ts: i64,  // i64::MIN if no valid timestamp seen
}

impl FileStats {
    pub(crate) fn new() -> Self {
        Self {
            lines: 0,
            malformed: 0,
            first_ts: i64::MAX,
            last_ts: i64::MIN,
        }
    }

    pub(crate) fn record(&mut self, ts: i64) {
        self.lines += 1;
        if ts == i64::MAX {
            self.malformed += 1;
        } else {
            if ts < self.first_ts {
                self.first_ts = ts;
            }
            if ts > self.last_ts {
                self.last_ts = ts;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Aggregate statistics
// ---------------------------------------------------------------------------

pub struct MergeStats {
    pub per_file: Vec<FileStats>,
}

impl MergeStats {
    pub fn total_lines(&self) -> u64 {
        self.per_file.iter().map(|f| f.lines).sum()
    }
    pub fn total_malformed(&self) -> u64 {
        self.per_file.iter().map(|f| f.malformed).sum()
    }
    pub fn first_ts(&self) -> i64 {
        self.per_file
            .iter()
            .map(|f| f.first_ts)
            .min()
            .unwrap_or(i64::MAX)
    }
    pub fn last_ts(&self) -> i64 {
        self.per_file
            .iter()
            .map(|f| f.last_ts)
            .max()
            .unwrap_or(i64::MIN)
    }

    /// Print a formatted summary to stderr.
    pub fn print(&self, file_names: &[std::path::PathBuf]) {
        let total = self.total_lines();
        let bad = self.total_malformed();
        let width = file_names
            .iter()
            .map(|p| p.display().to_string().len())
            .max()
            .unwrap_or(0);

        eprintln!("──── merge statistics ──────────────────────────────────────");
        eprintln!("  Files    : {}", self.per_file.len());
        eprintln!(
            "  Lines    : {}  ({} malformed)",
            fmt_u64(total),
            fmt_u64(bad)
        );
        eprintln!(
            "  Range    : {} → {}",
            ts_to_string(self.first_ts()),
            ts_to_string(self.last_ts())
        );
        eprintln!("────────────────────────────────────────────────────────────");
        for (f, name) in self.per_file.iter().zip(file_names) {
            eprintln!(
                "  {:<width$}  {:>12} lines  {} → {}{}",
                name.display(),
                fmt_u64(f.lines),
                ts_to_string(f.first_ts),
                ts_to_string(f.last_ts),
                if f.malformed > 0 {
                    format!("  ({} malformed)", fmt_u64(f.malformed))
                } else {
                    String::new()
                },
            );
        }
        eprintln!("────────────────────────────────────────────────────────────");
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Format a u64 with thousands separators, e.g. `1234567` → `"1,234,567"`.
pub(crate) fn fmt_u64(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}
