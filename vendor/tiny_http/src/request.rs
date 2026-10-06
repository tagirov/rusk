use std::io::Error as IoError;
use std::io::{self, Cursor, ErrorKind, Read, Write};

use std::fmt;
use std::net::SocketAddr;
use std::str::FromStr;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::util::{ChunkedBody, EqualReader, FusedReader};
use crate::{HTTPVersion, Header, Method, Response, StatusCode};

/// Represents an HTTP request made by a client.
///
/// A `Request` object is what is produced by the server, and is your what
/// your code must analyse and answer.
///
/// This object implements the `Send` trait, therefore you can dispatch your requests to
/// worker threads.
///
/// # Pipelining
///
/// If a client sends multiple requests in a row (without waiting for the response), then you will
/// get multiple `Request` objects simultaneously. This is called *requests pipelining*.
/// Tiny-http automatically reorders the responses so that you don't need to worry about the order
/// in which you call `respond` or `into_writer`.
///
/// This mechanic is disabled if:
///
///  - The body of a request is large enough (handling requires pipelining requires storing the
///    body of the request in a buffer ; if the body is too big, tiny-http will avoid doing that)
///  - A request sends a `Expect: 100-continue` header (which means that the client waits to
///    know whether its body will be processed before sending it)
///  - A request sends a `Connection: close` header or `Connection: upgrade` header (used for
///    websockets), which indicates that this is the last request that will be received on this
///    connection
///
/// # Automatic cleanup
///
/// If a `Request` object is destroyed without `into_writer` or `respond` being called,
/// an empty response with a 500 status code (internal server error) will automatically be
/// sent back to the client.
/// This means that if your code fails during the handling of a request, this "internal server
/// error" response will automatically be sent during the stack unwinding.
///
/// # Testing
///
/// If you want to build fake requests to test your server, use [`TestRequest`](crate::test::TestRequest).
pub struct Request {
    // where to read the body from
    data_reader: Option<Box<dyn Read + Send + 'static>>,

    // if this writer is empty, then the request has been answered
    response_writer: Option<Box<dyn Write + Send + 'static>>,

    remote_addr: Option<SocketAddr>,

    // true if HTTPS, false if HTTP
    secure: bool,

    method: Method,

    path: String,

    http_version: HTTPVersion,

    headers: Vec<Header>,

    body_length: Option<usize>,

    // true if a `100 Continue` response must be sent when `as_reader()` is called
    must_send_continue: bool,

    // If Some, a message must be sent after responding
    notify_when_responded: Option<Sender<()>>,

    // rusk: the connection's flag, set once nothing more is read from it
    // (see `RefinedTcpStream`, `ChunkedBody`); None for a request of no
    // connection (`TestRequest`)
    done_reading: Option<Arc<AtomicBool>>,

    // rusk: no request follows this one on its connection (see
    // `ClientConnection::next`)
    last: bool,

    // rusk: which request of its connection this is, and how many the
    // connection has taken so far (see `respond_impl`); None for a request
    // of no connection
    ordinal: usize,
    requests_taken: Option<Arc<AtomicUsize>>,

    // rusk: whether the request has a body to read — a `Content-Length`
    // above zero, or a `Transfer-Encoding` (see `respond_impl`)
    has_body: bool,

    // rusk: a `Connection: upgrade` request without a body keeps the
    // connection's reader whole for `upgrade()`; `as_reader()` gives it
    // `no_body` (see `new_request`)
    upgrade_handshake: bool,
    no_body: io::Empty,
}

struct NotifyOnDrop<R> {
    sender: Sender<()>,
    inner: R,
}

impl<R: Read> Read for NotifyOnDrop<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}
impl<R: Write> Write for NotifyOnDrop<R> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<R> Drop for NotifyOnDrop<R> {
    fn drop(&mut self) {
        self.sender.send(()).unwrap();
    }
}

/// Error that can happen when building a `Request` object.
#[derive(Debug)]
pub enum RequestCreationError {
    /// The client sent an `Expect` header that was not recognized by tiny-http.
    ExpectationFailed,

    /// rusk: the `Content-Length` is no number, or two of them differ
    BadContentLength,

    /// Error while reading data from the socket during the creation of the `Request`.
    CreationIoError(IoError),
}

impl From<IoError> for RequestCreationError {
    fn from(err: IoError) -> RequestCreationError {
        RequestCreationError::CreationIoError(err)
    }
}

/// Builds a new request.
///
/// After the request line and headers have been read from the socket, a new `Request` object
/// is built.
///
/// You must pass a `Read` that will allow the `Request` object to read from the incoming data.
/// It is the responsibility of the `Request` to read only the data of the request and not further.
///
/// The `Write` object will be used by the `Request` to write the response.
///
/// rusk: `done_reading` is the flag of the connection the request came
/// by (see `RefinedTcpStream`), `None` for a request of none.
#[allow(clippy::too_many_arguments)]
pub fn new_request<R, W>(
    secure: bool,
    method: Method,
    path: String,
    version: HTTPVersion,
    headers: Vec<Header>,
    remote_addr: Option<SocketAddr>,
    mut source_data: R,
    writer: W,
    done_reading: Option<Arc<AtomicBool>>,
) -> Result<Request, RequestCreationError>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    // finding the transfer-encoding header
    let transfer_encoding = headers
        .iter()
        .find(|h: &&Header| h.field.equiv("Transfer-Encoding"))
        .map(|h| h.value.clone());

    // finding the content-length header
    let content_length = if transfer_encoding.is_some() {
        // if transfer-encoding is specified, the Content-Length
        // header must be ignored (RFC2616 #4.4)
        None
    } else {
        // rusk: a `Content-Length` that is no number (digits only), or two
        // that differ, is a bad request (RFC 9112 §6.3), not a request
        // without a body: the crate as published took it for one, and read
        // the body as the next request (review of R35)
        let mut length = None;
        for header in headers.iter().filter(|h| h.field.equiv("Content-Length")) {
            let value = header.value.as_str().trim();
            let value: usize = match value.bytes().all(|b| b.is_ascii_digit()) {
                true => value.parse().map_err(|_| RequestCreationError::BadContentLength)?,
                false => return Err(RequestCreationError::BadContentLength),
            };
            if length.is_some_and(|known| known != value) {
                return Err(RequestCreationError::BadContentLength);
            }
            length = Some(value);
        }
        length
    };

    // true if the client sent a `Expect: 100-continue` header
    // rusk: an HTTP/1.0 client's expectation is ignored (RFC 9110
    // §10.1.1): it sends its body whatever comes, and a `100 Continue` is
    // nothing it knows (review of R35: it was sent one)
    let expects_continue = if version < HTTPVersion(1, 1) {
        false
    } else {
        match headers
            .iter()
            .find(|h: &&Header| h.field.equiv("Expect"))
            .map(|h| h.value.as_str())
        {
            None => false,
            Some(v) if v.eq_ignore_ascii_case("100-continue") => true,
            _ => return Err(RequestCreationError::ExpectationFailed),
        }
    };

    // true if the client sent a `Connection: upgrade` header
    let connection_upgrade = {
        match headers
            .iter()
            .find(|h: &&Header| h.field.equiv("Connection"))
            .map(|h| h.value.as_str())
        {
            Some(v) if v.to_ascii_lowercase().contains("upgrade") => true,
            _ => false,
        }
    };

    // rusk: whether there is a body to read
    let has_body = transfer_encoding.is_some() || content_length.is_some_and(|len| len > 0);

    // rusk: a `Connection: upgrade` request's body is framed by its headers
    // like any other: an upgrade is a proposal the server may not take up
    // (curl proposes h2c on every request with `--http2`), and the answer
    // to it is a usual one — after which the connection ends all the same
    // (`ClientConnection::next`): whether the request is upgraded is not
    // known when the next one would be read. The crate as published kept
    // the connection's reader whole for every such request, for
    // `upgrade()`, so that `as_reader()` read on past the body, to the
    // timeout (a `POST` with `Connection: upgrade` was 408 with its body
    // whole) — or for ever. The reader is kept whole for a request without
    // a body (the handshake) only, where `as_reader()` has nothing to give
    // anyway; `upgrade()` on a request with a body hands over a stream that
    // reads the body and no more.
    let upgrade_handshake = connection_upgrade && !has_body;

    // we wrap `source_data` around a reading whose nature depends on the transfer-encoding and
    // content-length headers
    let reader = if upgrade_handshake {
        // the handshake of an upgrade: keeping the whole reader
        Box::new(source_data) as Box<dyn Read + Send + 'static>
    } else if let Some(content_length) = content_length {
        if content_length == 0 {
            Box::new(io::empty()) as Box<dyn Read + Send + 'static>
        } else if content_length <= 1024 && !expects_continue {
            // if the content-length is small enough, we just read everything into a buffer

            let mut buffer = vec![0; content_length];
            let mut offset = 0;

            while offset != content_length {
                let read = source_data.read(&mut buffer[offset..])?;
                if read == 0 {
                    // the socket returned EOF, but we were before the expected content-length
                    // aborting
                    let info = "Connection has been closed before we received enough data";
                    let err = IoError::new(ErrorKind::ConnectionAborted, info);
                    return Err(RequestCreationError::CreationIoError(err));
                }

                offset += read;
            }

            Box::new(Cursor::new(buffer)) as Box<dyn Read + Send + 'static>
        } else {
            let (data_reader, _) = EqualReader::new(source_data, content_length); // TODO:
            Box::new(FusedReader::new(data_reader)) as Box<dyn Read + Send + 'static>
        }
    } else if transfer_encoding.is_some() {
        // if a transfer-encoding was specified, then "chunked" is ALWAYS applied
        // over the message (RFC2616 #3.6)
        // rusk: the decoder, with what it gets wrong put right (see
        // `ChunkedBody`)
        Box::new(ChunkedBody::new(source_data, done_reading.clone()))
            as Box<dyn Read + Send + 'static>
    } else {
        // if we have neither a Content-Length nor a Transfer-Encoding,
        // assuming that we have no data
        // TODO: could also be multipart/byteranges
        Box::new(io::empty()) as Box<dyn Read + Send + 'static>
    };

    Ok(Request {
        data_reader: Some(reader),
        response_writer: Some(Box::new(writer) as Box<dyn Write + Send + 'static>),
        remote_addr,
        secure,
        method,
        path,
        http_version: version,
        headers,
        body_length: content_length,
        must_send_continue: expects_continue,
        notify_when_responded: None,
        done_reading,
        last: false,
        ordinal: 0,
        requests_taken: None,
        has_body,
        upgrade_handshake,
        no_body: io::empty(),
    })
}

impl Request {
    /// Returns true if the request was made through HTTPS.
    #[inline]
    pub fn secure(&self) -> bool {
        self.secure
    }

    /// Returns the method requested by the client (eg. `GET`, `POST`, etc.).
    #[inline]
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the resource requested by the client.
    #[inline]
    pub fn url(&self) -> &str {
        &self.path
    }

    /// Returns a list of all headers sent by the client.
    #[inline]
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }

    /// Returns the HTTP version of the request.
    #[inline]
    pub fn http_version(&self) -> &HTTPVersion {
        &self.http_version
    }

    /// Returns the length of the body in bytes.
    ///
    /// Returns `None` if the length is unknown.
    #[inline]
    pub fn body_length(&self) -> Option<usize> {
        self.body_length
    }

    /// Returns the address of the client that sent this request.
    ///
    /// The address is always `Some` for TCP listeners, but always `None` for UNIX listeners
    /// (as the remote address of a UNIX client is almost always unnamed).
    ///
    /// Note that this is gathered from the socket. If you receive the request from a proxy,
    /// this function will return the address of the proxy and not the address of the actual
    /// user.
    #[inline]
    pub fn remote_addr(&self) -> Option<&SocketAddr> {
        self.remote_addr.as_ref()
    }

    /// Sends a response with a `Connection: upgrade` header, then turns the `Request` into a `Stream`.
    ///
    /// The main purpose of this function is to support websockets.
    /// If you detect that the request wants to use some kind of protocol upgrade, you can
    ///  call this function to obtain full control of the socket stream.
    ///
    /// If you call this on a non-websocket request, tiny-http will wait until this `Stream` object
    ///  is destroyed before continuing to read or write on the socket. Therefore you should always
    ///  destroy it as soon as possible.
    pub fn upgrade<R: Read>(
        mut self,
        protocol: &str,
        response: Response<R>,
    ) -> Box<dyn ReadWrite + Send> {
        use crate::util::CustomStream;

        response
            .raw_print(
                self.response_writer.as_mut().unwrap().by_ref(),
                self.http_version.clone(),
                &self.headers,
                false,
                Some(protocol),
            )
            .ok(); // TODO: unused result

        self.response_writer.as_mut().unwrap().flush().ok(); // TODO: unused result

        let stream = CustomStream::new(self.extract_reader_impl(), self.extract_writer_impl());
        if let Some(sender) = self.notify_when_responded.take() {
            let stream = NotifyOnDrop {
                sender,
                inner: stream,
            };
            Box::new(stream) as Box<dyn ReadWrite + Send>
        } else {
            Box::new(stream) as Box<dyn ReadWrite + Send>
        }
    }

    /// Allows to read the body of the request.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # extern crate rustc_serialize;
    /// # extern crate tiny_http;
    /// # use rustc_serialize::json::Json;
    /// # use std::io::Read;
    /// # fn get_content_type(_: &tiny_http::Request) -> &'static str { "" }
    /// # fn main() {
    /// # let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
    /// let mut request = server.recv().unwrap();
    ///
    /// if get_content_type(&request) == "application/json" {
    ///     let mut content = String::new();
    ///     request.as_reader().read_to_string(&mut content).unwrap();
    ///     let json: Json = content.parse().unwrap();
    /// }
    /// # }
    /// ```
    ///
    /// If the client sent a `Expect: 100-continue` header with the request, calling this
    ///  function will send back a `100 Continue` response.
    #[inline]
    pub fn as_reader(&mut self) -> &mut dyn Read {
        if self.must_send_continue {
            let msg = Response::new_empty(StatusCode(100));
            msg.raw_print(
                self.response_writer.as_mut().unwrap().by_ref(),
                self.http_version.clone(),
                &self.headers,
                true,
                None,
            )
            .ok();
            self.response_writer.as_mut().unwrap().flush().ok();
            self.must_send_continue = false;
        }

        // rusk: see `new_request`
        if self.upgrade_handshake {
            return &mut self.no_body;
        }

        self.data_reader.as_mut().unwrap()
    }

    /// Turns the `Request` into a writer.
    ///
    /// The writer has a raw access to the stream to the user.
    /// This function is useful for things like CGI.
    ///
    /// Note that the destruction of the `Writer` object may trigger
    /// some events. For exemple if a client has sent multiple requests and the requests
    /// have been processed in parallel, the destruction of a writer will trigger
    /// the writing of the next response.
    /// Therefore you should always destroy the `Writer` as soon as possible.
    #[inline]
    pub fn into_writer(mut self) -> Box<dyn Write + Send + 'static> {
        let writer = self.extract_writer_impl();
        if let Some(sender) = self.notify_when_responded.take() {
            let writer = NotifyOnDrop {
                sender,
                inner: writer,
            };
            Box::new(writer) as Box<dyn Write + Send + 'static>
        } else {
            writer
        }
    }

    /// Extract the response `Writer` object from the Request, dropping this `Writer` has the same side effects
    /// as the object returned by `into_writer` above.
    ///
    /// This may only be called once on a single request.
    fn extract_writer_impl(&mut self) -> Box<dyn Write + Send + 'static> {
        use std::mem;

        assert!(self.response_writer.is_some());

        let mut writer = None;
        mem::swap(&mut self.response_writer, &mut writer);
        writer.unwrap()
    }

    /// Extract the body `Reader` object from the Request.
    ///
    /// This may only be called once on a single request.
    fn extract_reader_impl(&mut self) -> Box<dyn Read + Send + 'static> {
        use std::mem;

        assert!(self.data_reader.is_some());

        let mut reader = None;
        mem::swap(&mut self.data_reader, &mut reader);
        reader.unwrap()
    }

    /// Sends a response to this request.
    #[inline]
    pub fn respond<R>(mut self, response: Response<R>) -> Result<(), IoError>
    where
        R: Read,
    {
        let res = self.respond_impl(response);
        if let Some(sender) = self.notify_when_responded.take() {
            sender.send(()).unwrap();
        }
        res
    }

    fn respond_impl<R>(&mut self, response: Response<R>) -> Result<(), IoError>
    where
        R: Read,
    {
        let mut writer = self.extract_writer_impl();

        let do_not_send_body = self.method == Method::Head;

        // rusk: a client waiting to be told to send its body (`Expect:
        // 100-continue`) that gets a final answer instead is not going to
        // send it (RFC 9110 §10.1.1): the answer says that the connection
        // ends, and nothing more is read from it — the rest of the body used
        // to be waited for when the request went, a timeout long (for ever
        // without one), with the client waiting for the 100 all the while.
        let waits_to_send = self.must_send_continue && self.has_body;
        if waits_to_send {
            if let Some(flag) = &self.done_reading {
                flag.store(true, Ordering::Release);
            }
        }

        // rusk: an answer after which the connection ends says so
        // (RFC 9112 §9.6): the client asked for that, or the connection
        // was cut off, or its reading had to end, while the request was
        // handled — a body that stopped coming (the 408 of whoever
        // answers), or went wrong — and no request has been taken from the
        // connection since: one taken ahead (pipelining) is answered after
        // this one, and the last of them says so (review of R34: an answer
        // said close for a later request's body). An answer on an HTTP/1.0
        // connection that goes on says so too (`Connection: keep-alive`):
        // such a client takes the answer for the last without it.
        let cut_off = self
            .done_reading
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire));
        let last_taken = self
            .requests_taken
            .as_ref()
            .is_none_or(|taken| taken.load(Ordering::Acquire) == self.ordinal);
        let ends = self.last || waits_to_send || (cut_off && last_taken);
        let response = if ends {
            response.closing()
        } else if self.http_version == HTTPVersion(1, 0) {
            response.keeping_alive()
        } else {
            response
        };

        Self::ignore_client_closing_errors(response.raw_print(
            writer.by_ref(),
            self.http_version.clone(),
            &self.headers,
            do_not_send_body,
            None,
        ))?;

        Self::ignore_client_closing_errors(writer.flush())
    }

    fn ignore_client_closing_errors(result: io::Result<()>) -> io::Result<()> {
        result.or_else(|err| match err.kind() {
            ErrorKind::BrokenPipe => Ok(()),
            ErrorKind::ConnectionAborted => Ok(()),
            ErrorKind::ConnectionRefused => Ok(()),
            ErrorKind::ConnectionReset => Ok(()),
            _ => Err(err),
        })
    }

    pub(crate) fn with_notify_sender(mut self, sender: Sender<()>) -> Self {
        self.notify_when_responded = Some(sender);
        self
    }

    /// rusk: the request's place on its connection — the `ordinal`th taken
    /// from it, `taken` counting them — and whether no request follows it
    /// (the client said `Connection: close` or `Connection: upgrade`, or is
    /// HTTP/1.0 without keep-alive): its answer says what becomes of the
    /// connection (see `respond_impl`).
    pub(crate) fn on_connection(
        mut self,
        ordinal: usize,
        taken: Arc<AtomicUsize>,
        last: bool,
    ) -> Self {
        self.ordinal = ordinal;
        self.requests_taken = Some(taken);
        self.last = last;
        self
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            formatter,
            "Request({} {} from {:?})",
            self.method, self.path, self.remote_addr
        )
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        if self.response_writer.is_some() {
            let response = Response::empty(500);
            let _ = self.respond_impl(response); // ignoring any potential error
            if let Some(sender) = self.notify_when_responded.take() {
                sender.send(()).unwrap();
            }
        }
    }
}

/// Dummy trait that regroups the `Read` and `Write` traits.
///
/// Automatically implemented on all types that implement both `Read` and `Write`.
pub trait ReadWrite: Read + Write {}
impl<T> ReadWrite for T where T: Read + Write {}

#[cfg(test)]
mod tests {
    use super::Request;

    #[test]
    fn must_be_send() {
        #![allow(dead_code)]
        fn f<T: Send>(_: &T) {}
        fn bar(rq: &Request) {
            f(rq);
        }
    }
}
