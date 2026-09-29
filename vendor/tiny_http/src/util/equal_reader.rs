use std::io::Read;
use std::io::Result as IoResult;
use std::sync::mpsc::channel;
use std::sync::mpsc::{Receiver, Sender};

/// A `Reader` that reads exactly the number of bytes from a sub-reader.
///
/// If the limit is reached, it returns EOF. If the limit is not reached
/// when the destructor is called, the remaining bytes will be read and
/// thrown away.
pub struct EqualReader<R>
where
    R: Read,
{
    reader: R,
    size: usize,
    last_read_signal: Sender<IoResult<()>>,
}

/// The buffer the bytes left unread are thrown away through when the
/// reader is dropped (rusk: see the `Drop` below).
const DISCARD_BUFFER_BYTES: usize = 8 * 1024;

impl<R> EqualReader<R>
where
    R: Read,
{
    pub fn new(reader: R, size: usize) -> (EqualReader<R>, Receiver<IoResult<()>>) {
        let (tx, rx) = channel();

        let r = EqualReader {
            reader,
            size,
            last_read_signal: tx,
        };

        (r, rx)
    }
}

impl<R> Read for EqualReader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if self.size == 0 {
            return Ok(0);
        }

        let buf = if buf.len() < self.size {
            buf
        } else {
            &mut buf[..self.size]
        };

        match self.reader.read(buf) {
            Ok(len) => {
                self.size -= len;
                Ok(len)
            }
            err @ Err(_) => err,
        }
    }
}

impl<R> Drop for EqualReader<R>
where
    R: Read,
{
    // rusk: the bytes the request left unread are thrown away through a
    // small buffer, and no further than the sub-reader gives them. The
    // buffer used to be as large as the rest of the announced body
    // (`vec![0; remaining_to_read]`, allocated again on every read): a
    // `Content-Length` of a petabyte, on a request answered without its
    // body, took the whole process down with it — `memory allocation of
    // 1000000000000000 bytes failed` — and one of `usize::MAX` panicked
    // the thread with `capacity overflow`. The size is what the client
    // says, not what it sends.
    fn drop(&mut self) {
        let mut buf = [0u8; DISCARD_BUFFER_BYTES];

        while self.size > 0 {
            let chunk = buf.len().min(self.size);

            match self.reader.read(&mut buf[..chunk]) {
                Err(e) => {
                    self.last_read_signal.send(Err(e)).ok();
                    return;
                }
                Ok(0) => break,
                Ok(read) => {
                    self.size -= read;
                }
            }
        }

        self.last_read_signal.send(Ok(())).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::EqualReader;
    use std::io::Read;

    #[test]
    fn test_limit() {
        use std::io::Cursor;

        let mut org_reader = Cursor::new("hello world".to_string().into_bytes());

        {
            let (mut equal_reader, _) = EqualReader::new(org_reader.by_ref(), 5);

            let mut string = String::new();
            equal_reader.read_to_string(&mut string).unwrap();
            assert_eq!(string, "hello");
        }

        let mut string = String::new();
        org_reader.read_to_string(&mut string).unwrap();
        assert_eq!(string, " world");
    }

    #[test]
    fn test_not_enough() {
        use std::io::Cursor;

        let mut org_reader = Cursor::new("hello world".to_string().into_bytes());

        {
            let (mut equal_reader, _) = EqualReader::new(org_reader.by_ref(), 5);

            let mut vec = [0];
            equal_reader.read_exact(&mut vec).unwrap();
            assert_eq!(vec[0], b'h');
        }

        let mut string = String::new();
        org_reader.read_to_string(&mut string).unwrap();
        assert_eq!(string, " world");
    }

    /// rusk: dropping a reader whose size was never going to be read did
    /// allocate the size (1 PiB here: the process aborted), and one of
    /// `usize::MAX` overflowed a `Vec`. The rest is read through a small
    /// buffer, and only as far as the sub-reader goes.
    #[test]
    fn dropping_an_unread_size_allocates_nothing_of_it() {
        use std::io::Cursor;

        for size in [1usize << 50, usize::MAX] {
            let mut org_reader = Cursor::new("hello world".to_string().into_bytes());
            let (mut equal_reader, signal) = EqualReader::new(org_reader.by_ref(), size);
            let mut one = [0];
            equal_reader.read_exact(&mut one).unwrap();
            drop(equal_reader);
            assert!(signal.recv().unwrap().is_ok());

            let mut rest = String::new();
            org_reader.read_to_string(&mut rest).unwrap();
            assert_eq!(rest, "", "the rest was not thrown away for {size}");
        }
    }

    /// rusk: the rest is drained no further than the size, and in more
    /// than one read of the small buffer.
    #[test]
    fn dropping_drains_the_size_and_no_more() {
        use std::io::Cursor;

        let data: Vec<u8> = (0..100_000u32).map(|i| i as u8).collect();
        let mut org_reader = Cursor::new(data);
        {
            let (equal_reader, _) = EqualReader::new(org_reader.by_ref(), 50_000);
            drop(equal_reader);
        }
        let mut rest = Vec::new();
        org_reader.read_to_end(&mut rest).unwrap();
        assert_eq!(rest.len(), 50_000);
        assert_eq!(rest[0], (50_000u32) as u8);
    }
}
