/// Hand-rolled CLF timestamp parser.
///
/// Input format inside `[…]`: `DD/Mon/YYYY:HH:MM:SS ±HHMM`
/// Returns a Unix timestamp (seconds since 1970-01-01 UTC) as `i64`,
/// or `None` for malformed lines.
///
/// ~10× faster than jiff::Zoned::strptime because it does only integer
/// arithmetic with fixed byte-offsets — no heap allocation, no locale,
/// no timezone database lookup.
pub fn parse_clf_timestamp(line: &str) -> Option<i64> {
    let s = line.as_bytes();

    // Find '['; everything before it is host/ident/user and varies in length.
    let bracket = s.iter().position(|&b| b == b'[')?;
    let s = &s[bracket + 1..];

    // Fixed layout after '[':
    //   DD/Mon/YYYY:HH:MM:SS ±HHMM
    //   0123456789012345678901234 5
    if s.len() < 26 {
        return None;
    }

    let day   = parse2(s[0],  s[1])?  as i64;
    // s[2]  == b'/'
    let month = parse_month(&s[3..6])? as i64;
    // s[6]  == b'/'
    let year  = parse4(s[7], s[8], s[9], s[10])? as i64;
    // s[11] == b':'
    let hour  = parse2(s[12], s[13])? as i64;
    // s[14] == b':'
    let min   = parse2(s[15], s[16])? as i64;
    // s[17] == b':'
    let sec   = parse2(s[18], s[19])? as i64;
    // s[20] == b' '
    let sign: i64 = if s[21] == b'+' { 1 } else { -1 };
    let tz_h  = parse2(s[22], s[23])? as i64;
    let tz_m  = parse2(s[24], s[25])? as i64;

    let tz_offset = sign * (tz_h * 3600 + tz_m * 60);
    let unix = days_since_epoch(year, month, day) * 86_400
        + hour * 3600
        + min  * 60
        + sec
        - tz_offset;

    Some(unix)
}

/// Sentinel value for lines whose timestamp cannot be parsed (sort to end).
#[inline]
pub fn sentinel() -> i64 {
    i64::MAX
}

/// Format a UTC Unix timestamp as `YYYY-MM-DD HH:MM:SS UTC`.
/// Returns `"N/A"` for the sentinel value.
pub fn ts_to_string(ts: i64) -> String {
    if ts == i64::MAX || ts == i64::MIN {
        return "N/A".to_string();
    }
    // Split into days + time-of-day, handling negative timestamps correctly.
    let (secs_of_day, days) = {
        let r = ts % 86_400;
        if r < 0 { (r + 86_400, ts / 86_400 - 1) } else { (r, ts / 86_400) }
    };
    let hour = secs_of_day / 3600;
    let min  = (secs_of_day % 3600) / 60;
    let sec  = secs_of_day % 60;
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{min:02}:{sec:02} UTC")
}

/// Inverse of `days_since_epoch` — Howard Hinnant's algorithm.
fn days_to_ymd(z: i64) -> (i64, i64, i64) {
    let z   = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y   = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let m   = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ── helpers ──────────────────────────────────────────────────────────────────

#[inline]
fn digit(b: u8) -> Option<u8> {
    if b.wrapping_sub(b'0') <= 9 { Some(b - b'0') } else { None }
}

#[inline]
fn parse2(a: u8, b: u8) -> Option<u8> {
    Some(digit(a)? * 10 + digit(b)?)
}

#[inline]
fn parse4(a: u8, b: u8, c: u8, d: u8) -> Option<u16> {
    Some(digit(a)? as u16 * 1000
       + digit(b)? as u16 * 100
       + digit(c)? as u16 * 10
       + digit(d)? as u16)
}

/// Map 3-byte month abbreviation to 1-based month number.
#[inline]
fn parse_month(s: &[u8]) -> Option<u8> {
    match s {
        b"Jan" => Some(1),  b"Feb" => Some(2),  b"Mar" => Some(3),
        b"Apr" => Some(4),  b"May" => Some(5),  b"Jun" => Some(6),
        b"Jul" => Some(7),  b"Aug" => Some(8),  b"Sep" => Some(9),
        b"Oct" => Some(10), b"Nov" => Some(11), b"Dec" => Some(12),
        _ => None,
    }
}

/// Days since Unix epoch (1970-01-01) for a proleptic Gregorian date.
/// Uses Howard Hinnant's algorithm (no branches, pure arithmetic).
#[inline]
fn days_since_epoch(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;                              // [0, 399]
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;    // [0, 146096]
    era * 146_097 + doe - 719_468
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_combined_log_line() {
        let line = r#"213.133.113.84 - - [06/Mar/2026:00:01:56 +0100] "GET / HTTP/1.1" 200 78 "-" "Bot""#;
        let ts = parse_clf_timestamp(line).expect("should parse");
        // 2026-03-06T00:01:56+01:00 == 2026-03-05T23:01:56Z
        // days_since_epoch(2026,3,5) * 86400 + 23*3600 + 1*60 + 56
        let expected: i64 = days_since_epoch(2026, 3, 5) * 86_400 + 23 * 3600 + 60 + 56;
        assert_eq!(ts, expected);
    }

    #[test]
    fn parses_negative_offset() {
        // 2023-10-01T12:00:00-0500 == 2023-10-01T17:00:00Z
        let line = r#"1.2.3.4 - - [01/Oct/2023:12:00:00 -0500] "GET / HTTP/1.1" 200 100 "-" "-""#;
        let ts = parse_clf_timestamp(line).unwrap();
        let expected = days_since_epoch(2023, 10, 1) * 86_400 + 17 * 3600;
        assert_eq!(ts, expected);
    }

    #[test]
    fn returns_none_for_garbage() {
        assert!(parse_clf_timestamp("not a log line").is_none());
    }
}
