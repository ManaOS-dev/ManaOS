//! Keyboard scancode queue.

use crate::kernel::sync::ring_buffer::LockFreeRingBuffer;

static SCANCODE_QUEUE: LockFreeRingBuffer<u8, 128> = LockFreeRingBuffer::new();

/// Push one scancode into the keyboard queue.
pub(super) fn push_scancode(scancode: u8) {
    let _ = SCANCODE_QUEUE.push(scancode);
}

/// Pop one scancode from the keyboard queue.
pub(super) fn pop_scancode() -> Option<u8> {
    SCANCODE_QUEUE.pop()
}
