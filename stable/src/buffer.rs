use parking_lot::{Condvar, Mutex, RwLock};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::ops::Deref;
use std::sync::Arc;

pub struct Buffer {
    inner: (Mutex<VecDeque<u8>>, Condvar),
    finished: RwLock<bool>,
}

impl Buffer {
    pub fn finished(&self) {
        *self.finished.write() = true;
    }

    pub fn is_finished(&self) -> bool {
        *self.finished.read()
    }
}

impl Deref for Buffer {
    type Target = (Mutex<VecDeque<u8>>, Condvar);
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub fn new(capacity: usize) -> (Writer, Reader) {
    let buffer = Buffer {
        inner: (
            Mutex::new(VecDeque::with_capacity(capacity)),
            Condvar::new(),
        ),
        finished: RwLock::new(false),
    };

    let buffer = Arc::new(buffer);

    let writer = Writer {
        buffer: buffer.clone(),
        capacity,
        total: 0,
    };

    let reader = Reader {
        buffer: buffer.clone(),
    };

    (writer, reader)
}

pub struct Writer {
    buffer: Arc<Buffer>,
    capacity: usize,
    total: usize,
}

impl Writer {
    pub fn total(&self) -> usize {
        self.total
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let (lock, condvar) = &**self.buffer;
        // 这里必须保证持有锁, 否则可能导致 Reader 死锁
        let _buffer = lock.lock();

        // 写入缓冲区结束
        self.buffer.finished();
        tracing::debug!("writer finished");

        // 通知读取线程
        condvar.notify_all();
    }
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let (mtx, condvar) = &**self.buffer;

        if buf.is_empty() {
            // 数据已经读取完毕, 但是缓冲区可能未满, 通知读取线程
            condvar.notify_all();
            return Ok(0);
        }

        let mut buffer = mtx.lock();

        // 读取缓冲区结束
        if self.buffer.is_finished() {
            return Ok(0);
        }

        // 如果缓冲区满了, 等待读取线程读取
        while buffer.len() >= self.capacity {
            // 读取缓冲区结束
            if self.buffer.is_finished() {
                tracing::debug!("no reader available, stop write.");
                // buf.len() != 0, but write finishes
                return Ok(0);
            }

            // 通知读取缓冲区
            condvar.notify_all();
            // 释放互斥锁, 等待缓冲区被读取
            condvar.wait(&mut buffer);
        }

        // 持有互斥锁, 写入缓冲区
        let written = (self.capacity - buffer.len())
            // 最大写入量
            .min(buf.len());
        buffer.extend(&buf[..written]);

        // 累计写入数量
        self.total += written;

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let (mtx, condvar) = &**self.buffer;
        let mut buffer = mtx.lock();

        // buffer 非空
        while buffer.len() > 0 {
            // 读取缓冲区结束
            if self.buffer.is_finished() {
                tracing::error!(
                    "no reader available, stop write. buffe size: {}",
                    buffer.len()
                );

                return Err(io::ErrorKind::Interrupted.into());
            }

            // 通知读取缓冲区
            condvar.notify_all();
            // 释放互斥锁, 等待缓冲区被读取
            condvar.wait(&mut buffer);
        }

        Ok(())
    }
}

pub struct Reader {
    buffer: Arc<Buffer>,
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let (lock, condvar) = &**self.buffer;
        let mut buffer = lock.lock();

        // 阻塞直到缓冲区有数据
        while buffer.is_empty() {
            // 写入缓冲区结束
            if self.buffer.is_finished() {
                return Ok(0);
            }

            // 通知写入缓冲区
            condvar.notify_all();
            // 释放锁, 等待缓冲区写入
            condvar.wait(&mut buffer);
        }

        // 持有互斥锁, 读取缓冲区
        let buf_size = buf.len();

        // 如果缓冲区数据够多, 读取写入缓存数量
        if buffer.len() >= buf_size {
            let take = buffer.drain(..buf_size).collect::<Vec<_>>();
            buf.copy_from_slice(&take);
            return Ok(buf_size);
        }

        // 如果缓冲区数据不够, 读取全部数据
        let take = buffer.drain(..).collect::<Vec<_>>();
        let taken = take.len();
        buf[..taken].copy_from_slice(&take);

        Ok(taken)
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        let (lock, condvar) = &**self.buffer;
        // 这里必须保证持有锁, 否则可能导致 Reader 死锁
        let _buffer = lock.lock();

        // 写入缓冲区结束
        self.buffer.finished();
        tracing::debug!("reader finished");

        // 通知读取线程
        condvar.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(b"1234567890", 2)]
    fn test_buffer(#[case] data: &[u8], #[case] size: usize) {
        let (mut writer, mut reader) = new(size);

        let size = writer.write(data).unwrap();
        assert_eq!(writer.total(), size);

        let mut buf = vec![0; size];
        let r = reader.read(&mut buf).unwrap();
        assert_eq!(r, size);
    }

    #[test]
    fn test_write_more() {
        let (mut writer, mut reader) = new(3);

        let _ = std::thread::spawn(move || {
            writer.write_all(b"0123456789").unwrap();
            assert_eq!(writer.total(), 10);
        });

        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0; 10];
            assert_eq!(reader.read(&mut buf).unwrap(), 3);
            assert_eq!(reader.read(&mut buf).unwrap(), 3);
            assert_eq!(reader.read(&mut buf).unwrap(), 3);
            assert_eq!(reader.read(&mut buf).unwrap(), 1);
            assert_eq!(reader.read(&mut buf).unwrap(), 0);
        });

        reader_thread.join().unwrap();
    }

    #[test]
    fn test_write_zero() {
        let (mut writer, mut reader) = new(3);

        let _ = std::thread::spawn(move || {
            writer.write_all(&[]).unwrap();
            assert_eq!(writer.total(), 0);
        });

        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0; 10];
            assert_eq!(reader.read(&mut buf).unwrap(), 0);
            assert_eq!(reader.read(&mut buf).unwrap(), 0);
        });

        reader_thread.join().unwrap();
    }

    #[test]
    fn test_write_flush() {
        let (mut writer, mut reader) = new(3);
        writer.write_all(b"012").unwrap();
        reader.read_exact(&mut [0; 3]).unwrap();
        writer.flush().unwrap();
    }

    #[test]
    fn test_write_interrupt() {
        let (mut writer, reader) = new(3);
        writer.write_all(b"012").unwrap();
        drop(reader);

        assert_eq!(
            writer.flush().err().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
    }
}
