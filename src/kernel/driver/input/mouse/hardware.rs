use x86_64::instructions::port::Port;

pub fn init() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        let mut command_port = Port::<u8>::new(0x64);
        let mut data_port = Port::<u8>::new(0x60);
        unsafe {
            wait_write();
            command_port.write(0xA8_u8);
            wait_write();
            command_port.write(0x20_u8);
            wait_read();
            let status = data_port.read() | 2;
            wait_write();
            command_port.write(0x60_u8);
            wait_write();
            data_port.write(status);
            mouse_write(0xF6);
            mouse_read();
            mouse_write(0xF4);
            mouse_read();
        }
    });
}

fn wait_write() {
    let mut port = Port::<u8>::new(0x64);
    unsafe { while (port.read() & 2) != 0 {} }
}

fn wait_read() {
    let mut port = Port::<u8>::new(0x64);
    unsafe { while (port.read() & 1) == 0 {} }
}

fn mouse_write(data: u8) {
    let mut command_port = Port::<u8>::new(0x64);
    let mut data_port = Port::<u8>::new(0x60);
    wait_write();
    unsafe { command_port.write(0xD4_u8) };
    wait_write();
    unsafe { data_port.write(data) };
}

fn mouse_read() -> u8 {
    let mut port = Port::new(0x60);
    wait_read();
    unsafe { port.read() }
}
