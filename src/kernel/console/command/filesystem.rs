//! Filesystem-oriented kernel console commands.

use alloc::format;
use alloc::string::{String, ToString};
use spin::{LazyLock, Mutex};

static CURRENT_DIRECTORY: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::from("/")));

pub(super) fn push_working_directory() {
    super::push_output(CURRENT_DIRECTORY.lock().clone());
}

pub(super) fn push_file_output(command_name: &str, path: &str) {
    if path.is_empty() {
        super::push_output(format!("usage: {command_name} /path"));
        return;
    }

    let path = resolve_path(path);
    let Ok(file_descriptor) = crate::kernel::filesystem::open(&path) else {
        super::push_output(format!("{command_name}: cannot open {path}"));
        return;
    };

    let mut buffer = [0_u8; 80];
    let result = crate::kernel::filesystem::read(file_descriptor, &mut buffer);
    let _ = crate::kernel::filesystem::close(file_descriptor);
    let Ok(bytes_read) = result else {
        super::push_output(format!("{command_name}: cannot read {path}"));
        return;
    };

    super::push_output(format_file_contents(&buffer[..bytes_read]));
}

pub(super) fn change_directory(path: &str) {
    let path = if path.is_empty() {
        String::from("/")
    } else {
        resolve_path(path)
    };
    match crate::kernel::filesystem::metadata(&path) {
        Ok(metadata) if metadata.file_type == crate::kernel::filesystem::FileType::Directory => {
            *CURRENT_DIRECTORY.lock() = path;
        }
        Ok(_) => super::push_output(format!("cd: not a directory: {path}")),
        Err(_) => super::push_output(format!("cd: no such directory: {path}")),
    }
}

pub(super) fn list_directory(path: &str) {
    let path = if path.is_empty() {
        CURRENT_DIRECTORY.lock().clone()
    } else {
        resolve_path(path)
    };
    let Ok(entries) = crate::kernel::filesystem::list_directory(&path) else {
        super::push_output(format!("ls: cannot list {path}"));
        return;
    };

    if entries.is_empty() {
        super::push_output(format!("{path}: empty"));
        return;
    }

    for entry in entries.iter().take(6) {
        super::push_output(format!(
            "{} {} {}",
            file_type_label(entry.metadata.file_type),
            entry.metadata.size,
            entry.name
        ));
    }
}

pub(super) fn push_stat_output(path: &str) {
    if path.is_empty() {
        super::push_output("usage: stat /path".to_string());
        return;
    }

    let path = resolve_path(path);
    match crate::kernel::filesystem::metadata(&path) {
        Ok(metadata) => super::push_output(format!(
            "{}: type={} size={} writable={}",
            path,
            file_type_label(metadata.file_type),
            metadata.size,
            metadata.writable
        )),
        Err(_) => super::push_output(format!("stat: cannot stat {path}")),
    }
}

pub(super) fn push_mounts_output() {
    let mounts = crate::kernel::filesystem::list_mounts();
    if mounts.is_empty() {
        super::push_output("mounts: none".to_string());
        return;
    }

    for mount in mounts.iter().take(6) {
        super::push_output(format!(
            "{} {:?} writable={}",
            mount.path, mount.source, mount.flags.writable
        ));
    }
}

pub(super) fn push_hexdump_output(path: &str) {
    if path.is_empty() {
        super::push_output("usage: hexdump /path".to_string());
        return;
    }

    let path = resolve_path(path);
    let Ok(file_descriptor) = crate::kernel::filesystem::open(&path) else {
        super::push_output(format!("hexdump: cannot open {path}"));
        return;
    };
    let _ = crate::kernel::filesystem::seek(file_descriptor, 0);
    let mut buffer = [0_u8; 16];
    let result = crate::kernel::filesystem::read(file_descriptor, &mut buffer);
    let _ = crate::kernel::filesystem::close(file_descriptor);
    let Ok(bytes_read) = result else {
        super::push_output(format!("hexdump: cannot read {path}"));
        return;
    };
    super::push_output(format!("0000: {}", HexBytes(&buffer[..bytes_read])));
}

fn format_file_contents(bytes: &[u8]) -> String {
    let mut output = String::new();
    for byte in bytes {
        match *byte {
            b'\n' | b'\r' => break,
            0x20..=0x7e => output.push(char::from(*byte)),
            _ => output.push('.'),
        }
    }

    output
}

fn resolve_path(path: &str) -> String {
    if path.starts_with('/') {
        crate::kernel::filesystem::normalize_path_for_display(path)
    } else {
        let current_directory = CURRENT_DIRECTORY.lock().clone();
        crate::kernel::filesystem::normalize_path_for_display(&format!(
            "{current_directory}/{path}"
        ))
    }
}

fn file_type_label(file_type: crate::kernel::filesystem::FileType) -> &'static str {
    match file_type {
        crate::kernel::filesystem::FileType::Regular => "file",
        crate::kernel::filesystem::FileType::Directory => "dir ",
        crate::kernel::filesystem::FileType::Device => "dev ",
    }
}

struct HexBytes<'a>(&'a [u8]);

impl core::fmt::Display for HexBytes<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x} ")?;
        }
        Ok(())
    }
}
