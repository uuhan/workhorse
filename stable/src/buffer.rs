use parking_lot::{Condvar, Mutex, RwLock};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::Arc;

pub struct Writer {
    buffer: Arc<(Mutex<VecDeque<u8>>, Condvar)>,
    complete: Arc<RwLock<bool>>,
    capacity: usize,
    total: usize,
}

impl Writer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Arc::new((
                Mutex::new(VecDeque::with_capacity(capacity)),
                Condvar::new(),
            )),
            complete: Arc::new(RwLock::new(false)),
            capacity,
            total: 0,
        }
    }

    pub fn make_reader(&self) -> Reader {
        Reader {
            buffer: self.buffer.clone(),
            complete: self.complete.clone(),
        }
    }
}

impl Writer {
    pub fn total(&self) -> usize {
        self.total
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let (lock, condvar) = &*self.buffer;
        // 这里必须保证持有锁, 否则可能导致 Reader 死锁
        let _buffer = lock.lock();

        // 写入缓冲区结束
        *self.complete.write() = true;
        // 通知读取线程
        condvar.notify_all();
    }
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let (lock, condvar) = &*self.buffer;

        if buf.is_empty() {
            // 数据已经读取完毕, 但是缓冲区可能未满, 通知读取线程
            condvar.notify_all();
            return Ok(0);
        }

        let mut buffer = lock.lock();

        // 如果缓冲区满了, 等待读取线程读取
        while buffer.len() >= self.capacity {
            // 通知读取线程
            condvar.notify_all();
            // 等待缓冲区读取
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
        Ok(())
    }
}

pub struct Reader {
    buffer: Arc<(Mutex<VecDeque<u8>>, Condvar)>,
    complete: Arc<RwLock<bool>>,
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let (lock, condvar) = &*self.buffer;
        let mut buffer = lock.lock();

        // 阻塞直到缓冲区有数据
        while buffer.is_empty() {
            // 写入缓冲区结束
            if *self.complete.read() {
                return Ok(0);
            }

            // 通知写入线程
            condvar.notify_all();
            // 等待缓冲区写入
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_less() {
        let mut writer = Writer::new(10);
        let mut reader = writer.make_reader();

        let _ = std::thread::spawn(move || {
            writer.write_all(b"012").unwrap();
            assert_eq!(writer.total(), 3);
        });

        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0; 10];
            assert_eq!(reader.read(&mut buf).unwrap(), 3);
            assert_eq!(reader.read(&mut buf).unwrap(), 0);
        });

        reader_thread.join().unwrap();
    }

    #[test]
    fn test_write_more() {
        let mut writer = Writer::new(3);
        let mut reader = writer.make_reader();

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
        let mut writer = Writer::new(3);
        let mut reader = writer.make_reader();

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
}
