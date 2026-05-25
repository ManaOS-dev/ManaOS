# Virtual Filesystem

## Path Normalization

The kernel virtual filesystem stores paths in a canonical absolute form.

- Repeated slashes are collapsed.
- `.` path components are ignored.
- `..` removes the previous component and never escapes above `/`.
- Trailing slashes do not create a different path.
- An empty normalized result is represented as `/`.

Examples:

- `/dev//console` becomes `/dev/console`
- `/disk/../README` becomes `/README`
- `/dev/` becomes `/dev`

The console resolves relative paths against its current working directory before
passing them to the virtual filesystem.
