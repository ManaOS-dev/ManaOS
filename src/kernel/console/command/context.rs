//! Shared console command helpers.

use alloc::format;
use alloc::string::String;
use spin::{LazyLock, Mutex};

static CURRENT_DIRECTORY: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::from("/")));

pub(super) fn get_current_directory() -> String {
    CURRENT_DIRECTORY.lock().clone()
}

pub(super) fn set_current_directory(path: String) {
    *CURRENT_DIRECTORY.lock() = path;
}

pub(super) fn resolve_path(path: &str) -> String {
    if path.starts_with('/') {
        crate::kernel::filesystem::normalize_path_for_display(path)
    } else {
        let current_directory = CURRENT_DIRECTORY.lock().clone();
        crate::kernel::filesystem::normalize_path_for_display(&format!(
            "{current_directory}/{path}"
        ))
    }
}

pub(super) fn file_type_label(file_type: crate::kernel::filesystem::FileType) -> &'static str {
    match file_type {
        crate::kernel::filesystem::FileType::Regular => "file",
        crate::kernel::filesystem::FileType::Directory => "dir ",
        crate::kernel::filesystem::FileType::Device => "dev ",
    }
}

pub(super) fn push_text_lines(bytes: &[u8], output: &mut super::output::CommandOutput) {
    let mut line = String::new();
    for byte in bytes {
        match *byte {
            b'\n' => {
                output.push(line.clone());
                line.clear();
            }
            b'\r' => {}
            0x20..=0x7e => line.push(char::from(*byte)),
            _ => line.push('.'),
        }
    }
    if !line.is_empty() {
        output.push(line);
    }
}

pub(super) struct HexBytes<'a>(pub(super) &'a [u8]);

impl core::fmt::Display for HexBytes<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x} ")?;
        }
        Ok(())
    }
}
