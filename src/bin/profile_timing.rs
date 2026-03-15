/// Quick wall-clock profiler: measures I/O, parsing, and heap separately.
use std::collections::BinaryHeap;
use std::cmp::Ordering;
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant};
use std::fs::File;

// ── inline parser (same logic as src/parser.rs) ──────────────────────────────

#[inline] fn digit(b: u8) -> Option<u8> { if b.wrapping_sub(b'0') <= 9 { Some(b - b'0') } else { None } }
#[inline] fn parse2(a: u8, b: u8) -> Option<u8> { Some(digit(a)? * 10 + digit(b)?) }
#[inline] fn parse4(a: u8, b: u8, c: u8, d: u8) -> Option<u16> {
    Some(digit(a)? as u16 * 1000 + digit(b)? as u16 * 100 + digit(c)? as u16 * 10 + digit(d)? as u16)
}
fn parse_month(s: &[u8]) -> Option<u8> {
    match s { b"Jan"=>Some(1),b"Feb"=>Some(2),b"Mar"=>Some(3),b"Apr"=>Some(4),
              b"May"=>Some(5),b"Jun"=>Some(6),b"Jul"=>Some(7),b"Aug"=>Some(8),
              b"Sep"=>Some(9),b"Oct"=>Some(10),b"Nov"=>Some(11),b"Dec"=>Some(12),_=>None }
}
fn days_since_epoch(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
fn parse_ts(line: &str) -> Option<i64> {
    let s = line.as_bytes();
    let bracket = s.iter().position(|&b| b == b'[')?;
    let s = &s[bracket + 1..];
    if s.len() < 26 { return None; }
    let day   = parse2(s[0],  s[1])?  as i64;
    let month = parse_month(&s[3..6])? as i64;
    let year  = parse4(s[7], s[8], s[9], s[10])? as i64;
    let hour  = parse2(s[12], s[13])? as i64;
    let min   = parse2(s[15], s[16])? as i64;
    let sec   = parse2(s[18], s[19])? as i64;
    let sign: i64 = if s[21] == b'+' { 1 } else { -1 };
    let tz_h  = parse2(s[22], s[23])? as i64;
    let tz_m  = parse2(s[24], s[25])? as i64;
    let tz_offset = sign * (tz_h * 3600 + tz_m * 60);
    Some(days_since_epoch(year, month, day) * 86_400 + hour * 3600 + min * 60 + sec - tz_offset)
}

// ── heap entry ────────────────────────────────────────────────────────────────

struct Entry {
    ts: i64,
    file_idx: usize,
    line: String,
    reader: BufReader<File>,
}
impl PartialEq for Entry { fn eq(&self, o: &Self) -> bool { self.ts == o.ts && self.file_idx == o.file_idx } }
impl Eq for Entry {}
impl PartialOrd for Entry { fn partial_cmp(&self, o: &Self) -> Option<Ordering> { Some(self.cmp(o)) } }
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other.ts.cmp(&self.ts).then_with(|| self.file_idx.cmp(&other.file_idx))
    }
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() { eprintln!("usage: profile_timing <file>..."); return; }

    let mut t_io    = Duration::ZERO;
    let mut t_parse = Duration::ZERO;
    let mut t_heap  = Duration::ZERO;
    let mut t_write = Duration::ZERO;
    let mut lines: u64 = 0;

    let mut heap: BinaryHeap<Entry> = BinaryHeap::with_capacity(args.len());
    let stdout = std::io::stdout();
    let mut sink = std::io::BufWriter::with_capacity(1 << 20, stdout.lock());

    // ── seed ──────────────────────────────────────────────────────────────────
    for (idx, path) in args.iter().enumerate() {
        let file = File::open(path).unwrap_or_else(|e| { eprintln!("{path}: {e}"); std::process::exit(1); });
        let mut reader = BufReader::with_capacity(256 * 1024, file);

        let t0 = Instant::now();
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap();
        t_io += t0.elapsed();
        if n == 0 { continue; }

        let t0 = Instant::now();
        let ts = parse_ts(&line).unwrap_or(i64::MAX);
        t_parse += t0.elapsed();

        let t0 = Instant::now();
        heap.push(Entry { ts, file_idx: idx, line, reader });
        t_heap += t0.elapsed();
    }

    // ── main loop ─────────────────────────────────────────────────────────────
    loop {
        let t0 = Instant::now();
        let entry = heap.pop();
        t_heap += t0.elapsed();
        let Some(Entry { line, mut reader, file_idx, .. }) = entry else { break };

        let t0 = Instant::now();
        sink.write_all(line.as_bytes()).unwrap();
        t_write += t0.elapsed();

        let t0 = Instant::now();
        let mut next = String::new();
        let n = reader.read_line(&mut next).unwrap();
        t_io += t0.elapsed();

        if n > 0 && !next.trim().is_empty() {
            let t0 = Instant::now();
            let ts = parse_ts(&next).unwrap_or(i64::MAX);
            t_parse += t0.elapsed();

            let t0 = Instant::now();
            heap.push(Entry { ts, file_idx, line: next, reader });
            t_heap += t0.elapsed();
        }
        lines += 1;
    }
    sink.flush().unwrap();

    let total = t_io + t_parse + t_heap + t_write;
    eprintln!("\n──── profile_timing ({lines} lines) ────────────────");
    eprintln!("  I/O   (read_line)    {:>8.3}s  ({:4.1}%)", t_io.as_secs_f64(),    t_io.as_secs_f64()    / total.as_secs_f64() * 100.0);
    eprintln!("  Parse (hand-rolled)  {:>8.3}s  ({:4.1}%)", t_parse.as_secs_f64(), t_parse.as_secs_f64() / total.as_secs_f64() * 100.0);
    eprintln!("  Heap  (push/pop)     {:>8.3}s  ({:4.1}%)", t_heap.as_secs_f64(),  t_heap.as_secs_f64()  / total.as_secs_f64() * 100.0);
    eprintln!("  Write (write_all)    {:>8.3}s  ({:4.1}%)", t_write.as_secs_f64(), t_write.as_secs_f64() / total.as_secs_f64() * 100.0);
    eprintln!("  ─────────────────────────────────────────────────");
    eprintln!("  Σ measured           {:>8.3}s", total.as_secs_f64());
}
