# Vendored dependencies

Cargo takes the crates here in place of the ones on crates.io (`[patch.crates-io]`
in `Cargo.toml`). Each holds a released version with a change rusk cannot do
without and upstream does not have.

## tiny_http 0.12.0

`tiny_http/` is the published crate `tiny_http-0.12.0.crate` (sha256
`389915df6413a2e74fb181895f933386023c71110878cd0825588928e64cdc82`, the
checksum in `Cargo.lock` before the patch): `src/`, the licenses and the
README, with the manifest stripped of its dev-dependencies; the examples,
benches and integration tests are left out. MIT OR Apache-2.0.

Five changes, each marked `rusk:` in the code:

- `src/util/equal_reader.rs`: `EqualReader::drop` throws the bytes a request
  left unread away through an 8 KiB buffer, and stops where the connection
  ends. Upstream (0.12.0 and its master as of 2026-09) allocates a buffer as
  large as the *announced* rest — `vec![0; remaining_to_read]`, again on every
  read — so a request with `Content-Length: 1000000000000000` that `rusk serve`
  answers without reading its body (413, 401, 404…) aborted the whole process
  when the request was dropped (`memory allocation of 1000000000000000 bytes
  failed`), and one with `Content-Length: 18446744073709551615` panicked its
  thread with `capacity overflow`. Two tests cover it in the same file.
- `src/util/task_pool.rs`: `TaskPool::spawn` queues a task only for an idle
  worker that no queued task has claimed yet. Upstream queues one whenever
  `waiting_tasks` is not zero — but a woken worker stays counted as waiting
  until it has run and taken its task, so a burst of connections (a browser
  opens up to six at once) queued more tasks than there were idle workers, and
  `notify_one` with nobody left waiting woke nobody: the connections past the
  first few were not read until another connection closed, which for a
  keep-alive connection is never (of 6 opened at once, 2 hung; the pool starts
  with 4 idle workers). One test covers it in the same file.
- `src/lib.rs` (with `client.rs`, `connection.rs`, `util/refined_tcp_stream.rs`,
  `util/sequential.rs` and the ssl wrappers): `ServerConfig::limits`, a
  `Limits` — a timeout, a pace, and a limit on the connections open at once.
  The socket waits the timeout on every read and write. Under a request's
  head — from its first byte to the end of its headers — no read waits past
  a deadline of one timeout, whatever comes meanwhile, so a client that sends
  a byte at a time is cut off all the same (408); an idle keep-alive
  connection is closed without a word, as browsers expect. A request's body
  (`PacedBody`) and the answers of a connection keep a pace (`Pace`): the
  time they spend waiting on the socket may come to one timeout and a second
  more per `min_rate` bytes moved (for answers, by the writes that waited —
  what goes straight into the socket's buffer says nothing of the client),
  so a body that trickles in, or an answer taken a byte at a time, is cut
  off too, while a slow client that keeps moving never is. A read or write
  that waited its time out ends the reading of the connection for both of
  its halves: every later read ends at once (EOF), the answer to the request
  still goes out, and no further request is taken from it — not even one
  read ahead into the buffer. At the limit, accepting waits for a connection
  to close (the next waits in the backlog); with a limit, a connection's
  requests are read one at a time, the next after the answer to the one
  before (the channel HTTPS connections use anyway), so the limit bounds the
  requests in flight too. Upstream has none of it: a connection costs a
  thread for as long as a client keeps it, a head or a body may take for
  ever, and the requests down one connection are queued as they come, a
  `Request` each. `Server::num_connections`, `unimplemented!()` upstream,
  counts them. A second handle on an accepted socket that cannot be had (no
  file descriptors left) is an accept error: upstream unwrapped it, and the
  thread that accepts connections panicked, leaving the server up without
  taking any. `Server::http` and friends keep the crate's behaviour
  (`Limits::default()`); rusk uses `Server::new`. Eleven tests cover it in
  `lib.rs`, one `Pace` in `util/refined_tcp_stream.rs`.
- `src/util/chunked_body.rs` (with `request.rs` and `client.rs`): a chunked
  request body is a `ChunkedBody`, `chunked_transfer`'s `Decoder` (1.5.0,
  from crates.io, not vendored) with what it gets wrong about its source put
  right. The decoder turns any failed read in a CRLF — after a chunk's size
  or data, at the end of the body — into a format error (`InvalidInput`,
  "Error while decoding chunks"), a read that timed out among them, so a
  chunked body that stopped there was a bad request, not a timeout: a body
  whose connection was cut off fails as timed out wherever it stopped. The
  decoder takes a source that ends inside a chunk for the end of the body:
  that is an error (`UnexpectedEof`), not a body cut short passed off as
  whole. A body in the wrong format ends the reading of its connection,
  since where the next request starts is not known: upstream read it from
  wherever the decoder stopped, inside the body — a request the client
  never sent on its own. The rest of a body the request leaves unread is
  thrown away when it goes, as `EqualReader` does for one of a known length:
  upstream left it, and read the next request from the middle of it (a
  `400`, and the end of the connection, mostly); it is thrown away for the
  last request of the connection too, as `EqualReader` does, so that the
  answer is not lost to the reset of a connection closed with bytes unread
  (a client that never finishes it holds the connection's thread for the
  timeout, or with no limits for as long as it likes — as it does with a
  `Content-Length`). A chunk's size line, which the decoder keeps in memory
  until its end, may take 4 KiB, no more (review of R34: a client sent
  gigabytes of hex digits and the server kept them all — reachable before
  R34 through any request whose body rusk reads, `POST /auth` among them,
  and through any chunked request since the body is thrown away). And the
  connection takes no request once its reading has ended, even one whose
  turn came after the check (`ClientConnection::read` looks again once the
  first byte of the head is there, which is when the turn comes — a body
  before it may have gone wrong or stopped while it was thrown away, after
  the connection's thread had passed the check and waited for its turn: a
  head read ahead into the buffer was taken then, nonsense answered 400 —
  and once more after the head, for a write that timed out meanwhile). Six
  tests cover it in `util/chunked_body.rs`, four in `lib.rs`.
- `src/response.rs` (with `request.rs` and `client.rs`): an answer after
  which the server closes the connection says so, `Connection: close`
  (RFC 9112 §9.6), which `Response::add_header` does not let a server say
  (it drops a `Connection` header): the `400` to a request line or a header
  that makes no sense, the `408` to a head that stalls, the `417`, the
  answer to a request that said it was the last (`Connection: close`,
  HTTP/1.0 without keep-alive), and whatever answers a request after its
  connection's reading ended — the `408` rusk gives a body that stops, the
  `400` to a chunked body it read and found in the wrong format (an answer
  sent before the body is thrown away cannot know what becomes of it).
  Answers on a connection that goes on are as they were. One test covers
  it in `lib.rs`; two there check it on the way.

To update: unpack the new crate, apply the changes (`git diff` of the vendored
tree against the crate shows exactly what they are), run its unit tests with
`cargo test -p tiny_http --lib` (the doc-tests need dev-dependencies that are
not vendored), then rusk's. Drop the directory and the `[patch]` entry once
upstream has all five.
