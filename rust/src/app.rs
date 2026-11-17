use std::env;
use std::ffi::{CStr, OsStr, OsString};
use std::fmt;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use crate::compat;
use crate::error::{AddError, StoreError, TaskNotFound};
use crate::locale;
use crate::priority::Priority;
use crate::store::TaskStore;
use crate::task::Task;

pub const OK: i32 = 0;
pub const ERROR: i32 = 1;
pub const USAGE: i32 = 2;

pub const USAGE_TEXT: &str = "\
Usage: todo <command> [arguments]

Commands:
  add [-p high|medium|low] <text...>   Add a task (priority defaults to medium;
                                       put -p before the text)
  list [--sort id|priority]            Show all tasks, by id (default) or priority
  complete <id>                        Mark a task as done
  delete <id>                          Remove a task

Tasks are stored in the file named by TODO_FILE,
or ~/.todo.tsv if TODO_FILE is not set.";

/// The task file: `TODO_FILE` if it is set and not blank, otherwise `.todo.tsv` in the user's
/// home folder.
pub fn default_file() -> PathBuf {
    file_from(env::var_os("TODO_FILE").as_deref(), home_dir)
}

pub fn file_from(todo_file: Option<&OsStr>, home: impl FnOnce() -> OsString) -> PathBuf {
    match todo_file {
        Some(configured) if !compat::is_blank(&configured.to_string_lossy()) => {
            compat::normalize(configured)
        }
        _ => compat::join(&home(), OsStr::new(".todo.tsv")),
    }
}

/// The home folder as Java's `user.home` finds it: the account's home folder from the password
/// database, falling back to `$HOME` only when that is missing or is a single character (such as
/// `/`), then to `?`.
fn home_dir() -> OsString {
    let from_passwd = passwd_home();
    if from_passwd.as_ref().is_none_or(|home| home.len() < 2) {
        if let Some(home) = env::var_os("HOME").filter(|h| !h.is_empty()) {
            return home;
        }
    }
    from_passwd.unwrap_or_else(|| OsString::from("?"))
}

fn passwd_home() -> Option<OsString> {
    let mut buf = vec![0 as libc::c_char; 1024];
    loop {
        // SAFETY: `passwd` is a plain C struct for which all-zero bytes is a valid value.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: every pointer refers to live, writable memory of the stated size.
        let rc = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                &mut entry,
                buf.as_mut_ptr(),
                buf.len(),
                &mut result,
            )
        };
        if rc == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        if rc != 0 || result.is_null() || entry.pw_dir.is_null() {
            return None;
        }
        // SAFETY: on success `pw_dir` points to a NUL-terminated string inside `buf`.
        let dir = unsafe { CStr::from_ptr(entry.pw_dir) };
        return Some(OsString::from_vec(dir.to_bytes().to_vec()));
    }
}

#[derive(Debug)]
enum Failure {
    Store(StoreError),
    Add(AddError),
    NotFound(TaskNotFound),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Store(e) => e.fmt(f),
            Failure::Add(e) => e.fmt(f),
            Failure::NotFound(e) => e.fmt(f),
        }
    }
}

impl From<StoreError> for Failure {
    fn from(e: StoreError) -> Self {
        Failure::Store(e)
    }
}

impl From<AddError> for Failure {
    fn from(e: AddError) -> Self {
        Failure::Add(e)
    }
}

impl From<TaskNotFound> for Failure {
    fn from(e: TaskNotFound) -> Self {
        Failure::NotFound(e)
    }
}

/// An exit code, or a failure that is reported as `Error: <message>` with exit code 1.
type Outcome = Result<i32, Failure>;

pub struct App<'a> {
    store: TaskStore,
    out: &'a mut dyn Write,
    err: &'a mut dyn Write,
    zero_digit: char,
}

impl<'a> App<'a> {
    pub fn new(store: TaskStore, out: &'a mut dyn Write, err: &'a mut dyn Write) -> App<'a> {
        App {
            store,
            out,
            err,
            zero_digit: '0',
        }
    }

    /// Writes task ids in `list` with the digits starting at `zero_digit`, as the Java version
    /// did in some locales (see [`crate::locale`]).
    pub fn with_zero_digit(mut self, zero_digit: char) -> App<'a> {
        self.zero_digit = zero_digit;
        self
    }

    /// Runs one command and returns its exit code.
    pub fn run<S: AsRef<str>>(&mut self, args: &[S]) -> i32 {
        let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
        let Some((&command, rest)) = args.split_first() else {
            self.err_line(USAGE_TEXT);
            return USAGE;
        };
        let outcome = match command {
            "add" => self.add(rest),
            "list" => self.list(rest),
            "complete" => self.complete(rest),
            "delete" => self.delete(rest),
            "help" | "-h" | "--help" => {
                self.out_line(USAGE_TEXT);
                Ok(OK)
            }
            _ => Ok(self.usage(&format!("Unknown command: {command}"))),
        };
        outcome.unwrap_or_else(|failure| {
            self.err_line(&format!("Error: {failure}"));
            ERROR
        })
    }

    fn add(&mut self, rest: &[&str]) -> Outcome {
        let mut priority = Priority::default();
        let mut i = 0;
        // Only the priority option is recognized; any other word, even one starting with "-",
        // begins the task text, so "todo add -5 degrees" keeps working.
        while i < rest.len() {
            let arg = rest[i];
            let value = if arg == "--" {
                i += 1;
                break;
            } else if arg == "-p" || arg == "--priority" {
                let Some(&value) = rest.get(i + 1) else {
                    return Ok(self.usage(&format!("{arg} needs a priority: high, medium or low")));
                };
                i += 2;
                value
            } else if let Some(value) = arg.strip_prefix("--priority=") {
                i += 1;
                value
            } else {
                break;
            };
            match Priority::parse(value) {
                Some(parsed) => priority = parsed,
                None => {
                    return Ok(
                        self.usage(&format!("Priority must be high, medium or low: {value}"))
                    );
                }
            }
        }
        let joined = rest[i..].join(" ");
        let text = compat::strip(&joined);
        if text.is_empty() {
            return Ok(
                self.usage("add needs the task text, for example: todo add -p high Buy milk")
            );
        }
        let task = {
            let _lock = self.store.lock()?;
            let mut list = self.store.load()?;
            let task = list.add(text, priority)?;
            self.store.save(&list)?;
            task
        };
        self.out_line(&format!(
            "Added task {} ({}): {}",
            task.id(),
            task.priority().label(),
            task.text()
        ));
        Ok(OK)
    }

    fn list(&mut self, rest: &[&str]) -> Outcome {
        let sort = match rest {
            [] => "id",
            ["--sort", value] => value,
            [arg] if arg.starts_with("--sort=") => &arg["--sort=".len()..],
            _ => return Ok(self.usage("list accepts only --sort id or --sort priority")),
        };
        let by_priority = match compat::strip(sort).to_lowercase().as_str() {
            "id" => false,
            "priority" => true,
            _ => return Ok(self.usage(&format!("Sort must be id or priority: {sort}"))),
        };
        let list = self.store.load()?;
        let tasks: Vec<&Task> = if by_priority {
            list.by_priority()
        } else {
            list.all().iter().collect()
        };
        if tasks.is_empty() {
            self.out_line("No tasks yet. Add one with: todo add <text>");
            return Ok(OK);
        }
        for task in tasks {
            let line = format_task(task, self.zero_digit);
            self.out_line(&line);
        }
        Ok(OK)
    }

    fn complete(&mut self, rest: &[&str]) -> Outcome {
        let id = match self.task_id(rest, "complete") {
            Ok(id) => id,
            Err(code) => return Ok(code),
        };
        let before = {
            let _lock = self.store.lock()?;
            let mut list = self.store.load()?;
            let before = list.complete(id)?;
            if before.done() {
                self.out_line(&format!("Task {id} is already completed."));
                return Ok(OK);
            }
            self.store.save(&list)?;
            before
        };
        self.out_line(&format!("Completed task {id}: {}", before.text()));
        Ok(OK)
    }

    fn delete(&mut self, rest: &[&str]) -> Outcome {
        let id = match self.task_id(rest, "delete") {
            Ok(id) => id,
            Err(code) => return Ok(code),
        };
        let removed = {
            let _lock = self.store.lock()?;
            let mut list = self.store.load()?;
            let removed = list.delete(id)?;
            self.store.save(&list)?;
            removed
        };
        self.out_line(&format!("Deleted task {id}: {}", removed.text()));
        Ok(OK)
    }

    /// The single task id argument, or the usage exit code after reporting what is wrong.
    fn task_id(&mut self, rest: &[&str], command: &str) -> Result<i32, i32> {
        let [arg] = rest else {
            return Err(self.usage(&format!(
                "{command} needs exactly one task id, for example: todo {command} 2"
            )));
        };
        compat::parse_int(arg)
            .ok_or_else(|| self.usage(&format!("Task id must be a number: {arg}")))
    }

    fn usage(&mut self, message: &str) -> i32 {
        self.err_line(&format!("Error: {message}"));
        self.err_line("");
        self.err_line(USAGE_TEXT);
        USAGE
    }

    // Write errors (such as a closed pipe) are ignored, as Java's System.out and System.err do,
    // so they do not change the exit code.
    fn out_line(&mut self, line: &str) {
        let _ = writeln!(self.out, "{line}");
    }

    fn err_line(&mut self, line: &str) {
        let _ = writeln!(self.err, "{line}");
    }
}

/// One line of `list` output. Line breaks in the text become spaces so each task stays on one
/// line.
pub fn format_task(task: &Task, zero_digit: char) -> String {
    let text = task.text().replace(['\r', '\n'], " ");
    format!(
        "{}. [{}] ({}) {}",
        locale::localize_digits(&format!("{:3}", task.id()), zero_digit),
        if task.done() { "x" } else { " " },
        task.priority().label(),
        text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_file_wins_unless_blank() {
        let home = || OsString::from("/home/me");
        assert_eq!(
            file_from(Some(OsStr::new("/tmp//x.tsv/")), home),
            PathBuf::from("/tmp/x.tsv")
        );
        assert_eq!(
            file_from(Some(OsStr::new(" \t")), home),
            PathBuf::from("/home/me/.todo.tsv")
        );
        assert_eq!(file_from(None, home), PathBuf::from("/home/me/.todo.tsv"));
        assert_eq!(
            file_from(None, || OsString::from("/")),
            PathBuf::from("/.todo.tsv")
        );
    }

    #[test]
    fn format_pads_id_and_flattens_line_breaks() {
        let task = Task::new(7, "a\r\nb", true, Priority::High);
        assert_eq!(format_task(&task, '0'), "  7. [x] (high) a  b");
        let task = Task::new(12345, "x", false, Priority::Low);
        assert_eq!(format_task(&task, '0'), "12345. [ ] (low) x");
        let task = Task::new(12, "x", false, Priority::Low);
        assert_eq!(
            format_task(&task, '\u{660}'),
            " \u{661}\u{662}. [ ] (low) x"
        );
    }
}
