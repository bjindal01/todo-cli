use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Why a file operation failed, worded the way the Java version worded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoFailure {
    AccessDenied,
    NoSuchFile,
    /// The Java version printed the path itself (and the second path of a move) for this case.
    AlreadyExists {
        file: PathBuf,
        other: Option<PathBuf>,
    },
    Reason(String),
}

impl IoFailure {
    /// A failure of an operation on a path (open, mkdir, stat, rename, chmod, readlink).
    pub fn on_path(err: io::Error, file: &Path) -> IoFailure {
        IoFailure::on_paths(err, file, None)
    }

    pub fn on_paths(err: io::Error, file: &Path, other: Option<&Path>) -> IoFailure {
        match err.raw_os_error() {
            Some(libc::EACCES) => IoFailure::AccessDenied,
            Some(libc::ENOENT) => IoFailure::NoSuchFile,
            Some(libc::EEXIST) => IoFailure::AlreadyExists {
                file: file.to_path_buf(),
                other: other.map(Path::to_path_buf),
            },
            Some(libc::ELOOP) => IoFailure::Reason(format!(
                "{} or unable to access attributes of symbolic link",
                os_message(&err)
            )),
            _ => IoFailure::Reason(os_message(&err)),
        }
    }

    /// A failure while reading or writing an already open file.
    pub fn on_stream(err: io::Error) -> IoFailure {
        IoFailure::Reason(os_message(&err))
    }
}

impl fmt::Display for IoFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoFailure::AccessDenied => f.write_str("permission denied"),
            IoFailure::NoSuchFile => f.write_str("no such file or folder"),
            IoFailure::AlreadyExists { file, other } => {
                write!(f, "{}", file.display())?;
                if let Some(other) = other {
                    write!(f, " -> {}", other.display())?;
                }
                Ok(())
            }
            IoFailure::Reason(reason) => f.write_str(reason),
        }
    }
}

/// The system's description of an OS error ("Is a directory"), without Rust's
/// " (os error N)" suffix.
fn os_message(err: &io::Error) -> String {
    let text = err.to_string();
    match err.raw_os_error() {
        Some(code) => text
            .strip_suffix(&format!(" (os error {code})"))
            .map(str::to_owned)
            .unwrap_or(text),
        None => text,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Lock,
    Read,
    Write,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Action::Lock => "lock",
            Action::Read => "read",
            Action::Write => "write",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Io {
        action: Action,
        file: PathBuf,
        failure: IoFailure,
    },
    NotUtf8 {
        file: PathBuf,
    },
    Corrupt {
        file: PathBuf,
        line: usize,
        reason: String,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io {
                action,
                file,
                failure,
            } => write!(f, "Could not {action} {}: {failure}.", file.display()),
            StoreError::NotUtf8 { file } => write!(
                f,
                "Could not read {}: it is not valid UTF-8 text.",
                file.display()
            ),
            StoreError::Corrupt { file, line, reason } => {
                write!(
                    f,
                    "Could not read {} line {line}: {reason}.",
                    file.display()
                )
            }
        }
    }
}

impl std::error::Error for StoreError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddError {
    EmptyText,
    NoMoreIds,
}

impl fmt::Display for AddError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AddError::EmptyText => "Task text cannot be empty.",
            AddError::NoMoreIds => "No more task ids are available.",
        })
    }
}

impl std::error::Error for AddError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskNotFound(pub i32);

impl fmt::Display for TaskNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "No task with id {}.", self.0)
    }
}

impl std::error::Error for TaskNotFound {}
