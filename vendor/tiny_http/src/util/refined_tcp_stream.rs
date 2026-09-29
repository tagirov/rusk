use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::connection::Connection;
#[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
use crate::ssl::SslStream;

pub(crate) enum Stream {
    Http(Connection),
    #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
    Https(SslStream),
}

impl From<Connection> for Stream {
    fn from(tcp_stream: Connection) -> Self {
        Stream::Http(tcp_stream)
    }
}

impl Stream {
    /// rusk: a second handle on the connection. It used to be `Clone`,
    /// which unwrapped `try_clone`: out of file descriptors, the thread
    /// that accepts connections panicked, and the server went on without
    /// taking any.
    fn try_clone(&self) -> IoResult<Stream> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.try_clone().map(Stream::Http),
            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
            Stream::Https(ssl_stream) => Ok(Stream::Https(ssl_stream.clone())),
        }
    }

    fn secure(&self) -> bool {
        match self {
            Stream::Http(_) => false,
            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
            Stream::Https(_) => true,
        }
    }

    fn peer_addr(&mut self) -> IoResult<Option<SocketAddr>> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.peer_addr(),
            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
            Stream::Https(ssl_stream) => ssl_stream.peer_addr(),
        }
    }

    fn shutdown(&mut self, how: Shutdown) -> IoResult<()> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.shutdown(how),
            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
            Stream::Https(ssl_stream) => ssl_stream.shutdown(how),
        }
    }

    /// rusk: how long a read may wait for data (see `RefinedTcpStream`).
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> IoResult<()> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.set_read_timeout(timeout),
            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
            Stream::Https(ssl_stream) => ssl_stream.set_read_timeout(timeout),
        }
    }

    /// rusk: how long a write may wait for room (see `RefinedTcpStream`).
    fn set_write_timeout(&mut self, timeout: Option<Duration>) -> IoResult<()> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.set_write_timeout(timeout),
            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
            Stream::Https(ssl_stream) => ssl_stream.set_write_timeout(timeout),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.read(buf),
            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
            Stream::Https(ssl_stream) => ssl_stream.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.write(buf),
            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
            Stream::Https(ssl_stream) => ssl_stream.write(buf),
        }
    }

    fn flush(&mut self) -> IoResult<()> {
        match self {
            Stream::Http(tcp_stream) => tcp_stream.flush(),
            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
            Stream::Https(ssl_stream) => ssl_stream.flush(),
        }
    }
}

/// rusk: whether an I/O error is a read or write that waited its whole
/// timeout out (`WouldBlock` on unix, `TimedOut` on Windows and from a
/// deadline of ours).
pub(crate) fn timed_out(err: &IoError) -> bool {
    match err.kind() {
        ErrorKind::WouldBlock | ErrorKind::TimedOut => true,
        _ => false,
    }
}

/// rusk: the pace a transfer — a request's body, the answers of a
/// connection — has to keep (see `Limits::min_rate`): the time its reads
/// or writes spend waiting on the socket may come to one timeout, and a
/// second more for every `rate` bytes moved (by the writes that waited,
/// for an answer: see `RefinedTcpStream::write`). A client that keeps moving
/// at least that fast is never cut off, however long it takes; one that
/// sends a byte, or takes one, now and then — well within the timeout
/// each time — is, once it has waited its allowance.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pace {
    timeout: Duration,
    rate: u32,
    waited: Duration,
    moved: u64,
}

impl Pace {
    pub(crate) fn new(timeout: Duration, rate: u32) -> Pace {
        Pace {
            timeout,
            rate: rate.max(1),
            waited: Duration::from_secs(0),
            moved: 0,
        }
    }

    /// How much longer the transfer may wait (zero: it has been too slow).
    pub(crate) fn left(&self) -> Duration {
        let earned = Duration::from_millis(self.moved.saturating_mul(1000) / u64::from(self.rate));
        self.timeout
            .checked_add(earned)
            .unwrap_or(Duration::MAX)
            .saturating_sub(self.waited)
    }

    /// One read or write: how long it took, and what it moved.
    pub(crate) fn account(&mut self, took: Duration, moved: usize) {
        self.waited = self.waited.saturating_add(took);
        self.moved = self.moved.saturating_add(moved as u64);
    }
}

/// How long a socket waits on one call: the timeout, or less when the
/// deadline or the pace leaves less; never zero, which a socket refuses.
fn wait_within(timeout: Option<Duration>, left: Duration) -> Option<Duration> {
    let left = left.max(Duration::from_millis(1));
    Some(timeout.map_or(left, |timeout| timeout.min(left)))
}

/// One half of a connection: the one that reads, or the one that writes.
///
/// rusk: the halves keep the connection's limits (see `Limits`). The
/// socket waits `timeout` on any read or write. Under a `deadline` — a
/// request's head from its first byte, a request's body at its pace — no
/// read waits past it, however slowly the bytes come: the socket's timeout
/// is set to what the deadline leaves before every read. The writing half
/// keeps the pace of the answers (`pace`) the same way. A read or write
/// that waited its time out ends the reading of the connection, for both
/// halves (`done_reading`): every read from then on ends at once (EOF),
/// so that whoever holds a reader of it — the body of a request being
/// answered, the drain of what was left unread — is done with it, and the
/// connection's thread takes no further request from it, not even one it
/// has read ahead. A read timeout leaves the answer to go out; a write
/// timeout shuts the connection down both ways.
pub struct RefinedTcpStream {
    stream: Stream,
    close_read: bool,
    close_write: bool,
    timeout: Option<Duration>,
    deadline: Option<Instant>,
    pace: Option<Pace>,
    // What the socket waits right now, so that it is set only when it
    // changes.
    waits: Option<Duration>,
    done_reading: Arc<AtomicBool>,
}

impl RefinedTcpStream {
    /// rusk: `timeout` is what the socket waits on a read or write already
    /// (set by whoever accepted it), `None` for ever; `min_rate` is the
    /// pace the answers have to keep (see `Pace`), with a timeout only.
    pub(crate) fn new<S>(
        stream: S,
        timeout: Option<Duration>,
        min_rate: Option<u32>,
    ) -> IoResult<(RefinedTcpStream, RefinedTcpStream)>
    where
        S: Into<Stream>,
    {
        let stream: Stream = stream.into();

        let (read, write) = (stream.try_clone()?, stream);
        let done_reading = Arc::new(AtomicBool::new(false));

        let read = RefinedTcpStream {
            stream: read,
            close_read: true,
            close_write: false,
            timeout,
            deadline: None,
            pace: None,
            waits: timeout,
            done_reading: done_reading.clone(),
        };

        let write = RefinedTcpStream {
            stream: write,
            close_read: false,
            close_write: true,
            timeout,
            deadline: None,
            pace: timeout.and_then(|timeout| min_rate.map(|rate| Pace::new(timeout, rate))),
            waits: timeout,
            done_reading,
        };

        Ok((read, write))
    }

    /// Returns true if this struct wraps around a secure connection.
    #[inline]
    pub(crate) fn secure(&self) -> bool {
        self.stream.secure()
    }

    pub(crate) fn peer_addr(&mut self) -> IoResult<Option<SocketAddr>> {
        self.stream.peer_addr()
    }

    /// rusk: until when the reads may wait, all of them together (`None`:
    /// each its timeout).
    pub(crate) fn set_deadline(&mut self, deadline: Option<Instant>) {
        self.deadline = deadline;
    }

    /// rusk: the flag both halves share: nothing more is read from the
    /// connection (see the type).
    pub(crate) fn done_reading(&self) -> Arc<AtomicBool> {
        self.done_reading.clone()
    }

    /// rusk: no more reading from this connection.
    pub(crate) fn stop_reading(&mut self) {
        self.done_reading.store(true, Ordering::Release);
        self.stream.shutdown(Shutdown::Read).ok();
    }
}

impl Drop for RefinedTcpStream {
    fn drop(&mut self) {
        if self.close_read {
            self.stream.shutdown(Shutdown::Read).ok();
        }

        if self.close_write {
            self.stream.shutdown(Shutdown::Write).ok();
        }
    }
}

impl Read for RefinedTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        // rusk: see the type.
        if self.done_reading.load(Ordering::Acquire) {
            return Ok(0);
        }
        let waits = match self.deadline {
            None => self.timeout,
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left == Duration::from_secs(0) {
                    self.stop_reading();
                    return Err(IoError::new(ErrorKind::TimedOut, "the request took too long"));
                }
                wait_within(self.timeout, left)
            }
        };
        if waits != self.waits {
            self.stream.set_read_timeout(waits)?;
            self.waits = waits;
        }
        let result = self.stream.read(buf);
        if let Err(err) = &result {
            if timed_out(err) {
                self.stop_reading();
            }
        }
        result
    }
}

impl Write for RefinedTcpStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        // rusk: the answers keep their pace (see the type).
        let result = match self.pace {
            None => self.stream.write(buf),
            Some(mut pace) => {
                let left = pace.left();
                let result = if left == Duration::from_secs(0) {
                    Err(IoError::new(ErrorKind::TimedOut, "the answer is taken too slowly"))
                } else {
                    let waits = wait_within(self.timeout, left);
                    if waits != self.waits {
                        self.stream.set_write_timeout(waits)?;
                        self.waits = waits;
                    }
                    let started = Instant::now();
                    let result = self.stream.write(buf);
                    // Only a write that waited counts: one that went
                    // straight into the socket's buffer says nothing of
                    // how fast the client takes the answer, and a slow
                    // client would earn hours of waiting (a second per
                    // kilobyte) from the megabytes the buffers hold.
                    let took = started.elapsed();
                    if took >= Duration::from_millis(1) {
                        pace.account(took, *result.as_ref().unwrap_or(&0));
                    }
                    result
                };
                self.pace = Some(pace);
                result
            }
        };
        // rusk: a client that took nothing of the answer for the whole
        // timeout, or too little of it for too long, gets no more of it,
        // and the connection ends (see the type).
        if let Err(err) = &result {
            if timed_out(err) {
                self.done_reading.store(true, Ordering::Release);
                self.stream.shutdown(Shutdown::Both).ok();
            }
        }
        result
    }

    fn flush(&mut self) -> IoResult<()> {
        self.stream.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::Pace;
    use std::time::Duration;

    /// rusk: a transfer may wait one timeout, and a second more for every
    /// `rate` bytes it has moved.
    #[test]
    fn a_pace_earns_waiting_by_moving() {
        let mut pace = Pace::new(Duration::from_secs(10), 1024);
        assert_eq!(pace.left(), Duration::from_secs(10));
        pace.account(Duration::from_secs(4), 0);
        assert_eq!(pace.left(), Duration::from_secs(6));
        pace.account(Duration::from_secs(1), 3 * 1024);
        assert_eq!(pace.left(), Duration::from_secs(8));
        pace.account(Duration::from_secs(20), 0);
        assert_eq!(pace.left(), Duration::from_secs(0));
        // Nothing overflows, whatever the numbers.
        let mut pace = Pace::new(Duration::MAX, 1);
        pace.account(Duration::MAX, usize::MAX);
        pace.account(Duration::from_secs(1), usize::MAX);
        let _ = pace.left();
    }
}
