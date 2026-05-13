use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct LockFreeRingBuffer<T, const N: usize> {
    buffer: [UnsafeCell<Option<T>>; N],
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl<T, const N: usize> LockFreeRingBuffer<T, N> {
    pub const fn new() -> Self {
        Self {
            buffer: [const { UnsafeCell::new(None) }; N],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, item: T) -> Result<(), T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        let next_head = (head + 1) % N;
        if next_head == tail {
            return Err(item);
        }

        unsafe {
            *self.buffer[head].get() = Some(item);
        }
        self.head.store(next_head, Ordering::Release);
        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        let item = unsafe { (*self.buffer[tail].get()).take() };
        self.tail.store((tail + 1) % N, Ordering::Release);
        item
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        if head >= tail {
            head - tail
        } else {
            N - (tail - head)
        }
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }
}

// Ensure the buffer is Sync so it can be shared between threads/interrupts.
// This is safe because we use AtomicUsize for head/tail and single-producer/single-consumer logic.
unsafe impl<T: Send, const N: usize> Sync for LockFreeRingBuffer<T, N> {}
unsafe impl<T: Send, const N: usize> Send for LockFreeRingBuffer<T, N> {}
