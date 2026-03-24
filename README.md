# mergelog-rs

[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

Merge and sort HTTP log files chronologically — Rust rewrite of
[mergelog 4.5](https://mergelog.sourceforge.net/) by Bertrand Demiddelaer.

Reads multiple HTTP access log files (**NCSA Combined Log Format** and **Caddy
JSON**) and writes a single chronologically sorted CLF stream to stdout.
Designed for consolidating logs from multiple servers behind a round-robin DNS.

> Full background and benchmarks: **[Alek's Blog — mergelog-rs](https://blog.none.at/blog/2026/2026-03-15-mergelog-rs/)**

---

## Features

- **Caddy JSON support** — auto-detects Caddy JSON access logs per file and converts them to CLF on the fly
- **Mixed input** — CLF and Caddy JSON files can be merged in a single run; format is detected per file
- **Auto-detect compression** via magic bytes — gzip, bzip2, xz, zstd, and plain text, no file extension needed
- **Timezone-aware** — timestamps are converted to UTC before comparison; logs from servers in different timezones sort correctly
- **O(N log K) k-way heap merge** — scales with the number of lines, not the time span of the logs
- **stdin support** — pass `-` as a filename to read from stdin
- **Statistics** — `--stats` reports per-file line counts, time ranges and malformed lines
- **2.26× faster** than the original C binary on the same workload

---

## Installation

### Pre-built binaries

Download the latest release for your platform from the
[Releases](https://github.com/git001/mergelog-rs/releases) page:

| Platform | Archive |
|----------|---------|
| Linux x86_64 (glibc) | `mergelog-rs-v1.0.0-x86_64-unknown-linux-gnu.tar.gz` |
| Linux x86_64 (static musl) | `mergelog-rs-v1.0.0-x86_64-unknown-linux-musl.tar.gz` |
| Linux arm64 (static musl) | `mergelog-rs-v1.0.0-aarch64-unknown-linux-musl.tar.gz` |
| macOS Apple Silicon | `mergelog-rs-v1.0.0-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `mergelog-rs-v1.0.0-x86_64-apple-darwin.tar.gz` |

### Container image

```sh
podman pull ghcr.io/git001/mergelog-rs:latest
# or
docker pull ghcr.io/git001/mergelog-rs:latest
```

### Build from source

Requires Rust 1.94+.

```sh
git clone https://github.com/git001/mergelog-rs
cd mergelog-rs
cargo build --release --locked
# binary at target/release/mergelog-rs
```

### Install binary + man page

```sh
make install                                      # to /usr/local
make install PREFIX=/usr                          # custom prefix
make install DESTDIR=/tmp/pkg PREFIX=/usr         # for packaging

# or with just:
just install
PREFIX=/usr just install
```

If pandoc is not in `PATH`, specify it explicitly:

```sh
make install PANDOC=/path/to/pandoc
PANDOC=/path/to/pandoc just install
```

---

## Usage

```
mergelog-rs [OPTIONS] <FILE>...
```

### Options

| Option | Description |
|--------|-------------|
| `<FILE>...` | Log files to merge. Use `-` for stdin. |
| `--progress` | Print running line count to stderr every 10,000 lines. |
| `--stats` | Print per-file statistics to stderr after the merge. |
| `-o, --output <FILE>` | Write to a file instead of stdout. |
| `--format <FORMAT>` | Force log format: `clf` or `caddy`. Default: auto-detect per file. |
| `-h, --help` | Print help. |
| `-V, --version` | Print version. |

### Examples

```sh
# Merge CLF log files
mergelog-rs access1.log access2.log access3.log > merged.log

# Merge Caddy JSON logs (auto-detected, output is CLF)
mergelog-rs caddy1.log caddy2.log > merged.log

# Mix CLF and Caddy JSON — format detected per file
mergelog-rs nginx.log caddy.log > merged.log

# Force Caddy format for all files (e.g. multiple pods)
mergelog-rs --format caddy pod1.log pod2.log > merged.log

# Merge compressed files — compression auto-detected via magic bytes
mergelog-rs access.log.gz archive.log.bz2 old.log.xz backup.log.zst > merged.log

# Read from stdin (e.g. over SSH), merge with a local file
ssh web01 "cat /var/log/caddy/access.log" | mergelog-rs - /var/log/caddy/access.log

# Write to a file and show statistics
mergelog-rs --stats -o merged.log access1.log access2.log

# Run in a container
podman run --rm -v /var/log/caddy:/logs:ro ghcr.io/git001/mergelog-rs \
  /logs/access1.log /logs/access2.log > merged.log
```

### Statistics output

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

---

## Development Tools

### `src/bin/profile_timing.rs`

A standalone timing binary used during development to identify where CPU time
is spent. Since `perf` was locked down on the target system
(`perf_event_paranoid = 4`), this binary instruments each phase of the merge
manually using `std::time::Instant`:

```sh
cargo build --release --bin profile_timing
target/release/profile_timing access1.log access2.log ... > /dev/null
```

Example output:

```
──── profile_timing (41.65M lines) ──────────────────
  I/O   (read_line)       3.57 s   47.1%
  Parse (hand-rolled)     1.47 s   19.5%
  Heap  (push/pop)        1.87 s   24.7%
  Write (write_all)       0.65 s    8.7%
  ─────────────────────────────────────────────────
  Σ measured              7.56 s
```

This tool was how the bottlenecks (`jiff::strptime` at 45.7%, then I/O at
47.1%) were found and quantified — driving the hand-rolled parser, buffer
reuse, and 4 MiB buffer optimizations. See the
[blog post](https://blog.none.at/blog/2026/2026-03-15-mergelog-rs/) for the
full story.

---

## Performance

Measured on 7 × 1 GiB log files (41.65 million lines, 7 GiB total):

| Version | Wall time | Peak RSS |
|---------|-----------|----------|
| mergelog-4.5 (C, 2001) | 9.98 s | 1.6 MiB |
| **mergelog-rs 1.0.0** | **4.43 s** | **30.7 MiB** |

**2.26× faster.** The higher RSS comes from 4 MiB read buffers per input file
(7 × 4 MiB = 28 MiB) — a deliberate speed/memory trade-off.

Key optimizations over a naive Rust port:

1. K-way heap merge instead of second-by-second iteration
2. Hand-rolled CLF timestamp parser (3.7× faster than `jiff::strptime`)
3. `String` buffer reuse across heap iterations
4. mimalloc as the global allocator
5. 4 MiB `BufReader` + SIMD `memchr` newline search

---

## Log Formats

### CLF — NCSA Combined Log Format

`%h %l %u %t "%r" %>s %b "%{Referer}i" "%{User-agent}i"` — produced by Apache, nginx, and others:

```
93.184.216.34 - alice [15/Mar/2026:12:00:00 +0100] "GET / HTTP/1.1" 200 4096 "https://example.com/" "Mozilla/5.0"
```

See the [Apache mod_log_config documentation](https://httpd.apache.org/docs/2.4/mod/mod_log_config.html)
for the full format reference.

### Caddy JSON

One JSON object per line, as written by Caddy's `json` log encoder:

```json
{"ts":"2026-03-22T14:50:51.525Z","request":{"client_ip":"1.2.3.4","method":"GET","uri":"/","proto":"HTTP/1.1","headers":{"User-Agent":["Mozilla/5.0"]}},"status":200,"size":4096,"user_id":""}
```

The `ts` field may be an ISO 8601 string or a Unix float. Non-request lines
(error/info/debug entries) are skipped silently. Output is always CLF.

---

## Acknowledgements

Full respect and honour to **Bertrand Demiddelaer**, who created mergelog in
2000. Writing a tool this fast, correct, and compact in C — with no external
dependencies beyond zlib — is genuinely impressive. mergelog-rs stands on his
shoulders.

---

## License

GPL-3.0-or-later.
Based on mergelog 4.5 © 2000–2001 Bertrand Demiddelaer (GPL-2.0-or-later).
