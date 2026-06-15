//! Keyboard-backed stdin byte queue.

use crate::kernel::sync::ring_buffer::LockFreeRingBuffer;

static STDIN_QUEUE: LockFreeRingBuffer<u8, 256> = LockFreeRingBuffer::new();

/// Push one byte into keyboard-backed standard input.
pub(super) fn push_byte(byte: u8) {
    let _ = STDIN_QUEUE.push(byte);
}

/// Push a decoded character into keyboard-backed standard input.
pub(super) fn push_character(character: char) {
    if character == '\r' {
        push_byte(b'\n');
        return;
    }

    let mut encoded = [0_u8; 4];
    push_bytes(character.encode_utf8(&mut encoded).as_bytes());
}

/// Push a byte slice into keyboard-backed standard input.
pub(super) fn push_bytes(bytes: &[u8]) {
    for byte in bytes {
        push_byte(*byte);
    }
}

/// Clear queued keyboard-backed standard input bytes.
pub(super) fn clear_buffer() {
    while STDIN_QUEUE.pop().is_some() {}
}

/// Drain queued keyboard-backed standard input bytes into `buffer`.
pub(super) fn get_bytes(buffer: &mut [u8]) -> usize {
    let mut bytes_read = 0;
    for byte in buffer {
        let Some(queued_byte) = STDIN_QUEUE.pop() else {
            break;
        };
        *byte = queued_byte;
        bytes_read += 1;
    }
    bytes_read
}
