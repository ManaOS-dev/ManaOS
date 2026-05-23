use x86_64::instructions::port::Port;

const WAIT_LIMIT: usize = 100_000;

/// Initialize the PS/2 auxiliary mouse device.
pub fn init() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        let mut command_port = Port::<u8>::new(0x64);
        let mut data_port = Port::<u8>::new(0x60);
        if !wait_write() {
            return;
        }
        // SAFETY: Port 0x64 is the PS/2 command port. Interrupts are disabled
        // during controller setup.
        unsafe {
            command_port.write(0xA8_u8);
        }
        if !wait_write() {
            return;
        }
        // SAFETY: Port 0x64 is the PS/2 command port. Command 0x20 requests
        // the controller configuration byte.
        unsafe {
            command_port.write(0x20_u8);
        }
        if !wait_read() {
            return;
        }
        // SAFETY: Port 0x60 is the PS/2 data port and contains the requested
        // controller configuration byte after wait_read succeeds.
        let status = unsafe { data_port.read() | 2 };
        if !wait_write() {
            return;
        }
        // SAFETY: Port 0x64 accepts the write-configuration command.
        unsafe {
            command_port.write(0x60_u8);
        }
        if !wait_write() {
            return;
        }
        // SAFETY: Port 0x60 accepts the new controller configuration byte after
        // command 0x60 and a successful write wait.
        unsafe {
            data_port.write(status);
        }
        mouse_write(0xF6);
        let _ = mouse_read();
        mouse_write(0xF4);
        let _ = mouse_read();
    });
}

fn wait_write() -> bool {
    let mut port = Port::<u8>::new(0x64);
    for _ in 0..WAIT_LIMIT {
        // SAFETY: Port 0x64 is the PS/2 controller status port.
        if unsafe { port.read() & 2 } == 0 {
            return true;
        }
    }
    false
}

fn wait_read() -> bool {
    let mut port = Port::<u8>::new(0x64);
    for _ in 0..WAIT_LIMIT {
        // SAFETY: Port 0x64 is the PS/2 controller status port.
        if unsafe { port.read() & 1 } != 0 {
            return true;
        }
    }
    false
}

fn mouse_write(data: u8) {
    let mut command_port = Port::<u8>::new(0x64);
    let mut data_port = Port::<u8>::new(0x60);
    if !wait_write() {
        return;
    }
    // SAFETY: Port 0x64 accepts command 0xD4 to route the next data byte to the
    // auxiliary PS/2 device.
    unsafe {
        command_port.write(0xD4_u8);
    }
    if !wait_write() {
        return;
    }
    // SAFETY: Port 0x60 accepts the mouse command byte after command 0xD4 and a
    // successful write wait.
    unsafe {
        data_port.write(data);
    }
}

fn mouse_read() -> Option<u8> {
    let mut port = Port::new(0x60);
    if !wait_read() {
        return None;
    }
    // SAFETY: Port 0x60 contains a PS/2 data byte after wait_read succeeds.
    Some(unsafe { port.read() })
}
