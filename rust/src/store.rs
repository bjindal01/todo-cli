//! Reads and writes tasks as UTF-8 text. A file starts with two header lines, `#version<TAB>2`
//! and `#next<TAB>id` (the next id to hand out), followed by one task per line:
//! `id<TAB>done<TAB>priority<TAB>text`. Backslashes, tabs and line breaks in the text are escaped
//! so each task stays on one line.
//!
//! Older files still load. Files without `#version` may have 3-field task lines
//! (`id<TAB>done<TAB>text`, read as medium priority) and may lack `#next`, in which case the next
//! id follows the highest task id.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, Permissions};
use std::hash::{BuildHasher, RandomState};
use std::io::{self, ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::compat;
use crate::error::{Action, IoFailure, StoreError};
use crate::priority::Priority;
use crate::task::Task;
use crate::todo_list::{MAX_NEXT_ID, TodoList};

const VERSION_PREFIX: &str = "#version\t";
const NEXT_ID_PREFIX: &str = "#next\t";
const VERSION: &str = "2";
const MAX_SYMLINK_HOPS: usize = 40;

#[derive(Debug, Clone)]
pub struct TaskStore {
    file: PathBuf,
}

/// An exclusive lock on the task file, released when dropped.
#[derive(Debug)]
#[must_use = "the lock is released as soon as it is dropped"]
pub struct StoreLock {
    _file: File,
}

impl TaskStore {
    pub fn new(file: impl Into<PathBuf>) -> TaskStore {
        TaskStore { file: file.into() }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Takes an exclusive lock that other todo processes (Rust or Java) also take, so a
    /// load-change-save sequence cannot overwrite another process's save. Blocks until the lock
    /// is free. The lock lives in a `.lock` file next to the task file, which is left in place
    /// because deleting it while another process waits on it would break the exclusion.
    pub fn lock(&self) -> Result<StoreLock, StoreError> {
        self.take_lock()
            .map_err(|failure| self.error(Action::Lock, failure))
    }

    fn take_lock(&self) -> Result<StoreLock, IoFailure> {
        let target = self.target()?;
        let Some(name) = compat::file_name(&target) else {
            return Err(no_file_name());
        };
        let mut lock_name = name.to_os_string();
        lock_name.push(".lock");
        let lock_path = compat::resolve_sibling(&target, Path::new(&lock_name));
        if let Some(dir) = compat::parent(&lock_path) {
            create_directories(&dir)?;
        }
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| IoFailure::on_path(e, &lock_path))?;
        lock_exclusive(&file).map_err(IoFailure::on_stream)?;
        Ok(StoreLock { _file: file })
    }

    pub fn load(&self) -> Result<TodoList, StoreError> {
        if fs::metadata(&self.file).is_err() {
            return Ok(TodoList::new());
        }
        let bytes = read_file(&self.file).map_err(|f| self.error(Action::Read, f))?;
        let text = String::from_utf8(bytes).map_err(|_| StoreError::NotUtf8 {
            file: self.file.clone(),
        })?;
        let mut versioned = false;
        let mut next_id = 1;
        let mut tasks = Vec::new();
        let mut ids = HashSet::new();
        for (index, line) in compat::lines(&text).enumerate() {
            let line_number = index + 1;
            if compat::is_blank(line) {
                continue;
            }
            if let Some(version) = line.strip_prefix(VERSION_PREFIX) {
                if version != VERSION {
                    return Err(
                        self.corrupt(line_number, format!("unsupported file version {version}"))
                    );
                }
                versioned = true;
                continue;
            }
            if let Some(value) = line.strip_prefix(NEXT_ID_PREFIX) {
                next_id = self.parse_next_id(value, line_number)?;
                continue;
            }
            let task = self.parse_task(line, line_number, versioned)?;
            if !ids.insert(task.id()) {
                return Err(self.corrupt(line_number, format!("duplicate id {}", task.id())));
            }
            tasks.push(task);
        }
        Ok(TodoList::with_next_id(tasks, next_id))
    }

    pub fn save(&self, list: &TodoList) -> Result<(), StoreError> {
        self.write_atomically(serialize(list).as_bytes())
            .map_err(|failure| self.error(Action::Write, failure))
    }

    fn write_atomically(&self, content: &[u8]) -> Result<(), IoFailure> {
        // Replace the file a symlink points to, not the symlink itself.
        let target = self.target()?;
        let (Some(dir), Some(name)) = (compat::parent(&target), compat::file_name(&target)) else {
            return Err(no_file_name());
        };
        create_directories(&dir)?;
        // Write to a sibling temp file first so a crash mid-write cannot truncate the existing list.
        let mut temp = TempFile::create(&dir, name)?;
        temp.write_all(content)?;
        copy_permissions(&target, &temp.path)?;
        temp.persist(&target)
    }

    /// The real file behind the configured path, following symlinks even when the final target
    /// does not exist yet.
    fn target(&self) -> Result<PathBuf, IoFailure> {
        let mut path = compat::to_absolute(&self.file);
        let mut hops = 0;
        while fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            if hops == MAX_SYMLINK_HOPS {
                return Err(IoFailure::Reason(
                    "too many levels of symbolic links".to_owned(),
                ));
            }
            let link = fs::read_link(&path).map_err(|e| IoFailure::on_path(e, &path))?;
            path = compat::resolve_sibling(&path, &link);
            hops += 1;
        }
        Ok(path)
    }

    fn parse_task(
        &self,
        line: &str,
        line_number: usize,
        versioned: bool,
    ) -> Result<Task, StoreError> {
        // Task text never contains a raw tab (it is escaped), so in unversioned files the field
        // count tells 3-field lines (before priorities) from 4-field lines.
        let parts: Vec<&str> = line.splitn(4, '\t').collect();
        if versioned && parts.len() != 4 {
            return Err(self.corrupt(line_number, "expected 4 tab-separated fields"));
        }
        if parts.len() != 3 && parts.len() != 4 {
            return Err(self.corrupt(line_number, "expected 3 or 4 tab-separated fields"));
        }
        let id = compat::parse_int(parts[0])
            .ok_or_else(|| self.corrupt(line_number, "id is not a number"))?;
        if id < 1 {
            return Err(self.corrupt(line_number, "id must be positive"));
        }
        let done = match parts[1] {
            "1" => true,
            "0" => false,
            _ => return Err(self.corrupt(line_number, "done flag must be 0 or 1")),
        };
        if parts.len() == 3 {
            return Ok(Task::new(id, unescape(parts[2]), done, Priority::Medium));
        }
        let priority = Priority::parse(parts[2])
            .ok_or_else(|| self.corrupt(line_number, "priority must be high, medium or low"))?;
        Ok(Task::new(id, unescape(parts[3]), done, priority))
    }

    fn parse_next_id(&self, value: &str, line_number: usize) -> Result<i64, StoreError> {
        let next_id = compat::parse_long(value)
            .ok_or_else(|| self.corrupt(line_number, "next id is not a number"))?;
        if next_id < 1 {
            return Err(self.corrupt(line_number, "next id must be positive"));
        }
        if next_id > MAX_NEXT_ID {
            return Err(self.corrupt(line_number, "next id is too large"));
        }
        Ok(next_id)
    }

    fn corrupt(&self, line: usize, reason: impl Into<String>) -> StoreError {
        StoreError::Corrupt {
            file: self.file.clone(),
            line,
            reason: reason.into(),
        }
    }

    fn error(&self, action: Action, failure: IoFailure) -> StoreError {
        StoreError::Io {
            action,
            file: self.file.clone(),
            failure,
        }
    }
}

fn serialize(list: &TodoList) -> String {
    let mut out = format!(
        "{VERSION_PREFIX}{VERSION}\n{NEXT_ID_PREFIX}{}\n",
        list.next_id()
    );
    for task in list.all() {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            task.id(),
            if task.done() { '1' } else { '0' },
            task.priority().label(),
            escape(task.text())
        ));
    }
    out
}

fn no_file_name() -> IoFailure {
    IoFailure::Reason("the path does not name a file".to_owned())
}

fn read_file(path: &Path) -> Result<Vec<u8>, IoFailure> {
    let mut file = File::open(path).map_err(|e| IoFailure::on_path(e, path))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(IoFailure::on_stream)?;
    Ok(bytes)
}

/// Waits for an exclusive `fcntl` record lock on the whole file. Java's `FileChannel.lock` takes
/// the same kind of lock, so the Rust and Java versions exclude each other; `flock` locks (what
/// `File::lock` uses on Linux) would not.
fn lock_exclusive(file: &File) -> io::Result<()> {
    // SAFETY: `flock` is a plain C struct for which all-zero bytes is a valid value.
    let mut request: libc::flock = unsafe { std::mem::zeroed() };
    request.l_type = libc::F_WRLCK as _;
    request.l_whence = libc::SEEK_SET as _;
    loop {
        // SAFETY: the descriptor stays open for the duration of the call and `request` is a
        // valid, initialized `flock`.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLKW, &request) } != -1 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if err.kind() != ErrorKind::Interrupted {
            return Err(err);
        }
    }
}

/// Java's `Files.createDirectories`, including how it reports a path that exists but is not a
/// folder, so failures are worded as before.
fn create_directories(dir: &Path) -> Result<(), IoFailure> {
    match create_and_check_is_directory(dir) {
        Ok(()) => return Ok(()),
        Err(e @ IoFailure::AlreadyExists { .. }) => return Err(e),
        Err(_) => {}
    }
    let dir = compat::to_absolute(dir);
    let mut existing = compat::parent(&dir);
    let base = loop {
        let Some(candidate) = existing else {
            return Err(IoFailure::Reason(
                "Unable to determine if root directory exists".to_owned(),
            ));
        };
        match fs::metadata(&candidate) {
            Ok(_) => break candidate,
            Err(e) if e.kind() == ErrorKind::NotFound => existing = compat::parent(&candidate),
            Err(e) => return Err(IoFailure::on_path(e, &candidate)),
        }
    };
    let rest = dir
        .strip_prefix(&base)
        .expect("an ancestor found by walking up is a prefix");
    let mut child = base;
    for name in rest.iter() {
        child = compat::resolve(&child, Path::new(name));
        create_and_check_is_directory(&child)?;
    }
    Ok(())
}

fn create_and_check_is_directory(dir: &Path) -> Result<(), IoFailure> {
    match fs::create_dir(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            if fs::metadata(dir).is_ok_and(|m| m.is_dir()) {
                Ok(())
            } else {
                Err(IoFailure::AlreadyExists {
                    file: dir.to_path_buf(),
                    other: None,
                })
            }
        }
        Err(e) => Err(IoFailure::on_path(e, dir)),
    }
}

/// New temp files are private (0600); keep the permissions the task file already had.
fn copy_permissions(from: &Path, to: &Path) -> Result<(), IoFailure> {
    let Ok(metadata) = fs::metadata(from) else {
        return Ok(());
    };
    let mode = metadata.permissions().mode() & 0o777;
    fs::set_permissions(to, Permissions::from_mode(mode)).map_err(|e| IoFailure::on_path(e, to))
}

/// A private temp file next to the task file, removed on drop unless it was moved into place.
struct TempFile {
    path: PathBuf,
    file: Option<File>,
}

impl TempFile {
    fn create(dir: &Path, prefix: &std::ffi::OsStr) -> Result<TempFile, IoFailure> {
        loop {
            let mut name = OsString::from(prefix);
            name.push(format!("{}.tmp", random_number()));
            let path = compat::resolve(dir, Path::new(&name));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(TempFile {
                        path,
                        file: Some(file),
                    });
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(IoFailure::on_path(e, &path)),
            }
        }
    }

    fn write_all(&mut self, content: &[u8]) -> Result<(), IoFailure> {
        let mut file = self.file.take().expect("written once");
        file.write_all(content).map_err(IoFailure::on_stream)
    }

    fn persist(mut self, target: &Path) -> Result<(), IoFailure> {
        fs::rename(&self.path, target)
            .map_err(|e| IoFailure::on_paths(e, &self.path, Some(target)))?;
        self.path = PathBuf::new();
        Ok(())
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn random_number() -> u64 {
    RandomState::new().hash_one((std::process::id(), SystemTime::now()))
}

/// Escapes backslashes, tabs and line breaks so the text fits on one tab-separated line.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

/// Reverses [`escape`]. A backslash before any other character stands for that character, and a
/// trailing lone backslash is kept.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}
