//! Output comparison for judged runs.
//!
//! The rules are unchanged (CRLF normalisation, trailing spaces/tabs ignored,
//! trailing blank lines insignificant, everything else byte-exact), but they
//! are applied while **streaming**: one output can be up to
//! `MAX_TESTDATA_EXTRACTED_BYTES` (512 MiB), and materialising both sides plus
//! a `Vec<Vec<u8>>` of normalized lines put the worker close to its 2 GiB
//! container limit for a single judgement.

use std::fs::File;
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;

/// Longest line the comparator is willing to buffer. A longer line is treated
/// as a mismatch (contestant fault) rather than an allocation.
const MAX_LINE_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn standard_output_matches(expected: &[u8], actual: &[u8]) -> bool {
    compare(Cursor::new(expected), Cursor::new(actual)).unwrap_or(false)
}

/// Compares two files line by line without loading either one.
pub(super) fn files_match(expected: &Path, actual: &Path) -> std::io::Result<bool> {
    compare(BufReader::new(File::open(expected)?), BufReader::new(File::open(actual)?))
}

/// Compares a (trusted, potentially large) expected file against output already
/// held in memory.
pub(super) fn file_matches_bytes(expected: &Path, actual: &[u8]) -> std::io::Result<bool> {
    compare(BufReader::new(File::open(expected)?), Cursor::new(actual))
}

fn compare<L: BufRead, R: BufRead>(left: L, right: R) -> std::io::Result<bool> {
    let mut left = LineCursor::new(left);
    let mut right = LineCursor::new(right);
    loop {
        match (left.next_line()?, right.next_line()?) {
            (Some(left), Some(right)) => {
                if left != right {
                    return Ok(false);
                }
            }
            // Trailing blank lines on either side are insignificant.
            (Some(line), None) | (None, Some(line)) => {
                if !line.is_empty() {
                    return Ok(false);
                }
            }
            (None, None) => return Ok(true),
        }
    }
}

/// Emits one normalized line at a time: `\r\n`, `\r` and `\n` all terminate a
/// line, and trailing spaces/tabs are dropped.
struct LineCursor<R: BufRead> {
    reader: R,
    /// A lone CR terminated the previous line; the next LF belongs to it.
    skip_lf: bool,
}

impl<R: BufRead> LineCursor<R> {
    const fn new(reader: R) -> Self {
        Self { reader, skip_lf: false }
    }

    fn next_line(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        let mut line: Vec<u8> = Vec::new();
        loop {
            if self.skip_lf {
                self.skip_lf = false;
                let available = self.reader.fill_buf()?;
                if available.first() == Some(&b'\n') {
                    self.reader.consume(1);
                }
            }
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                return if line.is_empty() { Ok(None) } else { Ok(Some(trim_end(&line))) };
            }
            match available.iter().position(|byte| matches!(byte, b'\n' | b'\r')) {
                Some(index) => {
                    line.extend_from_slice(&available[..index]);
                    let terminator = available[index];
                    self.reader.consume(index + 1);
                    self.skip_lf = terminator == b'\r';
                    if line.len() as u64 > MAX_LINE_BYTES {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "output line exceeds the comparison limit",
                        ));
                    }
                    return Ok(Some(trim_end(&line)));
                }
                None => {
                    line.extend_from_slice(available);
                    let consumed = available.len();
                    self.reader.consume(consumed);
                    if line.len() as u64 > MAX_LINE_BYTES {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "output line exceeds the comparison limit",
                        ));
                    }
                }
            }
        }
    }
}

fn trim_end(line: &[u8]) -> Vec<u8> {
    let end = line
        .iter()
        .rposition(|byte| !matches!(byte, b' ' | b'\t'))
        .map_or(0, |position| position + 1);
    line[..end].to_vec()
}
