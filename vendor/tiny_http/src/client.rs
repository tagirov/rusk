use ascii::AsciiString;

use std::io::Error as IoError;
use std::io::Result as IoResult;
use std::io::{BufReader, BufWriter, ErrorKind, Read, Write};

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::common::{HTTPVersion, Header, Method};
use crate::util::refined_tcp_stream::{timed_out, Pace};
use crate::util::RefinedTcpStream;
use crate::util::{SequentialReader, SequentialReaderBuilder, SequentialWriterBuilder};
use crate::Request;

/// A ClientConnection is an object that will store a socket to a client
/// and return Request objects.
pub struct ClientConnection {
    // address of the client
    remote_addr: IoResult<Option<SocketAddr>>,

    // sequence of Readers to the stream, so that the data is not read in
    //  the wrong order
    source: SequentialReaderBuilder<BufReader<RefinedTcpStream>>,

    // sequence of Writers to the stream, to avoid writing response #2 before
    //  response #1
    sink: SequentialWriterBuilder<BufWriter<RefinedTcpStream>>,

    // Reader to read the next header from
    next_header_source: SequentialReader<BufReader<RefinedTcpStream>>,

    // set to true if we know that the previous request is the last one
    no_more_requests: bool,

    // true if the connection goes through SSL
    secure: bool,

    // rusk: how long a request's head may take from its first byte (see
    // `read`); None for ever
    head_timeout: Option<Duration>,

    // rusk: the pace a request's body has to keep (see `PacedBody`)
    body_pace: Option<Pace>,

    // rusk: set once a read or write of the connection waited its time
    // out: no further request is taken from it (see `RefinedTcpStream`)
    done_reading: Arc<AtomicBool>,
}

/// Error that can happen when reading a request.
#[derive(Debug)]
enum ReadError {
    WrongRequestLine,
    WrongHeader(HTTPVersion),
    /// the client sent an unrecognized `Expect` header
    ExpectationFailed(HTTPVersion),
    ReadIoError(IoError),
    /// rusk: nothing came for as long as the connection may stay idle
    Idle,
}

impl ClientConnection {
    /// Creates a new `ClientConnection` that takes ownership of the `TcpStream`.
    ///
    /// rusk: `head_timeout` is how long a request's head may take from
    /// its first byte, `min_rate` the pace its body has to keep (see
    /// `Limits`).
    pub fn new(
        write_socket: RefinedTcpStream,
        mut read_socket: RefinedTcpStream,
        head_timeout: Option<Duration>,
        min_rate: Option<u32>,
    ) -> ClientConnection {
        let remote_addr = read_socket.peer_addr();
        let secure = read_socket.secure();
        let done_reading = read_socket.done_reading();

        let mut source = SequentialReaderBuilder::new(BufReader::with_capacity(1024, read_socket));
        let first_header = source.next().unwrap();

        ClientConnection {
            source,
            sink: SequentialWriterBuilder::new(BufWriter::with_capacity(1024, write_socket)),
            remote_addr,
            next_header_source: first_header,
            no_more_requests: false,
            secure,
            head_timeout,
            body_pace: head_timeout.and_then(|timeout| min_rate.map(|rate| Pace::new(timeout, rate))),
            done_reading,
        }
    }

    /// true if the connection is HTTPS
    pub fn secure(&self) -> bool {
        self.secure
    }

    /// rusk: until when the reads of the head may wait, all of them
    /// together (see `read`). The reader is this connection's by now: its
    /// first byte has been read.
    fn set_deadline(&mut self, deadline: Option<Instant>) {
        if let Some(reader) = self.next_header_source.reader_mut() {
            reader.get_mut().set_deadline(deadline);
        }
    }

    /// Reads the next line from self.next_header_source.
    ///
    /// Reads until `CRLF` is reached. The next read will start
    ///  at the first byte of the new line.
    ///
    /// rusk: `first` is the line's first byte when it has been read
    /// already (see `read`).
    fn read_next_line(&mut self, first: Option<u8>) -> IoResult<AsciiString> {
        let mut buf = Vec::new();
        let mut prev_byte_was_cr = false;

        if let Some(byte) = first {
            prev_byte_was_cr = byte == b'\r';
            buf.push(byte);
        }

        loop {
            let byte = self.next_header_source.by_ref().bytes().next();

            let byte = match byte {
                Some(b) => b?,
                None => return Err(IoError::new(ErrorKind::ConnectionAborted, "Unexpected EOF")),
            };

            if byte == b'\n' && prev_byte_was_cr {
                buf.pop(); // removing the '\r'
                return AsciiString::from_ascii(buf)
                    .map_err(|_| IoError::new(ErrorKind::InvalidInput, "Header is not in ASCII"));
            }

            prev_byte_was_cr = byte == b'\r';

            buf.push(byte);
        }
    }

    /// Reads a request from the stream.
    /// Blocks until the header has been read.
    ///
    /// rusk: the request's first byte is waited for as long as the
    /// connection may stay idle — the socket's own timeout; nothing by
    /// then is `ReadError::Idle`. From that byte on, the head (request line
    /// and headers) has `head_timeout`: a deadline every read of it keeps,
    /// however slowly the bytes come; past it, the read fails as timed out.
    /// The body, read by whoever answers the request, has the socket's
    /// timeout again on every read, and a pace to keep (`PacedBody`).
    /// Nothing more is read once a read or write of the connection has
    /// waited its time out, or a body of it went wrong (`ChunkedBody`), not
    /// even a request read ahead into the buffer.
    fn read(&mut self) -> Result<Request, ReadError> {
        let cut_off = || {
            ReadError::ReadIoError(IoError::new(
                ErrorKind::ConnectionAborted,
                "the connection was cut off",
            ))
        };
        if self.done_reading.load(Ordering::Acquire) {
            return Err(cut_off());
        }
        let first = match self.next_header_source.by_ref().bytes().next() {
            Some(Ok(byte)) => byte,
            Some(Err(e)) if timed_out(&e) => return Err(ReadError::Idle),
            Some(Err(e)) => return Err(ReadError::ReadIoError(e)),
            None => {
                return Err(ReadError::ReadIoError(IoError::new(
                    ErrorKind::ConnectionAborted,
                    "Unexpected EOF",
                )))
            }
        };
        // rusk: the first byte comes with the head's turn, once the body
        // before it is done with — and that may have ended the connection
        // meanwhile (it stopped, or went wrong, while it was thrown away),
        // after the check above: what was read ahead into the buffer is
        // not a request then, whatever it looks like (R34: a head read
        // ahead was taken, or answered 400, then)
        if self.done_reading.load(Ordering::Acquire) {
            return Err(cut_off());
        }
        // A timeout too large to add to now is no deadline (review of R33:
        // `web_timeout = 18446744073709551615` panicked every connection).
        self.set_deadline(
            self.head_timeout
                .and_then(|timeout| Instant::now().checked_add(timeout)),
        );
        let head = self.read_head(first);
        self.set_deadline(None);
        let (method, path, version, headers) = head?;
        // rusk: and once more for a write that timed out while the head
        // was read (an answer sent ahead, without a limit on connections)
        if self.done_reading.load(Ordering::Acquire) {
            return Err(cut_off());
        }

        // building the writer for the request
        let writer = self.sink.next().unwrap();

        // follow-up for next potential request
        let mut data_source = self.source.next().unwrap();
        std::mem::swap(&mut self.next_header_source, &mut data_source);
        let data_source = PacedBody {
            source: data_source,
            pace: self.body_pace,
        };

        // building the next reader
        let request = crate::request::new_request(
            self.secure,
            method,
            path,
            version.clone(),
            headers,
            *self.remote_addr.as_ref().unwrap(),
            data_source,
            writer,
            Some(self.done_reading.clone()),
        )
        .map_err(|e| {
            use crate::request;
            match e {
                request::RequestCreationError::CreationIoError(e) => ReadError::ReadIoError(e),
                request::RequestCreationError::ExpectationFailed => {
                    ReadError::ExpectationFailed(version)
                }
            }
        })?;

        // return the request
        Ok(request)
    }

    /// rusk: the request line and the headers, the first byte of the
    /// request line read already (see `read`).
    #[allow(clippy::type_complexity)]
    fn read_head(
        &mut self,
        first: u8,
    ) -> Result<(Method, String, HTTPVersion, Vec<Header>), ReadError> {
        // reading the request line
        let (method, path, version) = {
            let line = self
                .read_next_line(Some(first))
                .map_err(ReadError::ReadIoError)?;

            parse_request_line(
                line.as_str().trim(), // TODO: remove this conversion
            )?
        };

        // getting all headers
        let headers = {
            let mut headers = Vec::new();
            loop {
                let line = self.read_next_line(None).map_err(ReadError::ReadIoError)?;

                if line.is_empty() {
                    break;
                };
                headers.push(match FromStr::from_str(line.as_str().trim()) {
                    // TODO: remove this conversion
                    Ok(h) => h,
                    _ => return Err(ReadError::WrongHeader(version)),
                });
            }

            headers
        };

        Ok((method, path, version, headers))
    }
}

impl Iterator for ClientConnection {
    type Item = Request;

    /// Blocks until the next Request is available.
    /// Returns None when no new Requests will come from the client.
    fn next(&mut self) -> Option<Request> {
        use crate::{Response, StatusCode};

        // the client sent a "connection: close" header in this previous request
        //  or is using HTTP 1.0, meaning that no new request will come
        if self.no_more_requests {
            return None;
        }

        // rusk: the answers after which the connection ends say so
        // (`Response::closing`)
        loop {
            let rq = match self.read() {
                Err(ReadError::WrongRequestLine) => {
                    let writer = self.sink.next().unwrap();
                    let response = Response::new_empty(StatusCode(400)).closing();
                    response
                        .raw_print(writer, HTTPVersion(1, 1), &[], false, None)
                        .ok();
                    return None; // we don't know where the next request would start,
                                 // se we have to close
                }

                Err(ReadError::WrongHeader(ver)) => {
                    let writer = self.sink.next().unwrap();
                    let response = Response::new_empty(StatusCode(400)).closing();
                    response.raw_print(writer, ver, &[], false, None).ok();
                    return None; // we don't know where the next request would start,
                                 // se we have to close
                }

                Err(ReadError::ReadIoError(ref err)) if timed_out(err) => {
                    // request timeout
                    // rusk: the head took longer than its deadline, or one
                    // of its reads waited the socket's timeout out (see
                    // `read`); written out at once: the connection ends here
                    let mut writer = self.sink.next().unwrap();
                    let response = Response::new_empty(StatusCode(408)).closing();
                    response
                        .raw_print(writer.by_ref(), HTTPVersion(1, 1), &[], false, None)
                        .ok();
                    writer.flush().ok();
                    return None; // closing the connection
                }

                // rusk: nothing came for as long as the connection may stay
                // idle: closed without a word, as browsers expect of a
                // keep-alive connection
                Err(ReadError::Idle) => return None,

                Err(ReadError::ExpectationFailed(ver)) => {
                    let writer = self.sink.next().unwrap();
                    let response = Response::new_empty(StatusCode(417)).closing();
                    response.raw_print(writer, ver, &[], true, None).ok();
                    return None; // TODO: should be recoverable, but needs handling in case of body
                }

                Err(ReadError::ReadIoError(_)) => return None,

                Ok(rq) => rq,
            };

            // checking HTTP version
            if *rq.http_version() > (1, 1) {
                let writer = self.sink.next().unwrap();
                let response = Response::from_string(
                    "This server only supports HTTP versions 1.0 and 1.1".to_owned(),
                )
                .with_status_code(StatusCode(505));
                response
                    .raw_print(writer, HTTPVersion(1, 1), &[], false, None)
                    .ok();
                continue;
            }

            // updating the status of the connection
            let connection_header = rq
                .headers()
                .iter()
                .find(|h| h.field.equiv("Connection"))
                .map(|h| h.value.as_str());

            let lowercase = connection_header.map(|h| h.to_ascii_lowercase());

            match lowercase {
                Some(ref val) if val.contains("close") => self.no_more_requests = true,
                Some(ref val) if val.contains("upgrade") => self.no_more_requests = true,
                Some(ref val)
                    if !val.contains("keep-alive") && *rq.http_version() == HTTPVersion(1, 0) =>
                {
                    self.no_more_requests = true
                }
                None if *rq.http_version() == HTTPVersion(1, 0) => self.no_more_requests = true,
                _ => (),
            };

            // returning the request
            // rusk: the last one on the connection says so in its answer
            return Some(rq.last_on_connection(self.no_more_requests));
        }
    }
}

/// rusk: a request's body, read at its pace (see `Pace`): every read may
/// wait what the pace has left, no longer — past that, it fails as timed
/// out and the connection's reading ends. The pace counts the time spent
/// in reads, not between them: a body read late (`Expect: 100-continue`,
/// a server busy elsewhere) loses nothing by it. What the request leaves
/// unread and tiny_http throws away (`EqualReader`) keeps the same pace.
/// The deadline goes with the body: it is cleared before the reader passes
/// on to the next request's head.
struct PacedBody {
    source: SequentialReader<BufReader<RefinedTcpStream>>,
    pace: Option<Pace>,
}

impl Read for PacedBody {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        let mut pace = match self.pace {
            None => return self.source.read(buf),
            Some(pace) => pace,
        };
        let started = Instant::now();
        let deadline = started.checked_add(pace.left());
        if let Some(reader) = self.source.reader_mut() {
            reader.get_mut().set_deadline(deadline);
        }
        let result = self.source.read(buf);
        pace.account(started.elapsed(), *result.as_ref().unwrap_or(&0));
        self.pace = Some(pace);
        result
    }
}

impl Drop for PacedBody {
    fn drop(&mut self) {
        if let Some(reader) = self.source.reader_mut() {
            reader.get_mut().set_deadline(None);
        }
    }
}

/// Parses a "HTTP/1.1" string.
fn parse_http_version(version: &str) -> Result<HTTPVersion, ReadError> {
    let (major, minor) = match version {
        "HTTP/0.9" => (0, 9),
        "HTTP/1.0" => (1, 0),
        "HTTP/1.1" => (1, 1),
        "HTTP/2.0" => (2, 0),
        "HTTP/3.0" => (3, 0),
        _ => return Err(ReadError::WrongRequestLine),
    };

    Ok(HTTPVersion(major, minor))
}

/// Parses the request line of the request.
/// eg. GET / HTTP/1.1
fn parse_request_line(line: &str) -> Result<(Method, String, HTTPVersion), ReadError> {
    let mut parts = line.split(' ');

    let method = parts.next().and_then(|w| w.parse().ok());
    let path = parts.next().map(ToOwned::to_owned);
    let version = parts.next().and_then(|w| parse_http_version(w).ok());

    method
        .and_then(|method| Some((method, path?, version?)))
        .ok_or(ReadError::WrongRequestLine)
}

#[cfg(test)]
mod test {
    #[test]
    fn test_parse_request_line() {
        let (method, path, ver) = super::parse_request_line("GET /hello HTTP/1.1").unwrap();

        assert!(method == crate::Method::Get);
        assert!(path == "/hello");
        assert!(ver == crate::common::HTTPVersion(1, 1));

        assert!(super::parse_request_line("GET /hello").is_err());
        assert!(super::parse_request_line("qsd qsd qsd").is_err());
    }
}
