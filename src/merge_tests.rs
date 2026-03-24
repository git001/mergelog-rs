use super::*;
use std::io::Cursor;

// ── helpers ───────────────────────────────────────────────────────────────────

fn clf(ip: &str, ts: &str, uri: &str) -> String {
    format!("{ip} - - [{ts} +0000] \"GET {uri} HTTP/1.1\" 200 100 \"-\" \"-\"\n")
}

fn caddy_json(ts: &str, uri: &str) -> String {
    format!(
        "{{\"ts\":\"{ts}\",\"user_id\":\"\",\"status\":200,\"size\":512,\
         \"request\":{{\"client_ip\":\"1.2.3.4\",\"method\":\"GET\",\
         \"uri\":\"{uri}\",\"proto\":\"HTTP/1.1\",\"headers\":{{}}}}}}\n"
    )
}

fn readers(data: &[&str]) -> Vec<(usize, Box<dyn BufRead + Send>)> {
    data.iter()
        .enumerate()
        .map(|(i, s)| -> (usize, Box<dyn BufRead + Send>) {
            (i, Box::new(Cursor::new(s.to_string())))
        })
        .collect()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn merges_two_clf_files_in_order() {
    let f1 = clf("1.1.1.1", "01/Jan/2026:10:00:00", "/a")
        + &clf("1.1.1.1", "01/Jan/2026:10:00:02", "/b");
    let f2 = clf("2.2.2.2", "01/Jan/2026:10:00:01", "/c")
        + &clf("2.2.2.2", "01/Jan/2026:10:00:03", "/d");

    let mut out = Vec::new();
    merge(readers(&[&f1, &f2]), &mut out, false, None).unwrap();
    let s = String::from_utf8(out).unwrap();
    let uris: Vec<&str> = s
        .lines()
        .map(|l| l.split('"').nth(1).unwrap().split(' ').nth(1).unwrap())
        .collect();
    assert_eq!(uris, ["/a", "/c", "/b", "/d"]);
}

#[test]
fn merges_caddy_to_clf() {
    let data = caddy_json("2026-01-01T10:00:00.000Z", "/caddy")
        + &caddy_json("2026-01-01T10:00:01.000Z", "/caddy2");

    let mut out = Vec::new();
    merge(readers(&[&data]), &mut out, false, None).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("[01/Jan/2026:10:00:00 +0000]"));
    assert!(s.contains("\"GET /caddy HTTP/1.1\""));
    assert!(s.contains("[01/Jan/2026:10:00:01 +0000]"));
}

#[test]
fn merges_mixed_clf_and_caddy() {
    let clf_data = clf("9.9.9.9", "01/Jan/2026:10:00:01", "/clf");
    let caddy_data = caddy_json("2026-01-01T10:00:00.000Z", "/caddy");

    let mut out = Vec::new();
    merge(readers(&[&clf_data, &caddy_data]), &mut out, false, None).unwrap();
    let s = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines.len(), 2);
    // Caddy line (10:00:00) should come before CLF line (10:00:01)
    assert!(lines[0].contains("/caddy"));
    assert!(lines[1].contains("/clf"));
}

#[test]
fn handles_empty_file() {
    let mut out = Vec::new();
    merge(readers(&[""]), &mut out, false, None).unwrap();
    assert!(out.is_empty());
}

#[test]
fn force_format_caddy() {
    let data = caddy_json("2026-06-01T12:00:00.000Z", "/forced");
    let mut out = Vec::new();
    merge(readers(&[&data]), &mut out, false, Some(true)).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("[01/Jun/2026:12:00:00 +0000]"));
}

#[test]
fn caddy_non_request_lines_skipped() {
    let error_line =
        "{\"level\":\"error\",\"ts\":\"2026-01-01T10:00:00Z\",\"msg\":\"upstream failed\"}\n";
    let access = caddy_json("2026-01-01T10:00:01.000Z", "/ok");
    let data = format!("{error_line}{access}");

    let mut out = Vec::new();
    merge(readers(&[&data]), &mut out, false, None).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s.lines().count(), 1);
    assert!(s.contains("/ok"));
}

#[test]
fn malformed_clf_counted_and_sorted_last() {
    // Malformed line in file0 gets sentinel ts; good line in file1 has real ts.
    // k-way merge sorts file1's entry first because sentinel > any real ts.
    let bad_file = "not a log line\n".to_string();
    let good_file = clf("2.2.2.2", "01/Jan/2026:10:00:00", "/good");
    let mut out = Vec::new();
    let stats = merge(readers(&[&bad_file, &good_file]), &mut out, false, None).unwrap();
    let s = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(stats.total_lines(), 2);
    assert_eq!(stats.total_malformed(), 1);
    assert!(lines[0].contains("/good"), "got: {}", lines[0]);
    assert!(lines[1].contains("not a log line"));
}

#[test]
fn progress_does_not_crash() {
    let mut data = String::new();
    for i in 0..10_001 {
        data.push_str(&clf("1.1.1.1", "01/Jan/2026:10:00:00", &format!("/{i}")));
    }
    let mut out = Vec::new();
    merge(readers(&[&data]), &mut out, true, None).unwrap();
    assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), 10_001);
}

#[test]
fn stats_first_and_last_ts() {
    let f1 = clf("1.1.1.1", "01/Jan/2026:10:00:00", "/a");
    let f2 = clf("2.2.2.2", "01/Jan/2026:10:00:05", "/b");
    let mut out = Vec::new();
    let stats = merge(readers(&[&f1, &f2]), &mut out, false, None).unwrap();
    assert!(stats.first_ts() < stats.last_ts());
    // smoke-test print (writes to stderr — just verify it doesn't panic)
    stats.print(&[
        std::path::PathBuf::from("file1.log"),
        std::path::PathBuf::from("file2.log"),
    ]);
}

#[test]
fn returns_stats() {
    let data = clf("1.1.1.1", "01/Jan/2026:10:00:00", "/x");
    let mut out = Vec::new();
    let stats = merge(readers(&[&data]), &mut out, false, None).unwrap();
    assert_eq!(stats.total_lines(), 1);
    assert_eq!(stats.total_malformed(), 0);
}
