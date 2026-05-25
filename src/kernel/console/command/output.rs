//! Console command output and error types.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub(super) enum CommandEffect {
    Output(CommandOutput),
    Clear,
}

pub(super) struct CommandOutput {
    lines: Vec<String>,
}

impl CommandOutput {
    pub(super) fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub(super) fn single(line: String) -> Self {
        let mut output = Self::new();
        output.push(line);
        output
    }

    pub(super) fn push(&mut self, line: String) {
        self.lines.push(line);
    }

    pub(super) fn lines(&self) -> &[String] {
        &self.lines
    }
}

pub(super) enum CommandError {
    EmptyCommand,
    UnknownCommand,
    MissingArgument(&'static str),
    TooManyPipes,
    NotPipeable(&'static str),
    FileOpenFailed(String),
    FileReadFailed(String),
    DirectoryListFailed(String),
    StatFailed(String),
}

impl CommandError {
    pub(super) fn message(self, command: &str) -> String {
        match self {
            Self::EmptyCommand => "empty command".to_string(),
            Self::UnknownCommand => format!("unknown command: {command}"),
            Self::MissingArgument(command_name) => format!("usage: {command_name} /path"),
            Self::TooManyPipes => "only one pipe is supported".to_string(),
            Self::NotPipeable(command_name) => format!("{command_name}: cannot run in a pipe"),
            Self::FileOpenFailed(path) => format!("cannot open {path}"),
            Self::FileReadFailed(path) => format!("cannot read {path}"),
            Self::DirectoryListFailed(path) => format!("ls: cannot list {path}"),
            Self::StatFailed(path) => format!("stat: cannot stat {path}"),
        }
    }
}
