use atomicring::AtomicRingQueue;
use std::io::{Result, Write};
use std::sync::Arc;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone)]
pub struct RingWriter {
    pub queue: Arc<AtomicRingQueue<Vec<u8>>>,
}

impl RingWriter {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Arc::new(AtomicRingQueue::with_capacity(capacity)),
        }
    }
}

impl Write for RingWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // one line of log
        let mut log = buf
            .iter()
            .take_while(|&b| *b != b'\n')
            .copied()
            .collect::<Vec<u8>>();

        let taken = log.len();

        // prefixed with '\n'
        if taken == 0 {
            return Ok(1);
        }

        log.push(b'\n');
        self.queue.push_overwrite(log);

        Ok(taken)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for RingWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
