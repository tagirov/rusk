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

Three changes, each marked `rusk:` in the code:

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

To update: unpack the new crate, apply the changes (`git diff` of the vendored
tree against the crate shows exactly what they are), run its unit tests with
`cargo test -p tiny_http --lib` (the doc-tests need dev-dependencies that are
not vendored), then rusk's. Drop the directory and the `[patch]` entry once
upstream has all three.
