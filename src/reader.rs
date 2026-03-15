use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use xz2::read::XzDecoder;
use zstd::stream::read::Decoder as ZstdDecoder;

/// Read buffer: 4 MiB reduces syscall count vs the former 256 KiB.
pub const READER_BUF: usize = 4 * 1024 * 1024;

pub enum Compression { None, Gz, Bz2, Xz, Zstd }

/// Detect compression from the first bytes of a byte slice (magic numbers).
pub fn detect_from_bytes(magic: &[u8]) -> Compression {
    if magic.len() >= 2 && magic[..2] == [0x1f, 0x8b] {
        Compression::Gz
    } else if magic.len() >= 3 && magic[..3] == [0x42, 0x5a, 0x68] {
        Compression::Bz2
    } else if magic.len() >= 6 && magic[..6] == [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00] {
        Compression::Xz
    } else if magic.len() >= 4 && magic[..4] == [0x28, 0xb5, 0x2f, 0xfd] {
        Compression::Zstd
    } else {
        Compression::None
    }
}

/// Detect compression from a seekable File, then rewind to the start.
fn detect_file(file: &mut File) -> Result<Compression> {
    let mut magic = [0u8; 6];
    let n = file.read(&mut magic)?;
    use std::io::Seek;
    file.seek(std::io::SeekFrom::Start(0))?;
    Ok(detect_from_bytes(&magic[..n]))
}

/// Open a log file as a buffered reader.
///
/// Pass `-` as the path to read from stdin.
/// Compression is auto-detected via magic bytes in both cases.
pub fn open(path: &Path) -> Result<Box<dyn BufRead + Send>> {
    if path == Path::new("-") {
        return open_stdin();
    }

    let mut file = File::open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;

    Ok(match detect_file(&mut file)? {
        Compression::Gz => {
            let inner = BufReader::with_capacity(READER_BUF, file);
            Box::new(BufReader::with_capacity(READER_BUF, GzDecoder::new(inner)))
        }
        Compression::Bz2 => {
            let inner = BufReader::with_capacity(READER_BUF, file);
            Box::new(BufReader::with_capacity(READER_BUF, BzDecoder::new(inner)))
        }
        Compression::Xz => {
            let inner = BufReader::with_capacity(READER_BUF, file);
            Box::new(BufReader::with_capacity(READER_BUF, XzDecoder::new(inner)))
        }
        Compression::Zstd => {
            let inner = BufReader::with_capacity(READER_BUF, file);
            let dec = ZstdDecoder::new(inner)
                .with_context(|| format!("cannot init zstd decoder for {}", path.display()))?;
            Box::new(BufReader::with_capacity(READER_BUF, dec))
        }
        Compression::None => Box::new(BufReader::with_capacity(READER_BUF, file)),
    })
}

/// Open stdin as a buffered reader, auto-detecting compression via peek.
fn open_stdin() -> Result<Box<dyn BufRead + Send>> {
    let mut reader = BufReader::with_capacity(READER_BUF, std::io::stdin());

    // fill_buf() fills the internal buffer but does NOT advance the read
    // position, so the magic bytes are still available to the decoder.
    let compression = {
        let magic = reader.fill_buf().context("cannot read stdin")?;
        detect_from_bytes(magic)
    };

    Ok(match compression {
        Compression::Gz   => Box::new(BufReader::with_capacity(READER_BUF, GzDecoder::new(reader))),
        Compression::Bz2  => Box::new(BufReader::with_capacity(READER_BUF, BzDecoder::new(reader))),
        Compression::Xz   => Box::new(BufReader::with_capacity(READER_BUF, XzDecoder::new(reader))),
        Compression::Zstd => {
            let dec = ZstdDecoder::new(reader).context("cannot init zstd decoder for stdin")?;
            Box::new(BufReader::with_capacity(READER_BUF, dec))
        }
        Compression::None => Box::new(reader),
    })
}
