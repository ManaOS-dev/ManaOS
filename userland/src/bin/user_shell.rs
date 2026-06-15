#![no_main]
#![no_std]

use mana_userland::syscall;

const STDIN: usize = 0;
const STDOUT: usize = 1;
const COMMAND_BUFFER_BYTES: usize = 128;
const MAX_COMMAND_TOKENS: usize = 8;
const MAX_COMMAND_ARGUMENT_POINTERS: usize = MAX_COMMAND_TOKENS + 1;
const PWD_COMMAND: &[u8] = b"pwd";
const READY_MESSAGE: &[u8] = b"user shell ready\n";
const TOKENIZER_OK_MESSAGE: &[u8] = b"user shell tokenizer ok\n";
const PWD_OK_MESSAGE: &[u8] = b"user shell pwd ok\n";
const FILE_DEMO_LAUNCH_MESSAGE: &[u8] = b"user shell launching file_demo\n";
const FILE_DEMO_EXIT_MESSAGE: &[u8] = b"user shell file_demo exit ok\n";
const RELATIVE_FILE_DEMO_LAUNCH_MESSAGE: &[u8] = b"user shell launching relative file_demo\n";
const RELATIVE_FILE_DEMO_EXIT_MESSAGE: &[u8] = b"user shell relative file_demo exit ok\n";
const MISSING_COMMAND_OK_MESSAGE: &[u8] = b"user shell missing command not found ok\n";
const BOUNDED_ERRORS_OK_MESSAGE: &[u8] = b"user shell bounded errors ok\n";
const STDIN_EOF_MESSAGE: &[u8] = b"user shell stdin eof\n";
const READ_ERROR_MESSAGE: &[u8] = b"user shell read error\n";
const INPUT_BUFFERED_MESSAGE: &[u8] = b"user shell input buffered\n";
const EMPTY_COMMAND_MESSAGE: &[u8] = b"user shell empty command\n";
const TOKEN_LIMIT_MESSAGE: &[u8] = b"user shell token limit reached\n";
const EXECUTION_ERROR_MESSAGE: &[u8] = b"user shell execution error\n";
const PATH_TOO_LONG_MESSAGE: &[u8] = b"user shell argument buffer full\n";
const SPAWN_BAD_ADDRESS_MESSAGE: &[u8] = b"user shell spawn bad address\n";
const SPAWN_INVALID_ARGUMENT_MESSAGE: &[u8] = b"user shell spawn invalid argument\n";
const SPAWN_NOT_FOUND_MESSAGE: &[u8] = b"user shell spawn not found\n";
const SPAWN_UNSUPPORTED_MESSAGE: &[u8] = b"user shell spawn unsupported\n";
const WAIT_FAILED_MESSAGE: &[u8] = b"user shell wait failed\n";
const CHILD_FAILED_MESSAGE: &[u8] = b"user shell child failed\n";
const WORKING_DIRECTORY_FAILED_MESSAGE: &[u8] = b"user shell cwd failed\n";

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

#[derive(Clone, Copy)]
struct CommandPath {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CommandExecutionError {
    EmptyCommand,
    PathTooLong,
    TooManyTokens,
    SpawnFailed,
    SpawnBadAddress,
    SpawnInvalidArgument,
    SpawnNotFound,
    SpawnUnsupported,
    WorkingDirectoryFailed,
    WaitFailed,
    ChildFailed,
}

#[no_mangle]
extern "C" fn _start() -> ! {
    let _ = syscall::write(STDOUT, READY_MESSAGE);
    if !verify_tokenizer_smoke() {
        let _ = syscall::write(STDOUT, READ_ERROR_MESSAGE);
        syscall::exit(2);
    }
    let _ = syscall::write(STDOUT, TOKENIZER_OK_MESSAGE);
    if let Err(error) = verify_command_execution_smoke() {
        write_execution_error(error);
        syscall::exit(3);
    }

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
    if let Err(error) = execute_command(command_input) {
        if error != CommandExecutionError::EmptyCommand {
            write_execution_error(error);
            syscall::exit(3);
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

fn verify_command_execution_smoke() -> Result<(), CommandExecutionError> {
    if syscall::chdir(b"/disk\0") != 0 {
        return Err(CommandExecutionError::WorkingDirectoryFailed);
    }
    verify_pwd_builtin_smoke()?;
    let _ = syscall::write(STDOUT, FILE_DEMO_LAUNCH_MESSAGE);
    execute_command(b"/disk/bin/file_demo --shell-command-smoke")?;
    let _ = syscall::write(STDOUT, FILE_DEMO_EXIT_MESSAGE);
    let _ = syscall::write(STDOUT, RELATIVE_FILE_DEMO_LAUNCH_MESSAGE);
    execute_command(b"bin/file_demo --shell-command-smoke")?;
    let _ = syscall::write(STDOUT, RELATIVE_FILE_DEMO_EXIT_MESSAGE);
    verify_bounded_error_message_smoke()?;
    Ok(())
}

fn verify_pwd_builtin_smoke() -> Result<(), CommandExecutionError> {
    execute_command(b"pwd")?;
    let _ = syscall::write(STDOUT, PWD_OK_MESSAGE);
    Ok(())
}

fn verify_bounded_error_message_smoke() -> Result<(), CommandExecutionError> {
    verify_expected_command_error(b" \t\r\n", CommandExecutionError::EmptyCommand)?;
    verify_expected_command_error(b"0 1 2 3 4 5 6 7 8", CommandExecutionError::TooManyTokens)?;
    let long_command = [b'a'; COMMAND_BUFFER_BYTES];
    verify_expected_command_error(&long_command, CommandExecutionError::PathTooLong)?;
    verify_missing_command_smoke()?;
    let _ = syscall::write(STDOUT, BOUNDED_ERRORS_OK_MESSAGE);
    Ok(())
}

fn verify_missing_command_smoke() -> Result<(), CommandExecutionError> {
    verify_expected_command_error(
        b"bin/missing_shell_command",
        CommandExecutionError::SpawnNotFound,
    )?;
    let _ = syscall::write(STDOUT, MISSING_COMMAND_OK_MESSAGE);
    Ok(())
}

fn verify_expected_command_error(
    command: &[u8],
    expected: CommandExecutionError,
) -> Result<(), CommandExecutionError> {
    let error = execute_command(command)
        .err()
        .ok_or(CommandExecutionError::SpawnFailed)?;
    if error != expected {
        return Err(error);
    }
    write_execution_error(error);
    Ok(())
}

fn execute_command(command: &[u8]) -> Result<(), CommandExecutionError> {
    let tokens = tokenize_command(command).map_err(|_| CommandExecutionError::TooManyTokens)?;
    if tokens.len() == 0 {
        return Err(CommandExecutionError::EmptyCommand);
    }
    if execute_builtin_command(&tokens, command)? {
        return Ok(());
    }
    let mut argument_storage = [0_u8; COMMAND_BUFFER_BYTES];
    let mut argument_pointers = [core::ptr::null(); MAX_COMMAND_ARGUMENT_POINTERS];
    let command_path = build_argument_vector(
        command,
        &tokens,
        &mut argument_storage,
        &mut argument_pointers,
    )?;
    let executable_path = argument_storage
        .get(command_path.start..command_path.end)
        .ok_or(CommandExecutionError::PathTooLong)?;
    let child = syscall::spawn_with_vectors(
        executable_path,
        argument_pointers.as_ptr(),
        core::ptr::null(),
    );
    if child < 0 {
        return Err(classify_spawn_error(child));
    }

    let mut wait_status = 0_i32;
    let waited_child = syscall::waitpid(child, &mut wait_status, 0);
    if waited_child != child {
        return Err(CommandExecutionError::WaitFailed);
    }
    if wait_status != 0 {
        return Err(CommandExecutionError::ChildFailed);
    }
    Ok(())
}

fn execute_builtin_command(
    tokens: &CommandTokens,
    command: &[u8],
) -> Result<bool, CommandExecutionError> {
    let command_token = tokens.get(0).ok_or(CommandExecutionError::EmptyCommand)?;
    if command_token.as_bytes(command) != PWD_COMMAND {
        return Ok(false);
    }
    if tokens.len() != 1 {
        return Err(CommandExecutionError::TooManyTokens);
    }
    write_current_working_directory()?;
    Ok(true)
}

fn write_current_working_directory() -> Result<(), CommandExecutionError> {
    let mut directory_buffer = [0_u8; COMMAND_BUFFER_BYTES];
    let byte_count = syscall::getcwd(&mut directory_buffer);
    if byte_count <= 0 {
        return Err(CommandExecutionError::WorkingDirectoryFailed);
    }
    let byte_count = byte_count as usize;
    if byte_count > directory_buffer.len() {
        return Err(CommandExecutionError::WorkingDirectoryFailed);
    }

    let mut printable_bytes = byte_count;
    if directory_buffer
        .get(byte_count - 1)
        .is_some_and(|byte| *byte == 0)
    {
        printable_bytes -= 1;
    }
    let directory_path = directory_buffer
        .get(..printable_bytes)
        .ok_or(CommandExecutionError::WorkingDirectoryFailed)?;
    let _ = syscall::write(STDOUT, directory_path);
    let _ = syscall::write(STDOUT, b"\n");
    Ok(())
}

fn classify_spawn_error(result: isize) -> CommandExecutionError {
    match result {
        syscall::ERROR_BAD_ADDRESS => CommandExecutionError::SpawnBadAddress,
        syscall::ERROR_INVALID_ARGUMENT => CommandExecutionError::SpawnInvalidArgument,
        syscall::ERROR_NOT_FOUND => CommandExecutionError::SpawnNotFound,
        syscall::ERROR_NOT_IMPLEMENTED => CommandExecutionError::SpawnUnsupported,
        _ => CommandExecutionError::SpawnFailed,
    }
}

fn write_execution_error(error: CommandExecutionError) {
    let message = match error {
        CommandExecutionError::EmptyCommand => EMPTY_COMMAND_MESSAGE,
        CommandExecutionError::PathTooLong => PATH_TOO_LONG_MESSAGE,
        CommandExecutionError::TooManyTokens => TOKEN_LIMIT_MESSAGE,
        CommandExecutionError::SpawnBadAddress => SPAWN_BAD_ADDRESS_MESSAGE,
        CommandExecutionError::SpawnInvalidArgument => SPAWN_INVALID_ARGUMENT_MESSAGE,
        CommandExecutionError::SpawnNotFound => SPAWN_NOT_FOUND_MESSAGE,
        CommandExecutionError::SpawnUnsupported => SPAWN_UNSUPPORTED_MESSAGE,
        CommandExecutionError::WorkingDirectoryFailed => WORKING_DIRECTORY_FAILED_MESSAGE,
        CommandExecutionError::SpawnFailed => EXECUTION_ERROR_MESSAGE,
        CommandExecutionError::WaitFailed => WAIT_FAILED_MESSAGE,
        CommandExecutionError::ChildFailed => CHILD_FAILED_MESSAGE,
    };
    let _ = syscall::write(STDOUT, message);
}

fn build_argument_vector(
    command: &[u8],
    tokens: &CommandTokens,
    argument_storage: &mut [u8; COMMAND_BUFFER_BYTES],
    argument_pointers: &mut [*const u8; MAX_COMMAND_ARGUMENT_POINTERS],
) -> Result<CommandPath, CommandExecutionError> {
    let mut storage_cursor: usize = 0;
    let mut command_path = CommandPath { start: 0, end: 0 };
    for (token_index, argument_pointer) in
        argument_pointers.iter_mut().enumerate().take(tokens.len())
    {
        let token = tokens
            .get(token_index)
            .ok_or(CommandExecutionError::TooManyTokens)?;
        let token_bytes = token.as_bytes(command);
        let token_end = storage_cursor
            .checked_add(token_bytes.len())
            .ok_or(CommandExecutionError::PathTooLong)?;
        let nul_end = token_end
            .checked_add(1)
            .ok_or(CommandExecutionError::PathTooLong)?;
        if nul_end > argument_storage.len() {
            return Err(CommandExecutionError::PathTooLong);
        }
        argument_storage[storage_cursor..token_end].copy_from_slice(token_bytes);
        argument_storage[token_end] = 0;
        *argument_pointer = argument_storage[storage_cursor..].as_ptr();
        if token_index == 0 {
            command_path = CommandPath {
                start: storage_cursor,
                end: nul_end,
            };
        }
        storage_cursor = nul_end;
    }
    argument_pointers[tokens.len()] = core::ptr::null();
    Ok(command_path)
}

fn token_equals(tokens: &CommandTokens, command: &[u8], index: usize, expected: &[u8]) -> bool {
    tokens
        .get(index)
        .is_some_and(|token| token.as_bytes(command) == expected)
}
