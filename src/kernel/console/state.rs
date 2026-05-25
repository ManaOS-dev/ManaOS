//! Kernel console input, history, and scrollback state.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::{LazyLock, Mutex};

const MAX_INPUT_BYTES: usize = 96;
const MAX_OUTPUT_LINES: usize = 128;
const VISIBLE_OUTPUT_LINES: usize = 6;
const MAX_HISTORY_ENTRIES: usize = 32;

static STATE: LazyLock<Mutex<ConsoleState>> = LazyLock::new(|| Mutex::new(ConsoleState::new()));

pub(super) struct RenderSnapshot {
    pub(super) input: String,
    pub(super) cursor: usize,
    pub(super) output: Vec<String>,
    pub(super) open: bool,
    pub(super) needs_clear: bool,
}

struct ConsoleState {
    input: String,
    cursor: usize,
    output: Vec<String>,
    history: Vec<String>,
    history_cursor: Option<usize>,
    scrollback: usize,
    dirty: bool,
    open: bool,
    needs_clear: bool,
}

impl ConsoleState {
    fn new() -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            output: Vec::new(),
            history: Vec::new(),
            history_cursor: None,
            scrollback: 0,
            dirty: false,
            open: false,
            needs_clear: false,
        }
    }
}

pub(super) fn is_open() -> bool {
    STATE.lock().open
}

pub(super) fn toggle() {
    let mut state = STATE.lock();
    state.open = !state.open;
    state.dirty = true;
    state.needs_clear = true;
}

pub(super) fn push_character(character: char) {
    if character.is_control() {
        return;
    }

    let mut state = STATE.lock();
    if state.open && state.input.len() < MAX_INPUT_BYTES {
        let cursor = state.cursor.min(state.input.len());
        state.input.insert(cursor, character);
        state.cursor = cursor.saturating_add(character.len_utf8());
        state.dirty = true;
    }
}

pub(super) fn push_backspace() {
    let mut state = STATE.lock();
    if !state.open || state.cursor == 0 {
        return;
    }

    let remove_at = previous_character_boundary(&state.input, state.cursor);
    state.input.remove(remove_at);
    state.cursor = remove_at;
    state.dirty = true;
}

pub(super) fn move_cursor_left() {
    let mut state = STATE.lock();
    state.cursor = previous_character_boundary(&state.input, state.cursor);
    state.dirty = true;
}

pub(super) fn move_cursor_right() {
    let mut state = STATE.lock();
    state.cursor = next_character_boundary(&state.input, state.cursor);
    state.dirty = true;
}

pub(super) fn move_cursor_home() {
    let mut state = STATE.lock();
    state.cursor = 0;
    state.dirty = true;
}

pub(super) fn move_cursor_end() {
    let mut state = STATE.lock();
    state.cursor = state.input.len();
    state.dirty = true;
}

pub(super) fn load_previous_history() {
    let mut state = STATE.lock();
    if state.history.is_empty() {
        return;
    }

    let next_index = state
        .history_cursor
        .map_or(state.history.len().saturating_sub(1), |index| {
            index.saturating_sub(1)
        });
    state.history_cursor = Some(next_index);
    let history_entry = state.history[next_index].clone();
    state.input = history_entry;
    state.cursor = state.input.len();
    state.dirty = true;
}

pub(super) fn load_next_history() {
    let mut state = STATE.lock();
    let Some(index) = state.history_cursor else {
        return;
    };

    let next_index = index.saturating_add(1);
    if next_index >= state.history.len() {
        state.history_cursor = None;
        state.input.clear();
    } else {
        state.history_cursor = Some(next_index);
        let history_entry = state.history[next_index].clone();
        state.input = history_entry;
    }
    state.cursor = state.input.len();
    state.dirty = true;
}

pub(super) fn scroll_up() {
    let mut state = STATE.lock();
    state.scrollback = state.scrollback.saturating_add(1).min(state.output.len());
    state.dirty = true;
}

pub(super) fn scroll_down() {
    let mut state = STATE.lock();
    state.scrollback = state.scrollback.saturating_sub(1);
    state.dirty = true;
}

pub(super) fn take_submitted_command() -> Option<String> {
    let mut state = STATE.lock();
    if !state.open {
        return None;
    }
    let command = state.input.trim().to_string();
    state.input.clear();
    state.cursor = 0;
    state.history_cursor = None;
    if !command.is_empty() {
        if state.history.len() == MAX_HISTORY_ENTRIES {
            state.history.remove(0);
        }
        state.history.push(command.clone());
    }
    state.dirty = true;
    Some(command)
}

pub(super) fn clear_output() {
    let mut state = STATE.lock();
    state.output.clear();
    state.dirty = true;
}

pub(super) fn push_output(line: String) {
    let mut state = STATE.lock();
    if state.output.len() == MAX_OUTPUT_LINES {
        state.output.remove(0);
    }
    state.output.push(line);
    state.scrollback = 0;
    state.dirty = true;
}

pub(super) fn take_render_snapshot() -> Option<RenderSnapshot> {
    let mut state = STATE.lock();
    if !state.dirty {
        return None;
    }

    state.dirty = false;
    let needs_clear = state.needs_clear;
    state.needs_clear = false;
    Some(RenderSnapshot {
        input: state.input.clone(),
        cursor: state.cursor,
        output: visible_output(&state),
        open: state.open,
        needs_clear,
    })
}

fn visible_output(state: &ConsoleState) -> Vec<String> {
    let visible_count = VISIBLE_OUTPUT_LINES.min(state.output.len());
    let end = state
        .output
        .len()
        .saturating_sub(state.scrollback.min(state.output.len()));
    let start = end.saturating_sub(visible_count);
    state.output[start..end].to_vec()
}

fn previous_character_boundary(input: &str, cursor: usize) -> usize {
    input[..cursor.min(input.len())]
        .char_indices()
        .last()
        .map_or(0, |(index, _)| index)
}

fn next_character_boundary(input: &str, cursor: usize) -> usize {
    let cursor = cursor.min(input.len());
    input[cursor..]
        .char_indices()
        .nth(1)
        .map_or(input.len(), |(index, _)| cursor + index)
}
