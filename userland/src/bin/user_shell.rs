#![no_main]
#![no_std]

use mana_userland::syscall;

const STDIN: usize = 0;
const STDOUT: usize = 1;
const COMMAND_BUFFER_BYTES: usize = 128;
const MAX_COMMAND_TOKENS: usize = 8;
const READY_MESSAGE: &[u8] = b"user shell ready\n";
const TOKENIZER_OK_MESSAGE: &[u8] = b"user shell tokenizer ok\n";
const STDIN_EOF_MESSAGE: &[u8] = b"user shell stdin eof\n";
const READ_ERROR_MESSAGE: &[u8] = b"user shell read error\n";
const INPUT_BUFFERED_MESSAGE: &[u8] = b"user shell input buffered\n";
const EMPTY_COMMAND_MESSAGE: &[u8] = b"user shell empty command\n";
const TOKEN_LIMIT_MESSAGE: &[u8] = b"user shell token limit reached\n";

#[derive(Clone, Copy)]
struct CommandToken {
    start: usize,
    end: usize,
}

impl CommandToken {
    const fn empty() -> Self {
        Self { start: 0, end: 0 }
    }

    fn as_bytes<'a>(&self, command: &'a [u8]) -> &'a [u8] {
        command.get(self.start..self.end).unwrap_or(&[])
    }
}

struct CommandTokens {
    tokens: [CommandToken; MAX_COMMAND_TOKENS],
    count: usize,
}

impl CommandTokens {
    const fn new() -> Self {
        Self {
            tokens: [CommandToken::empty(); MAX_COMMAND_TOKENS],
            count: 0,
        }
    }

    fn push(&mut self, token: CommandToken) -> Result<(), TokenizeError> {
        if self.count == self.tokens.len() {
            return Err(TokenizeError::TooManyTokens);
        }
        self.tokens[self.count] = token;
        self.count += 1;
        Ok(())
    }

    fn len(&self) -> usize {
        self.count
    }

    fn get(&self, index: usize) -> Option<CommandToken> {
        if index < self.count {
            Some(self.tokens[index])
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TokenizeError {
    TooManyTokens,
}

#[no_mangle]
extern "C" fn _start() -> ! {
    let _ = syscall::write(STDOUT, READY_MESSAGE);
    if !verify_tokenizer_smoke() {
        let _ = syscall::write(STDOUT, READ_ERROR_MESSAGE);
        syscall::exit(2);
    }
    let _ = syscall::write(STDOUT, TOKENIZER_OK_MESSAGE);

    let mut command_buffer = [0_u8; COMMAND_BUFFER_BYTES];
    let bytes_read = syscall::read(STDIN, &mut command_buffer);
    if bytes_read < 0 {
        let _ = syscall::write(STDOUT, READ_ERROR_MESSAGE);
        syscall::exit(1);
    }
    if bytes_read == 0 {
        let _ = syscall::write(STDOUT, STDIN_EOF_MESSAGE);
        syscall::exit(0);
    }
    let bytes_read = bytes_read as usize;
    if bytes_read > command_buffer.len() {
        let _ = syscall::write(STDOUT, READ_ERROR_MESSAGE);
        syscall::exit(1);
    }
    let Some(command_input) = command_buffer.get(..bytes_read) else {
        let _ = syscall::write(STDOUT, READ_ERROR_MESSAGE);
        syscall::exit(1);
    };
    match tokenize_command(command_input) {
        Ok(tokens) if tokens.len() == 0 => {
            let _ = syscall::write(STDOUT, EMPTY_COMMAND_MESSAGE);
        }
        Ok(_) => {
            let _ = syscall::write(STDOUT, INPUT_BUFFERED_MESSAGE);
        }
        Err(TokenizeError::TooManyTokens) => {
            let _ = syscall::write(STDOUT, TOKEN_LIMIT_MESSAGE);
            syscall::exit(2);
        }
    }
    syscall::exit(0);
}

fn tokenize_command(command: &[u8]) -> Result<CommandTokens, TokenizeError> {
    let mut tokens = CommandTokens::new();
    let mut cursor = 0;
    while cursor < command.len() {
        while command
            .get(cursor)
            .is_some_and(|byte| is_shell_whitespace(*byte))
        {
            cursor += 1;
        }
        if cursor == command.len() {
            break;
        }

        let token_start = cursor;
        while command
            .get(cursor)
            .is_some_and(|byte| !is_shell_whitespace(*byte))
        {
            cursor += 1;
        }
        tokens.push(CommandToken {
            start: token_start,
            end: cursor,
        })?;
    }
    Ok(tokens)
}

fn is_shell_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn verify_tokenizer_smoke() -> bool {
    let command = b"  file_demo\t--flag\r\n/disk/hello.txt  ";
    let Ok(tokens) = tokenize_command(command) else {
        return false;
    };
    if tokens.len() != 3 {
        return false;
    }
    if !token_equals(&tokens, command, 0, b"file_demo") {
        return false;
    }
    if !token_equals(&tokens, command, 1, b"--flag") {
        return false;
    }
    if !token_equals(&tokens, command, 2, b"/disk/hello.txt") {
        return false;
    }

    let Ok(empty_tokens) = tokenize_command(b" \t\r\n ") else {
        return false;
    };
    if empty_tokens.len() != 0 {
        return false;
    }

    matches!(
        tokenize_command(b"0 1 2 3 4 5 6 7 8"),
        Err(TokenizeError::TooManyTokens)
    )
}

fn token_equals(
    tokens: &CommandTokens,
    command: &[u8],
    index: usize,
    expected: &[u8],
) -> bool {
    tokens
        .get(index)
        .is_some_and(|token| token.as_bytes(command) == expected)
}
