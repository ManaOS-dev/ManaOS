//! `mounts` kernel console command.

use super::output::{CommandEffect, CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if !arguments.is_empty() {
        return Err(CommandError::UnknownCommand);
    }

    let mut output = CommandOutput::new();
    let mounts = crate::kernel::filesystem::list_mounts();
    if mounts.is_empty() {
        output.push(alloc::string::ToString::to_string("mounts: none"));
        return Ok(CommandEffect::Output(output));
    }

    for mount in mounts.iter().take(6) {
        output.push(alloc::format!(
            "{} {:?} writable={}",
            mount.path,
            mount.source,
            mount.flags.writable
        ));
    }
    Ok(CommandEffect::Output(output))
}
