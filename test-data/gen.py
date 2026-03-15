#!/usr/bin/env python3
"""
Generate synthetic NCSA Combined Log Format files (~1 GB each).
7 plain text + 1 gzip + 1 bzip2 + 1 xz, spanning different months/years/timezones.
"""

import gzip
import bz2
import lzma
import io
import os
import random
import sys
from datetime import datetime, timezone, timedelta

MONTH_ABBR = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"]

IPS = (
    ["93.184.216.34", "172.217.16.46", "151.101.1.69", "104.16.132.229"]
    + ["192.168.1.{}".format(i) for i in range(1, 50)]
)

USERS = ["-", "-", "-", "alice", "bob", "carol", "dave", "-", "-", "-"]

METHODS = ["GET", "GET", "GET", "GET", "POST", "PUT", "DELETE", "HEAD"]

PATHS = [
    "/", "/index.html", "/about", "/contact", "/login", "/logout",
    "/api/v1/users", "/api/v1/posts", "/api/v2/items", "/static/main.css",
    "/static/app.js", "/images/logo.png", "/images/banner.jpg",
    "/products/123", "/products/456", "/products/789",
    "/blog/2022/hello-world", "/blog/2023/update",
    "/search?q=example&page=1", "/sitemap.xml", "/robots.txt",
    "/favicon.ico", "/wp-admin/", "/phpinfo.php",
]

STATUS = [200, 200, 200, 200, 200, 301, 302, 304, 400, 403, 404, 404, 500]

REFERERS = [
    "-", "-", "-",
    "https://www.google.com/",
    "https://www.bing.com/search?q=test",
    "https://example.com/page",
    "https://news.ycombinator.com/",
    "https://duckduckgo.com/?q=apache+logs",
    "-",
]

USER_AGENTS = [
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15',
    'Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/121.0',
    'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1',
    'curl/7.88.1',
    'python-requests/2.31.0',
    'Googlebot/2.1 (+http://www.google.com/bot.html)',
    'facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)',
    'Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)',
    'Mozilla/5.0 (Windows NT 6.1; Trident/7.0; rv:11.0) like Gecko',
]

# (start_year, start_month, tz_offset_hours)
FILE_CONFIGS = [
    # plain files
    (2020, 1,  +1),   # access-2020-eu.log
    (2020, 7,  -5),   # access-2020-us.log
    (2021, 3,  +9),   # access-2021-jp.log
    (2021, 9,  +0),   # access-2021-utc.log
    (2022, 2,  +5),   # access-2022-in.log
    (2022, 8,  -8),   # access-2022-us-west.log
    (2023, 4,  +2),   # access-2023-eu-east.log
    # compressed
    (2023, 10, -3),   # access-2023-br.log.gz
    (2024, 1,  +8),   # access-2024-cn.log.bz2
    (2024, 6,  +3),   # access-2024-ru.log.xz
]

FILE_NAMES = [
    "access-2020-eu.log",
    "access-2020-us.log",
    "access-2021-jp.log",
    "access-2021-utc.log",
    "access-2022-in.log",
    "access-2022-us-west.log",
    "access-2023-eu-east.log",
    "access-2023-br.log.gz",
    "access-2024-cn.log.bz2",
    "access-2024-xz.log.xz",
]

TARGET_UNCOMPRESSED = 1 * 1024 * 1024 * 1024  # 1 GiB uncompressed per file
CHUNK_LINES = 50_000  # build this many lines at once then write

rng = random.Random(42)

def rand_ip():
    return "{}.{}.{}.{}".format(rng.randint(1,254), rng.randint(0,255),
                                rng.randint(0,255), rng.randint(1,254))

def make_timestamp(base_dt: datetime, seconds_offset: int, tz_hours: int) -> str:
    dt = base_dt + timedelta(seconds=seconds_offset)
    sign = '+' if tz_hours >= 0 else '-'
    ah = abs(tz_hours)
    return "[{:02d}/{}/{:04d}:{:02d}:{:02d}:{:02d} {}{:02d}00]".format(
        dt.day, MONTH_ABBR[dt.month - 1], dt.year,
        dt.hour, dt.minute, dt.second,
        sign, ah
    )

def build_chunk(start_year, start_month, tz_hours, base_offset, n_lines):
    base = datetime(start_year, start_month, 1, 0, 0, 0)
    lines = []
    for i in range(n_lines):
        ip   = rand_ip()
        user = rng.choice(USERS)
        ts   = make_timestamp(base, base_offset + i * rng.randint(1, 5), tz_hours)
        meth = rng.choice(METHODS)
        path = rng.choice(PATHS)
        proto= "HTTP/1.1"
        stat = rng.choice(STATUS)
        size = rng.randint(100, 65535) if stat not in (304, 204) else 0
        sz_s = str(size) if size > 0 else "-"
        ref  = rng.choice(REFERERS)
        ua   = rng.choice(USER_AGENTS)
        line = f'{ip} - {user} {ts} "{meth} {path} {proto}" {stat} {sz_s} "{ref}" "{ua}"\n'
        lines.append(line)
    return "".join(lines).encode()

def open_output(path):
    if path.endswith(".gz"):
        return gzip.open(path, "wb", compresslevel=1)
    elif path.endswith(".bz2"):
        return bz2.open(path, "wb", compresslevel=1)
    elif path.endswith(".xz"):
        return lzma.open(path, "wb", preset=0)
    else:
        return open(path, "wb")

out_dir = os.path.dirname(os.path.abspath(__file__))

for idx, (fname, (start_year, start_month, tz_hours)) in enumerate(zip(FILE_NAMES, FILE_CONFIGS)):
    fpath = os.path.join(out_dir, fname)
    print(f"[{idx+1}/10] Generating {fname} ...", flush=True)
    written = 0
    offset  = 0
    with open_output(fpath) as f:
        while written < TARGET_UNCOMPRESSED:
            chunk = build_chunk(start_year, start_month, tz_hours, offset, CHUNK_LINES)
            f.write(chunk)
            written += len(chunk)
            offset  += CHUNK_LINES * 3
            pct = written * 100 // TARGET_UNCOMPRESSED
            print(f"  {pct:3d}%  ({written // 1_048_576} MiB uncompressed)", end="\r", flush=True)
    size_mb = os.path.getsize(fpath) / 1_048_576
    print(f"  done  → {size_mb:.0f} MiB on disk                          ")

print("All done.")
