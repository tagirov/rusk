//! # Simple usage
//!
//! ## Creating the server
//!
//! The easiest way to create a server is to call `Server::http()`.
//!
//! The `http()` function returns an `IoResult<Server>` which will return an error
//! in the case where the server creation fails (for example if the listening port is already
//! occupied).
//!
//! ```no_run
//! let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
//! ```
//!
//! A newly-created `Server` will immediately start listening for incoming connections and HTTP
//! requests.
//!
//! ## Receiving requests
//!
//! Calling `server.recv()` will block until the next request is available.
//! This function returns an `IoResult<Request>`, so you need to handle the possible errors.
//!
//! ```no_run
//! # let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
//!
//! loop {
//!     // blocks until the next request is received
//!     let request = match server.recv() {
//!         Ok(rq) => rq,
//!         Err(e) => { println!("error: {}", e); break }
//!     };
//!
//!     // do something with the request
//!     // ...
//! }
//! ```
//!
//! In a real-case scenario, you will probably want to spawn multiple worker tasks and call
//! `server.recv()` on all of them. Like this:
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use std::thread;
//! # let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
//! let server = Arc::new(server);
//! let mut guards = Vec::with_capacity(4);
//!
//! for _ in (0 .. 4) {
//!     let server = server.clone();
//!
//!     let guard = thread::spawn(move || {
//!         loop {
//!             let rq = server.recv().unwrap();
//!
//!             // ...
//!         }
//!     });
//!
//!     guards.push(guard);
//! }
//! ```
//!
//! If you don't want to block, you can call `server.try_recv()` instead.
//!
//! ## Handling requests
//!
//! The `Request` object returned by `server.recv()` contains informations about the client's request.
//! The most useful methods are probably `request.method()` and `request.url()` which return
//! the requested method (`GET`, `POST`, etc.) and url.
//!
//! To handle a request, you need to create a `Response` object. See the docs of this object for
//! more infos. Here is an example of creating a `Response` from a file:
//!
//! ```no_run
//! # use std::fs::File;
//! # use std::path::Path;
//! let response = tiny_http::Response::from_file(File::open(&Path::new("image.png")).unwrap());
//! ```
//!
//! All that remains to do is call `request.respond()`:
//!
//! ```no_run
//! # use std::fs::File;
//! # use std::path::Path;
//! # let server = tiny_http::Server::http("0.0.0.0:0").unwrap();
//! # let request = server.recv().unwrap();
//! # let response = tiny_http::Response::from_file(File::open(&Path::new("image.png")).unwrap());
//! let _ = request.respond(response);
//! ```
#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]
#![allow(clippy::match_like_matches_macro)]

#[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
use zeroize::Zeroizing;

use std::error::Error;
use std::io::Error as IoError;
use std::io::ErrorKind as IoErrorKind;
use std::io::Result as IoResult;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use client::ClientConnection;
use connection::Connection;
use util::MessagesQueue;

pub use common::{HTTPVersion, Header, HeaderField, Method, StatusCode};
pub use connection::{ConfigListenAddr, ListenAddr, Listener};
pub use request::{ReadWrite, Request};
pub use response::{Response, ResponseBox};
pub use test::TestRequest;

mod client;
mod common;
mod connection;
mod request;
mod response;
mod ssl;
mod test;
mod util;

/// The main class of this library.
///
/// Destroying this object will immediately close the listening socket and the reading
///  part of all the client's connections. Requests that have already been returned by
///  the `recv()` function will not close and the responses will be transferred to the client.
pub struct Server {
    // should be false as long as the server exists
    // when set to true, all the subtasks will close within a few hundreds ms
    close: Arc<AtomicBool>,

    // queue for messages received by child threads
    messages: Arc<MessagesQueue<Message>>,

    // result of TcpListener::local_addr()
    listening_addr: ListenAddr,

    // rusk: the connections open right now (see `Limits`)
    connections: Arc<Connections>,
}

/// rusk: what the server lets its connections take of it. The crate as
/// published (`Limits::default()`) lets them take anything: a connection
/// costs a thread for as long as the client keeps it open, a request's
/// head or body may take for ever to arrive, and a client may open as many
/// connections as the server has file descriptors, and send as many
/// requests down one of them as it likes without waiting for an answer —
/// each of them a `Request` in the queue.
#[derive(Debug, Clone, Default)]
pub struct Limits {
    /// How long a connection may wait: idle, for the next request; for the
    /// rest of a request's head (request line and headers) after its first
    /// byte, however slowly the bytes come; and, in a request's body or in
    /// an answer, for the socket to move at all. A connection that waits
    /// longer is closed: with `408 Request Timeout` when part of a head had
    /// come, without a word when it was idle, and — a body or an answer —
    /// with the read or write failing as timed out for whoever holds the
    /// `Request`, after which every read of it ends (EOF). `None`: for
    /// ever.
    pub timeout: Option<Duration>,

    /// Connections open at once. At the limit, the next one is accepted
    /// when one closes (it waits in the listening socket's backlog
    /// meanwhile). With a limit, a connection's requests are also read one
    /// at a time — the next after the answer to the one before has been
    /// sent — so the limit bounds the requests in flight too. `None`: no
    /// limit, and the requests of a connection are read as they come
    /// (pipelining).
    pub max_connections: Option<usize>,

    /// Bytes a second a request's body, and the answers of a connection,
    /// have to keep up once they have waited one `timeout` (with a timeout
    /// only): the time their reads or writes spend waiting on the socket
    /// may come to one timeout, and a second more for every `min_rate`
    /// bytes moved. Without it, a client that sends a byte of a body, or
    /// takes a byte of an answer, now and then — each well within the
    /// timeout — holds its connection for ever. `None`: no pace.
    pub min_rate: Option<u32>,
}

/// rusk: the connections open at once, and the places for more (see
/// `Limits::max_connections`).
struct Connections {
    open: Mutex<usize>,
    closed: Condvar,
}

/// rusk: one open connection's place in the count, given back when the
/// connection's task ends.
struct ConnectionSlot(Arc<Connections>);

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        *self.0.open.lock().unwrap() -= 1;
        self.0.closed.notify_one();
    }
}

impl Connections {
    fn new() -> Arc<Connections> {
        Arc::new(Connections {
            open: Mutex::new(0),
            closed: Condvar::new(),
        })
    }

    /// Waits for room for one more connection: below `max` (when there is
    /// one), as soon as another closes; `false` when the server closes
    /// meanwhile.
    fn wait_for_room(&self, max: Option<usize>, close: &AtomicBool) -> bool {
        let mut open = self.open.lock().unwrap();
        while max.is_some_and(|max| *open >= max) {
            if close.load(Relaxed) {
                return false;
            }
            open = self
                .closed
                .wait_timeout(open, Duration::from_millis(100))
                .unwrap()
                .0;
        }
        true
    }

    /// One more connection's place in the count.
    fn enter(self: &Arc<Self>) -> ConnectionSlot {
        *self.open.lock().unwrap() += 1;
        ConnectionSlot(self.clone())
    }
}

enum Message {
    Error(IoError),
    NewRequest(Request),
}

impl From<IoError> for Message {
    fn from(e: IoError) -> Message {
        Message::Error(e)
    }
}

impl From<Request> for Message {
    fn from(rq: Request) -> Message {
        Message::NewRequest(rq)
    }
}

// this trait is to make sure that Server implements Share and Send
#[doc(hidden)]
trait MustBeShareDummy: Sync + Send {}
#[doc(hidden)]
impl MustBeShareDummy for Server {}

pub struct IncomingRequests<'a> {
    server: &'a Server,
}

/// Represents the parameters required to create a server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The addresses to try to listen to.
    pub addr: ConfigListenAddr,

    /// If `Some`, then the server will use SSL to encode the communications.
    pub ssl: Option<SslConfig>,

    /// rusk: what the connections may take of the server (see `Limits`).
    pub limits: Limits,
}

/// Configuration of the server for SSL.
#[derive(Debug, Clone)]
pub struct SslConfig {
    /// Contains the public certificate to send to clients.
    pub certificate: Vec<u8>,
    /// Contains the ultra-secret private key used to decode communications.
    pub private_key: Vec<u8>,
}

impl Server {
    /// Shortcut for a simple server on a specific address.
    #[inline]
    pub fn http<A>(addr: A) -> Result<Server, Box<dyn Error + Send + Sync + 'static>>
    where
        A: ToSocketAddrs,
    {
        Server::new(ServerConfig {
            addr: ConfigListenAddr::from_socket_addrs(addr)?,
            ssl: None,
            limits: Limits::default(),
        })
    }

    /// Shortcut for an HTTPS server on a specific address.
    #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
    #[inline]
    pub fn https<A>(
        addr: A,
        config: SslConfig,
    ) -> Result<Server, Box<dyn Error + Send + Sync + 'static>>
    where
        A: ToSocketAddrs,
    {
        Server::new(ServerConfig {
            addr: ConfigListenAddr::from_socket_addrs(addr)?,
            ssl: Some(config),
            limits: Limits::default(),
        })
    }

    #[cfg(unix)]
    #[inline]
    /// Shortcut for a UNIX socket server at a specific path
    pub fn http_unix(
        path: &std::path::Path,
    ) -> Result<Server, Box<dyn Error + Send + Sync + 'static>> {
        Server::new(ServerConfig {
            addr: ConfigListenAddr::unix_from_path(path),
            ssl: None,
            limits: Limits::default(),
        })
    }

    /// Builds a new server that listens on the specified address.
    pub fn new(config: ServerConfig) -> Result<Server, Box<dyn Error + Send + Sync + 'static>> {
        let listener = config.addr.bind()?;
        Self::from_listener_within(listener, config.ssl, config.limits)
    }

    /// Builds a new server using the specified TCP listener.
    ///
    /// This is useful if you've constructed TcpListener using some less usual method
    /// such as from systemd. For other cases, you probably want the `new()` function.
    pub fn from_listener<L: Into<Listener>>(
        listener: L,
        ssl_config: Option<SslConfig>,
    ) -> Result<Server, Box<dyn Error + Send + Sync + 'static>> {
        Self::from_listener_within(listener, ssl_config, Limits::default())
    }

    /// rusk: `from_listener`, within `limits` (see `Limits`).
    pub fn from_listener_within<L: Into<Listener>>(
        listener: L,
        ssl_config: Option<SslConfig>,
        limits: Limits,
    ) -> Result<Server, Box<dyn Error + Send + Sync + 'static>> {
        let listener = listener.into();
        // building the "close" variable
        let close_trigger = Arc::new(AtomicBool::new(false));

        // building the TcpListener
        let (server, local_addr) = {
            let local_addr = listener.local_addr()?;
            log::debug!("Server listening on {}", local_addr);
            (listener, local_addr)
        };

        // building the SSL capabilities
        #[cfg(all(feature = "ssl-openssl", feature = "ssl-rustls"))]
        compile_error!(
            "Features 'ssl-openssl' and 'ssl-rustls' must not be enabled at the same time"
        );
        #[cfg(not(any(feature = "ssl-openssl", feature = "ssl-rustls")))]
        type SslContext = ();
        #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
        type SslContext = crate::ssl::SslContextImpl;
        let ssl: Option<SslContext> = {
            match ssl_config {
                #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
                Some(config) => Some(SslContext::from_pem(
                    config.certificate,
                    Zeroizing::new(config.private_key),
                )?),
                #[cfg(not(any(feature = "ssl-openssl", feature = "ssl-rustls")))]
                Some(_) => return Err(
                    "Building a server with SSL requires enabling the `ssl` feature in tiny-http"
                        .into(),
                ),
                None => None,
            }
        };

        // creating a task where server.accept() is continuously called
        // and ClientConnection objects are pushed in the messages queue
        let messages = MessagesQueue::with_capacity(8);

        // rusk: the connections open at once
        let connections = Connections::new();

        let inside_close_trigger = close_trigger.clone();
        let inside_messages = messages.clone();
        let inside_connections = connections.clone();
        thread::spawn(move || {
            // a tasks pool is used to dispatch the connections into threads
            let tasks_pool = util::TaskPool::new();

            log::debug!("Running accept thread");
            while !inside_close_trigger.load(Relaxed) {
                // rusk: at the limit, the next connection waits in the
                // backlog until one closes
                if !inside_connections.wait_for_room(limits.max_connections, &inside_close_trigger) {
                    break;
                }
                let new_client = match server.accept() {
                    Ok((sock, _)) => {
                        use util::RefinedTcpStream;
                        let slot = inside_connections.enter();
                        // rusk: the timeouts, before anything is read from
                        // the connection (the reads and writes of a TLS
                        // handshake, which is made on this thread, too)
                        if let Err(e) = sock
                            .set_read_timeout(limits.timeout)
                            .and_then(|_| sock.set_write_timeout(limits.timeout))
                        {
                            log::error!("Error limiting a new client: {}", e);
                            continue;
                        }
                        let halves = match ssl {
                            None => RefinedTcpStream::new(sock, limits.timeout, limits.min_rate),
                            #[cfg(any(feature = "ssl-openssl", feature = "ssl-rustls"))]
                            Some(ref ssl) => {
                                // trying to apply SSL over the connection
                                // if an error occurs, we just close the socket and resume listening
                                let sock = match ssl.accept(sock) {
                                    Ok(s) => s,
                                    Err(_) => continue,
                                };

                                RefinedTcpStream::new(sock, limits.timeout, limits.min_rate)
                            }
                            #[cfg(not(any(feature = "ssl-openssl", feature = "ssl-rustls")))]
                            Some(ref _ssl) => unreachable!(),
                        };

                        // rusk: a second handle on the socket takes a file
                        // descriptor, and there may be none left: that is
                        // as bad as a failed accept (it used to panic this
                        // thread, and the server went on without taking
                        // connections)
                        halves.map(|(read_closable, write_closable)| {
                            (
                                ClientConnection::new(
                                    write_closable,
                                    read_closable,
                                    limits.timeout,
                                    limits.min_rate,
                                ),
                                slot,
                            )
                        })
                    }
                    Err(e) => Err(e),
                };

                match new_client {
                    Ok((client, slot)) => {
                        let messages = inside_messages.clone();
                        let mut client = Some(client);
                        // rusk: with a limit on the connections, a
                        // connection's requests are read one at a time
                        // (see `Limits::max_connections`)
                        let one_at_a_time = limits.max_connections.is_some();
                        let mut slot = Some(slot);
                        tasks_pool.spawn(Box::new(move || {
                            if let Some(client) = client.take() {
                                // rusk: the connection's place in the
                                // count, given back when this task ends
                                let _slot = slot.take();
                                // Synchronization is needed for HTTPS requests to avoid a deadlock
                                if client.secure() || one_at_a_time {
                                    let (sender, receiver) = mpsc::channel();
                                    for rq in client {
                                        messages.push(rq.with_notify_sender(sender.clone()).into());
                                        receiver.recv().unwrap();
                                    }
                                } else {
                                    for rq in client {
                                        messages.push(rq.into());
                                    }
                                }
                            }
                        }));
                    }

                    Err(e) => {
                        log::error!("Error accepting new client: {}", e);
                        inside_messages.push(e.into());
                        break;
                    }
                }
            }
            log::debug!("Terminating accept thread");
        });

        // result
        Ok(Server {
            messages,
            close: close_trigger,
            listening_addr: local_addr,
            connections,
        })
    }

    /// Returns an iterator for all the incoming requests.
    ///
    /// The iterator will return `None` if the server socket is shutdown.
    #[inline]
    pub fn incoming_requests(&self) -> IncomingRequests<'_> {
        IncomingRequests { server: self }
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn server_addr(&self) -> ListenAddr {
        self.listening_addr.clone()
    }

    /// Returns the number of clients currently connected to the server.
    ///
    /// rusk: counted (the crate as published had `unimplemented!()`): the
    /// connections accepted and not yet done with, whether or not one of
    /// their requests is being answered.
    pub fn num_connections(&self) -> usize {
        *self.connections.open.lock().unwrap()
    }

    /// Blocks until an HTTP request has been submitted and returns it.
    pub fn recv(&self) -> IoResult<Request> {
        match self.messages.pop() {
            Some(Message::Error(err)) => Err(err),
            Some(Message::NewRequest(rq)) => Ok(rq),
            None => Err(IoError::new(IoErrorKind::Other, "thread unblocked")),
        }
    }

    /// Same as `recv()` but doesn't block longer than timeout
    pub fn recv_timeout(&self, timeout: Duration) -> IoResult<Option<Request>> {
        match self.messages.pop_timeout(timeout) {
            Some(Message::Error(err)) => Err(err),
            Some(Message::NewRequest(rq)) => Ok(Some(rq)),
            None => Ok(None),
        }
    }

    /// Same as `recv()` but doesn't block.
    pub fn try_recv(&self) -> IoResult<Option<Request>> {
        match self.messages.try_pop() {
            Some(Message::Error(err)) => Err(err),
            Some(Message::NewRequest(rq)) => Ok(Some(rq)),
            None => Ok(None),
        }
    }

    /// Unblock thread stuck in recv() or incoming_requests().
    /// If there are several such threads, only one is unblocked.
    /// This method allows graceful shutdown of server.
    pub fn unblock(&self) {
        self.messages.unblock();
    }
}

impl Iterator for IncomingRequests<'_> {
    type Item = Request;
    fn next(&mut self) -> Option<Request> {
        self.server.recv().ok()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.close.store(true, Relaxed);
        // Connect briefly to ourselves to unblock the accept thread
        let maybe_stream = match &self.listening_addr {
            ListenAddr::IP(addr) => TcpStream::connect(addr).map(Connection::from),
            #[cfg(unix)]
            ListenAddr::Unix(addr) => {
                // TODO: use connect_addr when its stabilized.
                let path = addr.as_pathname().unwrap();
                std::os::unix::net::UnixStream::connect(path).map(Connection::from)
            }
        };
        if let Ok(stream) = maybe_stream {
            let _ = stream.shutdown(Shutdown::Both);
        }

        #[cfg(unix)]
        if let ListenAddr::Unix(addr) = &self.listening_addr {
            if let Some(path) = addr.as_pathname() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// rusk: what `Limits` does to a connection (see `Limits`). The bounds on
/// time are loose on purpose (review of R33): a machine under load, or
/// Windows with its clock ticks, runs a little early or late.
#[cfg(test)]
mod limits {
    use super::{ConfigListenAddr, Limits, Response, Server, ServerConfig};
    use crate::util::refined_tcp_stream::timed_out;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::thread;
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_millis(500);

    fn serve_within(limits: Limits) -> (Server, SocketAddr) {
        let server = Server::new(ServerConfig {
            addr: ConfigListenAddr::from_socket_addrs("127.0.0.1:0").unwrap(),
            ssl: None,
            limits,
        })
        .unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        (server, addr)
    }

    fn timeout_only() -> Limits {
        Limits {
            timeout: Some(TIMEOUT),
            ..Limits::default()
        }
    }

    fn client(addr: SocketAddr) -> TcpStream {
        let stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
    }

    /// What the server sends until it closes the connection (or resets it:
    /// then what came before the reset).
    fn rest_of(stream: &mut TcpStream) -> String {
        let mut buf = Vec::new();
        let _ = stream.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// The server's next request, which has to be there.
    fn next_request(server: &Server) -> super::Request {
        server
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .expect("no request came")
    }

    /// About one timeout: not before it, and well before a client that is
    /// never cut off would be done.
    fn assert_in_time(took: Duration) {
        assert!(
            took >= TIMEOUT - Duration::from_millis(50) && took < TIMEOUT * 6,
            "took {:?}",
            took
        );
    }

    fn assert_no_connection_left(server: &Server) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while server.num_connections() != 0 {
            assert!(Instant::now() < deadline, "a connection is still open");
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// A connection that stays idle for the timeout is closed without a
    /// word — a new one, and one that has been answered (keep-alive)
    /// alike.
    #[test]
    fn an_idle_connection_is_closed_without_a_word() {
        let (server, addr) = serve_within(timeout_only());
        let mut idle = client(addr);
        let started = Instant::now();
        assert_eq!(rest_of(&mut idle), "");
        assert_in_time(started.elapsed());

        let mut http = client(addr);
        http.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        next_request(&server)
            .respond(Response::from_string("hi"))
            .unwrap();
        let started = Instant::now();
        let answer = rest_of(&mut http);
        assert!(
            answer.starts_with("HTTP/1.1 200") && answer.ends_with("\r\n\r\nhi"),
            "{}",
            answer
        );
        assert_in_time(started.elapsed());
        assert_no_connection_left(&server);
    }

    /// A head that stops half-way is answered 408 at the timeout, and the
    /// connection ends; no request reaches the server.
    #[test]
    fn a_head_that_stalls_is_408() {
        let (server, addr) = serve_within(timeout_only());
        let mut slow = client(addr);
        slow.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n").unwrap();
        let started = Instant::now();
        let answer = rest_of(&mut slow);
        assert!(answer.starts_with("HTTP/1.1 408"), "{}", answer);
        assert_in_time(started.elapsed());
        assert!(server
            .recv_timeout(Duration::from_millis(100))
            .unwrap()
            .is_none());
        assert_no_connection_left(&server);
    }

    /// A head that comes a byte at a time, each well within the timeout,
    /// is cut off at the deadline all the same: the timeout is for the
    /// head as a whole, not for each of its bytes.
    #[test]
    fn a_head_that_trickles_in_is_cut_at_its_deadline() {
        let (server, addr) = serve_within(timeout_only());
        let mut slow = client(addr);
        let started = Instant::now();
        let writer = {
            let mut slow = slow.try_clone().unwrap();
            thread::spawn(move || {
                for byte in b"GET / HTTP/1.1\r\nHost: x\r\nX-Slow: y\r\n".iter().cycle() {
                    if slow.write_all(&[*byte]).is_err() || started.elapsed() > TIMEOUT * 12 {
                        break;
                    }
                    thread::sleep(TIMEOUT / 8);
                }
            })
        };
        // The 408 may be lost to the reset of a connection closed with
        // bytes unread; what is certain is when the connection ends.
        let answer = rest_of(&mut slow);
        let took = started.elapsed();
        assert!(
            took >= TIMEOUT - Duration::from_millis(50) && took < TIMEOUT * 8,
            "took {:?}: {}",
            took,
            answer
        );
        writer.join().unwrap();
        assert!(server
            .recv_timeout(Duration::from_millis(100))
            .unwrap()
            .is_none());
        assert_no_connection_left(&server);
    }

    /// A body that stops coming fails its reader as timed out, after which
    /// the reader gives nothing more; the answer still goes out, and the
    /// connection ends with it.
    #[test]
    fn a_body_that_stalls_fails_its_reader_and_ends_the_connection() {
        let (server, addr) = serve_within(timeout_only());
        let mut http = client(addr);
        http.write_all(b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 2000\r\n\r\nabc")
            .unwrap();
        let mut request = next_request(&server);
        let started = Instant::now();
        let mut body = Vec::new();
        let err = request.as_reader().read_to_end(&mut body).unwrap_err();
        assert!(timed_out(&err), "{:?}", err);
        assert_in_time(started.elapsed());
        assert_eq!(body, b"abc");
        assert_eq!(request.as_reader().read(&mut [0; 8]).unwrap(), 0);
        request.respond(Response::empty(408)).unwrap();
        let answer = rest_of(&mut http);
        assert!(answer.starts_with("HTTP/1.1 408"), "{}", answer);
        assert_no_connection_left(&server);
    }

    /// A body that comes a few bytes at a time, each well within the
    /// timeout, is cut off once it has waited more than its pace allows;
    /// one that keeps the pace goes through, however long it takes.
    #[test]
    fn a_body_keeps_its_pace_or_is_cut_off() {
        let (server, addr) = serve_within(Limits {
            timeout: Some(TIMEOUT),
            max_connections: None,
            min_rate: Some(1000),
        });

        let mut slow = client(addr);
        slow.write_all(b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 100000\r\n\r\n")
            .unwrap();
        let mut request = next_request(&server);
        let started = Instant::now();
        let writer = {
            let mut slow = slow.try_clone().unwrap();
            thread::spawn(move || {
                // Ten bytes every TIMEOUT / 5: 100 bytes a second, a
                // tenth of the pace.
                while started.elapsed() < TIMEOUT * 12 {
                    if slow.write_all(b"0123456789").is_err() {
                        break;
                    }
                    thread::sleep(TIMEOUT / 5);
                }
            })
        };
        let mut body = Vec::new();
        let err = request.as_reader().read_to_end(&mut body).unwrap_err();
        assert!(timed_out(&err), "{:?}", err);
        let took = started.elapsed();
        assert!(
            took >= TIMEOUT - Duration::from_millis(50) && took < TIMEOUT * 8,
            "took {:?}",
            took
        );
        request.respond(Response::empty(408)).unwrap();
        drop(slow);
        writer.join().unwrap();

        // 4 KB every TIMEOUT / 5: 40 kB a second, over four timeouts.
        let mut steady = client(addr);
        steady
            .write_all(b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 81920\r\n\r\n")
            .unwrap();
        let mut request = next_request(&server);
        let writer = {
            let mut steady = steady.try_clone().unwrap();
            thread::spawn(move || {
                for _ in 0..20 {
                    steady.write_all(&[b'x'; 4096]).unwrap();
                    thread::sleep(TIMEOUT / 5);
                }
            })
        };
        let mut body = Vec::new();
        request.as_reader().read_to_end(&mut body).unwrap();
        assert_eq!(body.len(), 81920);
        writer.join().unwrap();
        request.respond(Response::from_string("ok")).unwrap();
        let mut answer = [0; 64];
        let n = steady.read(&mut answer).unwrap();
        assert!(String::from_utf8_lossy(&answer[..n]).starts_with("HTTP/1.1 200"));
    }

    /// An answer the client takes nothing of for the timeout fails as
    /// timed out, and the connection ends.
    #[test]
    fn an_answer_nobody_takes_fails_in_time() {
        let (server, addr) = serve_within(timeout_only());
        let mut http = client(addr);
        http.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let request = next_request(&server);
        let started = Instant::now();
        let err = request
            .respond(Response::from_data(vec![b'x'; 64 << 20]))
            .unwrap_err();
        assert!(timed_out(&err), "{:?}", err);
        // A write that moved a little before it waited its timeout out is
        // followed by one more that waits it whole.
        let took = started.elapsed();
        assert!(
            took >= TIMEOUT - Duration::from_millis(50) && took < TIMEOUT * 8,
            "took {:?}",
            took
        );
        assert_no_connection_left(&server);
        drop(http);
    }

    /// An answer taken steadily but too slowly — the client never stalls
    /// long enough for a write to time out — fails once it has waited more
    /// than its pace allows.
    #[test]
    fn an_answer_taken_too_slowly_is_cut_off() {
        let (server, addr) = serve_within(Limits {
            timeout: Some(TIMEOUT),
            max_connections: None,
            min_rate: Some(1 << 20),
        });
        let mut http = client(addr);
        http.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let request = next_request(&server);
        let started = Instant::now();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = {
            let mut http = http.try_clone().unwrap();
            let stop = stop.clone();
            thread::spawn(move || {
                // 64 KiB every TIMEOUT / 5: 640 KiB a second, under the
                // pace of 1 MiB.
                let mut chunk = vec![0; 64 << 10];
                while started.elapsed() < TIMEOUT * 20 && !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    match http.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => thread::sleep(TIMEOUT / 5),
                    }
                }
            })
        };
        let err = request
            .respond(Response::from_data(vec![b'x'; 64 << 20]))
            .unwrap_err();
        assert!(timed_out(&err), "{:?}", err);
        assert!(started.elapsed() < TIMEOUT * 12, "took {:?}", started.elapsed());
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        reader.join().unwrap();
        drop(http);
        assert_no_connection_left(&server);
    }

    /// Review of R33: once a write has timed out, the requests the client
    /// sent ahead are not taken any more — they used to be, and answered
    /// into a dead socket, every one of them.
    #[test]
    fn after_a_write_timeout_no_request_is_taken() {
        let (server, addr) = serve_within(Limits {
            timeout: Some(TIMEOUT),
            max_connections: Some(8),
            min_rate: None,
        });
        let mut http = client(addr);
        http.write_all(&b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".repeat(50))
            .unwrap();
        let request = next_request(&server);
        let err = request
            .respond(Response::from_data(vec![b'x'; 64 << 20]))
            .unwrap_err();
        assert!(timed_out(&err), "{:?}", err);
        assert!(
            server
                .recv_timeout(Duration::from_millis(500))
                .unwrap()
                .is_none(),
            "a request was taken after the write timed out"
        );
        assert_no_connection_left(&server);
        drop(http);
    }

    /// Review of R33: a timeout too large to add to the clock (the config
    /// takes any number of seconds) panicked the thread of every
    /// connection; it is no deadline.
    #[test]
    fn a_timeout_too_large_for_the_clock_is_no_deadline() {
        let (server, addr) = serve_within(Limits {
            timeout: Some(Duration::MAX),
            max_connections: Some(8),
            min_rate: Some(1024),
        });
        let mut http = client(addr);
        http.write_all(b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 2000\r\n\r\n")
            .unwrap();
        http.write_all(&[b'x'; 2000]).unwrap();
        let mut request = next_request(&server);
        let mut body = Vec::new();
        request.as_reader().read_to_end(&mut body).unwrap();
        assert_eq!(body.len(), 2000);
        request.respond(Response::from_string("ok")).unwrap();
        let mut answer = [0; 64];
        let n = http.read(&mut answer).unwrap();
        assert!(String::from_utf8_lossy(&answer[..n]).starts_with("HTTP/1.1 200"));
    }

    /// At the limit, the next connection is not read until one of the open
    /// ones closes; it is neither refused nor lost.
    #[test]
    fn at_the_limit_the_next_connection_waits_for_one_to_close() {
        let (server, addr) = serve_within(Limits {
            max_connections: Some(2),
            ..Limits::default()
        });
        let first = client(addr);
        let second = client(addr);
        let deadline = Instant::now() + Duration::from_secs(10);
        while server.num_connections() != 2 {
            assert!(Instant::now() < deadline, "the first two were not counted");
            thread::sleep(Duration::from_millis(20));
        }
        let mut third = client(addr);
        third
            .write_all(b"GET /third HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        assert!(
            server
                .recv_timeout(Duration::from_millis(300))
                .unwrap()
                .is_none(),
            "the third connection was read over the limit"
        );
        assert_eq!(server.num_connections(), 2);
        drop(first);
        let request = next_request(&server);
        assert_eq!(request.url(), "/third");
        request.respond(Response::from_string("ok")).unwrap();
        let mut answer = [0; 64];
        let n = third.read(&mut answer).unwrap();
        assert!(String::from_utf8_lossy(&answer[..n]).starts_with("HTTP/1.1 200"));
        drop(second);
        drop(third);
        assert_no_connection_left(&server);
    }

    /// With a limit on connections, a connection's requests are read one
    /// at a time, the next after the answer to the one before; without
    /// one, as they come (pipelining, as published).
    #[test]
    fn with_a_limit_a_connections_requests_are_read_one_at_a_time() {
        let two = b"GET /1 HTTP/1.1\r\nHost: x\r\n\r\nGET /2 HTTP/1.1\r\nHost: x\r\n\r\n";

        let (server, addr) = serve_within(Limits {
            max_connections: Some(8),
            ..Limits::default()
        });
        let mut http = client(addr);
        http.write_all(two).unwrap();
        let first = next_request(&server);
        assert_eq!(first.url(), "/1");
        assert!(
            server
                .recv_timeout(Duration::from_millis(300))
                .unwrap()
                .is_none(),
            "the second request was read before the first was answered"
        );
        first.respond(Response::from_string("one")).unwrap();
        let second = next_request(&server);
        assert_eq!(second.url(), "/2");
        second.respond(Response::from_string("two")).unwrap();
        let mut answers = Vec::new();
        let mut chunk = [0; 4096];
        while !String::from_utf8_lossy(&answers).ends_with("two") {
            let n = http.read(&mut chunk).unwrap();
            assert!(n > 0, "the connection closed before both answers came");
            answers.extend_from_slice(&chunk[..n]);
        }
        let answers = String::from_utf8_lossy(&answers);
        assert!(answers.find("one").unwrap() < answers.find("two").unwrap());
        drop(http);
        drop(server);

        let (server, addr) = serve_within(Limits::default());
        let mut http = client(addr);
        http.write_all(two).unwrap();
        let first = next_request(&server);
        assert_eq!(first.url(), "/1");
        let second = next_request(&server);
        assert_eq!(second.url(), "/2");
        drop((first, second));
    }
}
