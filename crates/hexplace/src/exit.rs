//! Process exit codes for the Hexplace CLI.

use std::process::ExitCode;

use hexplace_core::CoreError;

/// Exit code for unclassified / internal failures ([`ExitCode::FAILURE`]).
pub const EXIT_INTERNAL: u8 = 1;

/// Exit code for invalid user input or configuration.
pub const EXIT_INVALID: u8 = 2;

/// Exit code for I/O failures at the process boundary.
pub const EXIT_IO: u8 = 3;

/// Maps a fatal [`CoreError`] to a stable numeric exit status.
///
/// Failure classes:
/// - [`CoreError::InvalidRequest`] → [`EXIT_INVALID`] (`2`)
/// - [`CoreError::Io`] → [`EXIT_IO`] (`3`)
/// - all other variants → [`EXIT_INTERNAL`] (`1`)
#[must_use]
pub const fn exit_status(err: &CoreError) -> u8 {
    match err {
        CoreError::InvalidRequest(_) => EXIT_INVALID,
        CoreError::Io(_) => EXIT_IO,
        CoreError::NotFound(_)
        | CoreError::Storage(_)
        | CoreError::Index(_)
        | CoreError::Import(_) => EXIT_INTERNAL,
    }
}

/// Maps a fatal [`CoreError`] to a process [`ExitCode`].
#[must_use]
pub fn exit_code_for(err: &CoreError) -> ExitCode {
    ExitCode::from(exit_status(err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_request_uses_invalid_exit_status() {
        let err = CoreError::invalid("query must not be empty");
        assert_eq!(
            exit_status(&err),
            EXIT_INVALID,
            "invalid input must use the dedicated invalid exit status"
        );
    }

    #[test]
    fn io_uses_io_exit_status() {
        let err = CoreError::io("failed to open data dir");
        assert_eq!(
            exit_status(&err),
            EXIT_IO,
            "I/O failures must use the dedicated I/O exit status"
        );
    }

    #[test]
    fn storage_index_import_and_not_found_are_internal() {
        let cases = [
            CoreError::NotFound(1),
            CoreError::storage("truncated column"),
            CoreError::index("missing field"),
            CoreError::import("no searchable places"),
        ];
        for err in cases {
            assert_eq!(
                exit_status(&err),
                EXIT_INTERNAL,
                "non-input, non-I/O failures must map to the internal exit status"
            );
        }
    }
}
