//! A small command-line to-do app: a Rust port of the Java `todo-cli`.
//!
//! The command-line interface, messages, exit codes and task file format are the same as the
//! Java version's, so the two can be swapped for each other and share one task file.

#[cfg(not(unix))]
compile_error!("todo-cli supports Unix-like systems only (Linux, macOS).");

pub mod app;
pub mod compat;
pub mod error;
pub mod locale;
pub mod priority;
pub mod store;
pub mod task;
pub mod todo_list;

pub use app::App;
pub use error::{AddError, IoFailure, StoreError, TaskNotFound};
pub use priority::Priority;
pub use store::TaskStore;
pub use task::Task;
pub use todo_list::TodoList;
