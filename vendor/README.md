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

Two changes, each marked `rusk:` in the code:

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

To update: unpack the new crate, apply the changes (`git diff` of the vendored
tree against the crate shows exactly what they are), run its unit tests with
`cargo test -p tiny_http --lib` (the doc-tests need dev-dependencies that are
not vendored), then rusk's. Drop the directory and the `[patch]` entry once
upstream has both.
