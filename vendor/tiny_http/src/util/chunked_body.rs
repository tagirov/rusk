use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Result as IoResult;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chunked_transfer::Decoder;

use crate::util::refined_tcp_stream::timed_out;

/// The buffer the rest of a body left unread is thrown away through (see
/// the `Drop` below).
const DISCARD_BUFFER_BYTES: usize = 8 * 1024;

/// What one read of the decoder may take from the source beyond what it
/// gives back: a chunk's size line (size, extensions) and the CRLFs around
/// the chunk. The decoder keeps a size line in memory until its end,
/// however long it is (review of R34: a client sent gigabytes of hex
/// digits, at the pace, and the server kept them all); past this much, the
/// body is in the wrong format.
const CHUNK_LINE_BYTES: usize = 4 * 1024;

/// rusk: a chunked request body — `chunked_transfer`'s `Decoder`, with
/// what it gets wrong about its source put right:
///
/// - A body whose connection was cut off — a read of it, or a write of the
///   answer, waited its time out (see `RefinedTcpStream`) — fails as timed
///   out wherever it stopped. The decoder turned a read that timed out in
///   the CRLF after a chunk's size or data, or at the end of the body, into
///   a format error (`InvalidInput`, "Error while decoding chunks"), and so
///   the end of the reads that follows a timeout too.
/// - A source that ends inside a chunk is not the end of the body: the
///   decoder gave `Ok(0)`, and the part that came passed for all of it.
/// - A body that goes wrong ends the reading of its connection (`None`: a
///   request of no connection), since where the next request starts is not
///   known then; its answer says so (`Connection: close`). Upstream read
///   the next request from wherever the decoder stopped.
/// - The rest of a body the request leaves unread is read and thrown away
///   when it goes, as `EqualReader` does for one of a known length, so that
///   the next request is read from where it starts, and the answer is not
///   lost to the reset of a connection closed with bytes unread. Upstream
///   read the next request from the middle of this body (an answer to a
///   request that was never sent, `400 Bad Request` mostly, and the end of
///   the connection).
/// - A chunk's size line may take `CHUNK_LINE_BYTES`, no more (`Metered`).
///
/// Every read after the body has ended gives nothing (EOF); every read
/// after it went wrong fails the same way.
pub struct ChunkedBody<R>
where
    R: Read,
{
    decoder: Decoder<Metered<R>>,
    done_reading: Option<Arc<AtomicBool>>,
    ended: bool,
    failed: Option<ErrorKind>,
}

/// rusk: a source that gives so many bytes and no more (see
/// `CHUNK_LINE_BYTES`): the budget of one read of the decoder.
struct Metered<R> {
    source: R,
    left: usize,
}

impl<R> Read for Metered<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if self.left == 0 {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                "a chunk's size line is too long",
            ));
        }
        let take = buf.len().min(self.left);
        let read = self.source.read(&mut buf[..take])?;
        self.left -= read;
        Ok(read)
    }
}

impl<R> ChunkedBody<R>
where
    R: Read,
{
    /// `done_reading` is the connection's flag (see `RefinedTcpStream`),
    /// set by now if the connection has been cut off.
    pub fn new(source: R, done_reading: Option<Arc<AtomicBool>>) -> ChunkedBody<R> {
        ChunkedBody {
            decoder: Decoder::new(Metered { source, left: 0 }),
            done_reading,
            ended: false,
            failed: None,
        }
    }

    fn cut_off(&self) -> bool {
        self.done_reading
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    }
}

impl<R> Read for ChunkedBody<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if let Some(kind) = self.failed {
            return Err(IoError::new(kind, "the chunked body went wrong before"));
        }
        if self.ended || buf.is_empty() {
            return Ok(0);
        }
        // One read of the decoder takes a chunk's size line at most, and
        // data up to `buf`: what it may take from the source.
        self.decoder.get_mut().left = buf.len().saturating_add(CHUNK_LINE_BYTES);
        let result = match self.decoder.read(buf) {
            Ok(0) if self.decoder.remaining_chunks_size().is_some() => Err(IoError::new(
                ErrorKind::UnexpectedEof,
                "the body ended inside a chunk",
            )),
            other => other,
        };
        match result {
            Ok(0) => {
                self.ended = true;
                Ok(0)
            }
            Ok(read) => Ok(read),
            Err(err) if err.kind() == ErrorKind::Interrupted => Err(err),
            Err(err) => {
                // Only a timeout sets the flag before this does (see
                // `RefinedTcpStream`).
                let err = if self.cut_off() && !timed_out(&err) {
                    IoError::new(
                        ErrorKind::TimedOut,
                        "the body stopped coming, or came too slowly",
                    )
                } else {
                    err
                };
                if let Some(flag) = &self.done_reading {
                    flag.store(true, Ordering::Release);
                }
                self.failed = Some(err.kind());
                Err(err)
            }
        }
    }
}

impl<R> Drop for ChunkedBody<R>
where
    R: Read,
{
    fn drop(&mut self) {
        // rusk: see the type. Through a small buffer, and at the pace the
        // source keeps (see `PacedBody`): a body that stops, or goes
        // wrong, ends it.
        let mut buf = [0u8; DISCARD_BUFFER_BYTES];
        while !self.ended && self.failed.is_none() {
            let _ = self.read(&mut buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ChunkedBody;
    use crate::util::refined_tcp_stream::timed_out;
    use std::io::{Error as IoError, ErrorKind, Read, Result as IoResult};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    const BODY: &[u8] = b"5\r\nhello\r\n7;ext=1\r\n, world\r\n0\r\n\r\nGET /next";

    /// A connection that gives `data`, then waits its time out, as
    /// `RefinedTcpStream` does: the read fails as timed out and sets the
    /// connection's flag, every read after it ends at once.
    struct Stalls {
        data: std::io::Cursor<Vec<u8>>,
        flag: Arc<AtomicBool>,
    }

    impl Read for Stalls {
        fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
            if self.flag.load(Ordering::Acquire) {
                return Ok(0);
            }
            let read = self.data.read(buf)?;
            if read == 0 && !buf.is_empty() {
                self.flag.store(true, Ordering::Release);
                return Err(IoError::new(ErrorKind::WouldBlock, "timed out"));
            }
            Ok(read)
        }
    }

    fn stalls_after(data: &[u8]) -> (Stalls, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(false));
        let source = Stalls {
            data: std::io::Cursor::new(data.to_vec()),
            flag: flag.clone(),
        };
        (source, flag)
    }

    /// A whole body reads whole, whatever the size of the reads, and not a
    /// byte past its end.
    #[test]
    fn a_whole_body_reads_up_to_its_end() {
        for size in [1, 3, 64] {
            let mut source = &BODY[..];
            let mut body = ChunkedBody::new(&mut source, None);
            let mut read = Vec::new();
            let mut buf = vec![0; size];
            loop {
                let n = body.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                read.extend_from_slice(&buf[..n]);
            }
            assert_eq!(read, b"hello, world");
            assert_eq!(body.read(&mut buf).unwrap(), 0);
            drop(body);
            assert_eq!(source, b"GET /next");
        }
    }

    /// A body that stops anywhere — in a size, an extension, a chunk's
    /// data or any of its CRLFs — fails as timed out, however the decoder
    /// takes it; and ends the reading of the connection.
    #[test]
    fn a_body_that_stops_anywhere_times_out() {
        let end = BODY.len() - b"GET /next".len();
        for cut in 0..end {
            let (source, flag) = stalls_after(&BODY[..cut]);
            let mut body = ChunkedBody::new(source, Some(flag.clone()));
            let mut read = Vec::new();
            let err = body.read_to_end(&mut read).unwrap_err();
            assert!(timed_out(&err), "cut at {}: {:?}", cut, err);
            assert!(flag.load(Ordering::Acquire));
            let again = body.read(&mut [0; 8]).unwrap_err();
            assert!(timed_out(&again), "cut at {}: {:?}", cut, again);
        }
    }

    /// A body that ends inside a chunk — the client gave up — is not the
    /// body it meant.
    #[test]
    fn a_body_that_ends_inside_a_chunk_is_no_body() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut body = ChunkedBody::new(&b"5\r\nhel"[..], Some(flag.clone()));
        let mut read = Vec::new();
        let err = body.read_to_end(&mut read).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
        assert_eq!(read, b"hel");
        assert!(flag.load(Ordering::Acquire));
    }

    /// A body in the wrong format is a format error still, and ends the
    /// reading of the connection: where the next request starts is not
    /// known.
    #[test]
    fn a_wrong_body_is_a_format_error_and_ends_the_connection() {
        for wrong in [&b"zz\r\n"[..], b"5\r\nhelloXY", b"5\r\nhello\r\n0\r\nXY"] {
            let flag = Arc::new(AtomicBool::new(false));
            let mut body = ChunkedBody::new(wrong, Some(flag.clone()));
            let err = body.read_to_end(&mut Vec::new()).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{:?}", wrong);
            assert!(flag.load(Ordering::Acquire));
            let again = body.read(&mut [0; 8]).unwrap_err();
            assert_eq!(again.kind(), ErrorKind::InvalidInput);
        }
    }

    /// Review of R34: a chunk's size line is kept in memory until its end;
    /// one longer than any size and extensions come to is a format error,
    /// and the body is done with there — the rest of it is never read.
    #[test]
    fn a_size_line_without_end_is_a_format_error() {
        let mut long = "f".repeat(100_000).into_bytes();
        long.extend_from_slice(b"\r\nhello");
        let flag = Arc::new(AtomicBool::new(false));
        let mut source = &long[..];
        let mut body = ChunkedBody::new(&mut source, Some(flag.clone()));
        let err = body.read_to_end(&mut Vec::new()).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(flag.load(Ordering::Acquire));
        drop(body);
        assert!(source.len() > 90_000, "{} bytes left", source.len());

        // Size lines and extensions of any reasonable length are fine,
        // however small the reads.
        let mut with_extensions = Vec::new();
        for (size, data) in [(5usize, &b"hello"[..]), (7, b", world")] {
            let ext = ";".to_string() + &"x=y;".repeat(200);
            with_extensions.extend_from_slice(format!("{:0>32x}{}\r\n", size, ext).as_bytes());
            with_extensions.extend_from_slice(data);
            with_extensions.extend_from_slice(b"\r\n");
        }
        with_extensions.extend_from_slice(b"0\r\n\r\n");
        let mut body = ChunkedBody::new(&with_extensions[..], None);
        let mut read = Vec::new();
        let mut one = [0; 1];
        while body.read(&mut one).unwrap() == 1 {
            read.push(one[0]);
        }
        assert_eq!(read, b"hello, world");
    }

    /// The rest of a body left unread is thrown away when it goes, up to
    /// its end and no further; one that stops is thrown away as far as it
    /// came.
    #[test]
    fn an_unread_body_is_thrown_away_up_to_its_end() {
        let mut source = &BODY[..];
        let mut body = ChunkedBody::new(&mut source, None);
        let mut first = [0; 2];
        body.read_exact(&mut first).unwrap();
        drop(body);
        assert_eq!(source, b"GET /next");

        let (source, flag) = stalls_after(b"5\r\nhello\r\n");
        drop(ChunkedBody::new(source, Some(flag.clone())));
        assert!(flag.load(Ordering::Acquire));
    }
}
