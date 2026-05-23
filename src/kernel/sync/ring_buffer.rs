use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

/// A fixed-size single-producer single-consumer ring buffer.
pub struct LockFreeRingBuffer<T, const N: usize> {
    buffer: [UnsafeCell<Option<T>>; N],
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl<T, const N: usize> LockFreeRingBuffer<T, N> {
    /// Create an empty ring buffer.
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

        // SAFETY: This type is used as a single-producer single-consumer queue.
        // The producer owns the head slot until publishing the new head value.
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

        // SAFETY: This type is used as a single-producer single-consumer queue.
        // The consumer owns the tail slot after observing head != tail.
        let item = unsafe { (*self.buffer[tail].get()).take() };
        self.tail.store((tail + 1) % N, Ordering::Release);
        item
    }

    #[allow(dead_code)]
    /// Return the number of queued items.
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
    /// Return whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }
}

// SAFETY: The buffer is shared only between one producer and one consumer. Slot
// ownership is coordinated by atomic head/tail indexes.
unsafe impl<T: Send, const N: usize> Sync for LockFreeRingBuffer<T, N> {}
// SAFETY: Moving the queue between contexts is safe when T is Send; interior
// mutation remains governed by the single-producer single-consumer contract.
unsafe impl<T: Send, const N: usize> Send for LockFreeRingBuffer<T, N> {}
