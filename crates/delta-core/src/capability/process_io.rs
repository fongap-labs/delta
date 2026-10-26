//! Reading what a child process writes, for the runners that supervise one.
//!
//! A child's output arrives on pipes that a reader thread drains. Three things go wrong with the
//! obvious `BufReader::lines()` loop, and each of them has hurt a runner here:
//!
//! - a line that is not valid UTF-8 (a native tool on a GBK console, say) ends the loop, the pipe
//!   is closed on a child that is still writing, and everything after it is lost;
//! - a child that prints without ever ending a line grows the line buffer without bound;
//! - waiting for the reader to finish waits for every process that still holds the pipe, so a
//!   background process the command left running keeps a finished (or timed-out) call waiting.
//!
//! The functions here read bytes, cut an over-long line, decode lossily, and give the readers a
//! short grace period to finish instead of waiting for them.

use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::sync::mpsc::{Sender, SyncSender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a supervisor waits, once the child has ended, for the readers to see the end of their
/// pipes. A reader still blocked after that is held open by a process the child left behind; it is
/// detached and stops by itself when that process lets go of the pipe or the receiver is gone.
pub(super) const READER_GRACE: Duration = Duration::from_millis(500);

const POLL: Duration = Duration::from_millis(10);

/// Where a reader sends its lines: an unbounded channel, or a bounded one whose full queue makes
/// the reader wait, which in turn makes the child wait on its pipe.
pub(super) trait LineSink<T>: Send + 'static {
    /// `false` when nobody is receiving any more.
    fn send_line(&self, item: T) -> bool;
}

impl<T: Send + 'static> LineSink<T> for Sender<T> {
    fn send_line(&self, item: T) -> bool {
        self.send(item).is_ok()
    }
}

impl<T: Send + 'static> LineSink<T> for SyncSender<T> {
    fn send_line(&self, item: T) -> bool {
        self.send(item).is_ok()
    }
}

/// Read one line into `line` without its terminator, keeping at most `max` bytes of it and
/// discarding the rest of a longer line. `Ok(false)` at the end of the stream.
pub(super) fn read_capped_line(
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
    max: usize,
) -> std::io::Result<bool> {
    line.clear();
    let mut has_data = false;
    loop {
        let (used, is_line_end) = {
            let chunk = match reader.fill_buf() {
                Ok(chunk) => chunk,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if chunk.is_empty() {
                return Ok(has_data);
            }
            has_data = true;
            let end = chunk.iter().position(|byte| *byte == b'\n');
            let room = max.saturating_sub(line.len());
            let kept = end.unwrap_or(chunk.len()).min(room);
            line.extend_from_slice(&chunk[..kept]);
            (end.map_or(chunk.len(), |at| at + 1), end.is_some())
        };
        reader.consume(used);
        if is_line_end {
            return Ok(true);
        }
    }
}

/// Drain `stream` on a thread of its own, sending each line (decoded lossily, without its line
/// ending, cut at `max_line` bytes) through `wrap` into `sink`. The thread ends with the stream or
/// when the sink has no receiver.
pub(super) fn spawn_line_reader<R, T, S>(
    name: &str,
    stream: R,
    max_line: usize,
    sink: S,
    wrap: impl Fn(String) -> T + Send + 'static,
) -> std::io::Result<JoinHandle<()>>
where
    R: Read + Send + 'static,
    T: Send + 'static,
    S: LineSink<T>,
{
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut line = Vec::new();
            while let Ok(true) = read_capped_line(&mut reader, &mut line, max_line) {
                let text = String::from_utf8_lossy(&line);
                if !sink.send_line(wrap(text.trim_end_matches('\r').to_string())) {
                    break;
                }
            }
        })
}

/// Give `readers` up to [`READER_GRACE`] to finish, calling `drain` while waiting and once more at
/// the end so nothing they sent is left in the channel. Readers that are still blocked are
/// detached, not waited for.
pub(super) fn join_readers_within(readers: Vec<JoinHandle<()>>, drain: &mut dyn FnMut()) {
    let deadline = Instant::now() + READER_GRACE;
    while Instant::now() < deadline && readers.iter().any(|reader| !reader.is_finished()) {
        drain();
        std::thread::sleep(POLL);
    }
    for reader in readers {
        if reader.is_finished() {
            let _ = reader.join();
        }
    }
    drain();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn lines_of(bytes: &[u8], max: usize) -> Vec<Vec<u8>> {
        let mut reader = BufReader::with_capacity(8, Cursor::new(bytes.to_vec()));
        let mut line = Vec::new();
        let mut out = Vec::new();
        while read_capped_line(&mut reader, &mut line, max).unwrap() {
            out.push(line.clone());
        }
        out
    }

    #[test]
    fn lines_are_split_on_newlines_and_a_last_line_needs_no_newline() {
        assert_eq!(
            lines_of(b"a\nbb\n\nccc", 100),
            vec![b"a".to_vec(), b"bb".to_vec(), vec![], b"ccc".to_vec()]
        );
        assert!(lines_of(b"", 100).is_empty());
    }

    #[test]
    fn a_line_over_the_limit_is_cut_and_the_next_line_is_intact() {
        let out = lines_of(b"0123456789abcdef\nnext\n", 5);
        assert_eq!(out, vec![b"01234".to_vec(), b"next".to_vec()]);
    }

    #[test]
    fn bytes_that_are_not_utf8_do_not_stop_the_reader() {
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = spawn_line_reader(
            "test-reader",
            Cursor::new(b"before\n\xff\xfe noise\r\nafter\n".to_vec()),
            100,
            tx,
            |line| line,
        )
        .unwrap();
        reader.join().unwrap();
        let got: Vec<String> = rx.try_iter().collect();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], "before");
        assert!(got[1].contains("noise") && !got[1].ends_with('\r'));
        assert_eq!(got[2], "after");
    }
}
