/// Caddy JSON access-log parser with CLF output conversion.
///
/// Caddy writes one JSON object per line, e.g.:
///   {"level":"info","ts":"2026-03-22T14:50:51.525Z","logger":"http.log.access",
///    "msg":"handled request","request":{...},"status":404,"size":29558,...}
///
/// This module parses that line and produces a CLF-formatted string:
///   client_ip - user [DD/Mon/YYYY:HH:MM:SS +0000] "METHOD URI PROTO" status bytes "referer" "ua"
use serde::Deserialize;

use crate::parser::{days_since_epoch, days_to_ymd};

// ── Serde structs ─────────────────────────────────────────────────────────────

/// `ts` can be either a Unix float (`1742651451.525`) or an ISO 8601 string
/// (`"2026-03-22T14:50:51.525Z"`), depending on the Caddy log encoder config.
#[derive(Deserialize)]
#[serde(untagged)]
enum Ts {
    Float(f64),
    Str(String),
}

#[derive(Deserialize)]
struct CaddyLog {
    ts: Ts,
    #[serde(default)]
    user_id: String,
    status: u16,
    size: u64,
    request: Request,
    // level, logger, msg, bytes_read, duration, resp_headers — ignored by serde
}

#[derive(Deserialize)]
struct Request {
    client_ip: String,
    method: String,
    uri: String,
    proto: String,
    #[serde(default)]
    headers: Headers,
}

#[derive(Deserialize, Default)]
struct Headers {
    // HTTP/1.x canonical form; also accept lowercase for HTTP/2+
    #[serde(rename = "Referer", alias = "referer", default)]
    referer: Vec<String>,
    #[serde(rename = "User-Agent", alias = "user-agent", default)]
    user_agent: Vec<String>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Parse a Caddy JSON log line.
/// Returns `(unix_timestamp_secs, clf_line)` or `None` if the line cannot be
/// parsed.  The returned CLF line is terminated with `\n`.
pub fn parse_caddy_line(line: &str) -> Option<(i64, String)> {
    let log: CaddyLog = serde_json::from_str(line).ok()?;
    let ts = match &log.ts {
        Ts::Float(f) => *f as i64,
        Ts::Str(s) => parse_iso8601(s)?,
    };
    Some((ts, to_clf(&log, ts)))
}

// ── CLF serialisation ─────────────────────────────────────────────────────────

fn to_clf(log: &CaddyLog, ts: i64) -> String {
    let user = if log.user_id.is_empty() {
        "-"
    } else {
        &log.user_id
    };
    let referer = log
        .request
        .headers
        .referer
        .first()
        .map(|s| s.as_str())
        .unwrap_or("-");
    let ua = log
        .request
        .headers
        .user_agent
        .first()
        .map(|s| s.as_str())
        .unwrap_or("-");
    let size = if log.size == 0 {
        "-".to_string()
    } else {
        log.size.to_string()
    };

    format!(
        "{} - {} {} \"{} {} {}\" {} {} \"{}\" \"{}\"\n",
        log.request.client_ip,
        user,
        clf_timestamp(ts),
        log.request.method,
        log.request.uri,
        log.request.proto,
        log.status,
        size,
        referer,
        ua,
    )
}

/// Format a UTC Unix timestamp as the CLF `%t` field: `[DD/Mon/YYYY:HH:MM:SS +0000]`.
fn clf_timestamp(ts: i64) -> String {
    let (secs_of_day, days) = {
        let r = ts % 86_400;
        if r < 0 {
            (r + 86_400, ts / 86_400 - 1)
        } else {
            (r, ts / 86_400)
        }
    };
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    let (y, mo, d) = days_to_ymd(days);
    let mon = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(mo - 1) as usize];
    format!("[{d:02}/{mon}/{y:04}:{h:02}:{m:02}:{s:02} +0000]")
}

// ── Timestamp parsing ─────────────────────────────────────────────────────────

/// Parse ISO 8601 UTC timestamp: `YYYY-MM-DDTHH:MM:SS[.mmm]Z`
/// Only handles UTC (Z suffix); fractional seconds are truncated.
fn parse_iso8601(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let year = parse4(b[0], b[1], b[2], b[3])? as i64;
    // b[4] = '-'
    let month = parse2(b[5], b[6])? as i64;
    // b[7] = '-'
    let day = parse2(b[8], b[9])? as i64;
    // b[10] = 'T'
    let hour = parse2(b[11], b[12])? as i64;
    // b[13] = ':'
    let min = parse2(b[14], b[15])? as i64;
    // b[16] = ':'
    let sec = parse2(b[17], b[18])? as i64;
    // b[19] = '.' or 'Z' — always UTC, fractional seconds ignored
    Some(days_since_epoch(year, month, day) * 86_400 + hour * 3600 + min * 60 + sec)
}

#[inline]
fn digit(b: u8) -> Option<u8> {
    if b.wrapping_sub(b'0') <= 9 {
        Some(b - b'0')
    } else {
        None
    }
}

#[inline]
fn parse2(a: u8, b: u8) -> Option<u8> {
    Some(digit(a)? * 10 + digit(b)?)
}

#[inline]
fn parse4(a: u8, b: u8, c: u8, d: u8) -> Option<u16> {
    Some(
        digit(a)? as u16 * 1000 + digit(b)? as u16 * 100 + digit(c)? as u16 * 10 + digit(d)? as u16,
    )
}

#[cfg(test)]
#[path = "caddy_tests.rs"]
mod tests;
