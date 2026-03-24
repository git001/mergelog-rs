---
title: MERGELOG-RS
section: 1
date: 2026-03-24
author: mergelog-rs contributors
---

# mergelog-rs

Merge and sort HTTP log files chronologically — Rust rewrite of [mergelog 4.5](https://mergelog.sourceforge.net/) by Bertrand Demiddelaer.

## Synopsis

```
mergelog-rs [OPTIONS] <FILE>...
```

## Description

`mergelog-rs` reads multiple HTTP log files and writes a single chronologically sorted stream to stdout (or a file). It is designed for consolidating logs from multiple servers behind a round-robin DNS.

Supported input formats: **NCSA Combined Log Format** (CLF) and **Caddy JSON** access logs. The format is auto-detected per file — no flag needed for mixed inputs.

All input files must be sorted chronologically within themselves (as Apache, nginx, and Caddy produce them). Timestamps are compared in UTC — timezone offsets in CLF lines are correctly handled.

## Log Formats

### CLF — NCSA Combined Log Format

```
%h %l %u %t "%r" %>s %b "%{Referer}i" "%{User-agent}i"
```

Example:

```
93.184.216.34 - alice [15/Mar/2026:12:00:00 +0100] "GET /index.html HTTP/1.1" 200 4096 "https://example.com/" "Mozilla/5.0"
```

The timestamp field `%t` uses the format `[DD/Mon/YYYY:HH:MM:SS ±HHMM]`.

Lines whose timestamp cannot be parsed are sorted to the **end** of the output (matching the behaviour of the original mergelog 4.5).

### Caddy JSON

Caddy access logs written in JSON format (one object per line):

```json
{"ts":"2026-03-22T14:50:51.525Z","request":{"client_ip":"1.2.3.4","method":"GET","uri":"/","proto":"HTTP/1.1","headers":{}},"status":200,"size":1234,"user_id":""}
```

The `ts` field may be an ISO 8601 string (`"2026-03-22T14:50:51.525Z"`) or a Unix float (`1742651451.525`). Non-request lines (error/info/debug entries without a `request` field) are skipped silently. Output is always CLF.

## Compression

Compression is auto-detected via **magic bytes** — no file extension is required:

| Format | Magic bytes      |
|--------|-----------------|
| gzip   | `1F 8B`          |
| bzip2  | `42 5A 68` (BZh) |
| xz     | `FD 37 7A 58 5A 00` |
| zstd   | `28 B5 2F FD`    |
| plain  | (anything else)  |

## Options

| Option | Description |
|--------|-------------|
| `<FILE>...` | One or more log files to merge. Use `-` for stdin. |
| `--format <FORMAT>` | Force log format: `clf` or `caddy`. Default: auto-detect per file. |
| `--progress` | Print a running line count to stderr every 10,000 lines. |
| `--stats` | Print per-file statistics to stderr after the merge. |
| `-o, --output <FILE>` | Write output to a file instead of stdout. |
| `-h, --help` | Print help. |
| `-V, --version` | Print version. |

## Examples

**Merge plain-text CLF log files:**
```sh
mergelog-rs access1.log access2.log access3.log > merged.log
```

**Merge Caddy JSON logs (auto-detected, output is CLF):**
```sh
mergelog-rs caddy1.log caddy2.log > merged.log
```

**Mix CLF and Caddy JSON (auto-detected per file):**
```sh
mergelog-rs nginx.log caddy.log > merged.log
```

**Force Caddy format for all files:**
```sh
mergelog-rs --format caddy pod1.log pod2.log > merged.log
```

**Merge compressed log files (compression auto-detected):**
```sh
mergelog-rs access.log.gz archive.log.bz2 old.log.xz > merged.log
```

**Read from stdin (e.g. over SSH):**
```sh
ssh web01 "cat /var/log/caddy/access.log" | mergelog-rs - /var/log/caddy/access.log
```

**Write to a file instead of stdout:**
```sh
mergelog-rs -o merged.log access1.log access2.log
```

**Show progress and statistics:**
```sh
mergelog-rs --progress --stats access1.log access2.log > merged.log
```

**Run in a container:**
```sh
podman run --rm -v /var/log/caddy:/logs:ro ghcr.io/git001/mergelog-rs \
  /logs/access1.log /logs/access2.log > merged.log
```

## Statistics Output

With `--stats`, a summary is printed to stderr after the merge:

```
──── merge statistics ──────────────────────────────────────
  Files    : 3
  Lines    : 12,450,000  (0 malformed)
  Range    : 2024-01-01 00:00:00 UTC → 2024-12-31 23:59:59 UTC
────────────────────────────────────────────────────────────
  access-eu.log    4,150,000 lines  2024-01-01 00:00:00 UTC → 2024-12-31 22:00:00 UTC
  access-us.log    4,150,000 lines  2024-01-01 05:00:00 UTC → 2024-12-31 23:59:59 UTC
  access-ap.log    4,150,000 lines  2024-01-01 09:00:00 UTC → 2024-12-31 21:00:00 UTC
────────────────────────────────────────────────────────────
```

## Exit Status

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Error (file not found, I/O error, invalid arguments) |

## Performance

Measured on 7 × 1 GiB log files (41.65 million lines):

| | mergelog-4.5 (C) | mergelog-rs 1.0.0 |
|--|--|--|
| Wall time | 9.98 s | **4.43 s** |
| Peak RSS | 1.6 MiB | 30.7 MiB |

mergelog-rs is **2.26× faster** than the original, using a k-way heap merge (O(N log K)) instead of the original's second-by-second iteration (O(timespan × K)).

The higher memory usage comes from 4 MiB read buffers per input file (7 × 4 MiB = 28 MiB). This can be reduced by adjusting `READER_BUF` in `src/reader.rs` if memory is constrained.

## Limitations

- Each input file must be internally sorted by timestamp (i.e. entries must be in chronological order within the file). `mergelog-rs` merges already-sorted files — it does not sort within a single file. Apache, nginx, and Caddy write logs sequentially, so this is always satisfied in practice.
- Only one file can be read from stdin at a time (`-`).
- Caddy JSON output is always converted to CLF; the JSON fields are not preserved.
- Open file descriptor limit applies (`ulimit -n`). Increase with `ulimit -n 65536` if merging many files.

## See Also

- `mergelog(1)` — original C implementation
- Apache [mod_log_config](https://httpd.apache.org/docs/2.4/mod/mod_log_config.html) — log format reference

## License

GPL-3.0-or-later. Based on mergelog 4.5 © 2000–2001 Bertrand Demiddelaer (GPL-2.0-or-later).
