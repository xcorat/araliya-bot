# Logger CLI file sink (`--log-file`)

Date: 2026-02-25

## Summary
- Added CLI-only logging sink override: `--log-file <PATH>`.
- Logging remains global/bootstrap-level infrastructure (`bootstrap::logger`), not a manager/subsystem component.
- When provided, logs are appended to the target file; otherwise logs continue to stderr.

## Wiring
- CLI parse: `crates/araliya-bot/src/main.rs` (`CliArgs`, `parse_cli_args`).
- Startup callsite: `crates/araliya-bot/src/main.rs` now passes optional path into logger init.
- Logger sink selection: `crates/araliya-bot/src/bootstrap/logger.rs`.

## Behavior
- File open mode: create + append.
- Startup fails with `AppError::Logger` if the file cannot be opened.
- Existing verbosity precedence (`-v` vs `RUST_LOG` vs config) is unchanged.
