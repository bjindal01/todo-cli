//! Port of the Java `TaskStoreTest`.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

use tempfile::TempDir;
use todo_cli::store::{escape, unescape};
use todo_cli::todo_list::MAX_NEXT_ID;
use todo_cli::{Priority, Task, TaskStore, TodoList};

fn medium(id: i32, text: &str, done: bool) -> Task {
    Task::new(id, text, done, Priority::Medium)
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn running_as_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

#[test]
fn missing_file_loads_as_empty_list() {
    let dir = TempDir::new().unwrap();
    let store = TaskStore::new(dir.path().join("none.tsv"));

    assert!(store.load().unwrap().all().is_empty());
}

#[test]
fn save_then_load_round_trips_tasks() {
    let dir = TempDir::new().unwrap();
    let store = TaskStore::new(dir.path().join("tasks.tsv"));
    let mut list = TodoList::new();
    list.add("Buy milk", Priority::Medium).unwrap();
    list.add("Pay rent", Priority::Medium).unwrap();
    list.complete(1).unwrap();

    store.save(&list).unwrap();

    assert_eq!(
        store.load().unwrap().all(),
        [medium(1, "Buy milk", true), medium(2, "Pay rent", false)]
    );
}

#[test]
fn text_with_tabs_newlines_backslashes_and_unicode_survives_round_trip() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    let store = TaskStore::new(&file);
    let tricky = "a\tb\nc\\d\\te \r café ✓";
    let list = TodoList::with_tasks(vec![medium(1, tricky, false)]);

    store.save(&list).unwrap();

    // Two header lines plus one task line: the escaping kept the task on a single line.
    assert_eq!(lines(&file).len(), 3);
    assert_eq!(store.load().unwrap().all()[0].text(), tricky);
}

#[test]
fn save_creates_missing_parent_directories_and_leaves_no_temp_files() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("nested/deeper/tasks.tsv");
    let store = TaskStore::new(&file);
    let mut list = TodoList::new();
    list.add("x", Priority::Medium).unwrap();

    store.save(&list).unwrap();
    store.save(&list).unwrap();

    assert!(file.exists());
    let entries: Vec<_> = fs::read_dir(file.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries, [file]);
}

#[test]
fn blank_lines_are_ignored() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "\n1\t0\tone\n\n2\t1\ttwo\n").unwrap();

    assert_eq!(
        TaskStore::new(&file).load().unwrap().all(),
        [medium(1, "one", false), medium(2, "two", true)]
    );
}

#[test]
fn corrupt_line_fails_with_line_number() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "1\t0\tone\nnot-a-task\n").unwrap();

    let e = TaskStore::new(&file).load().unwrap_err().to_string();
    assert!(e.contains("line 2"), "{e}");
}

#[test]
fn invalid_done_flag_and_id_are_rejected() {
    let dir = TempDir::new().unwrap();
    let bad_flag = dir.path().join("flag.tsv");
    fs::write(&bad_flag, "1\tyes\tone\n").unwrap();
    let bad_id = dir.path().join("id.tsv");
    fs::write(&bad_id, "0\t0\tone\n").unwrap();

    assert!(TaskStore::new(&bad_flag).load().is_err());
    assert!(TaskStore::new(&bad_id).load().is_err());
}

#[test]
fn next_id_survives_deleting_the_newest_task() {
    let dir = TempDir::new().unwrap();
    let store = TaskStore::new(dir.path().join("tasks.tsv"));
    let mut list = TodoList::new();
    list.add("one", Priority::Medium).unwrap();
    list.add("two", Priority::Medium).unwrap();
    list.delete(2).unwrap();

    store.save(&list).unwrap();
    let mut reloaded = store.load().unwrap();

    assert_eq!(reloaded.all(), [medium(1, "one", false)]);
    assert_eq!(reloaded.add("three", Priority::Medium).unwrap().id(), 3);
}

#[test]
fn file_without_next_id_line_loads_with_id_after_highest_task() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "1\t0\tone\n4\t1\tfour\n").unwrap();

    let list = TaskStore::new(&file).load().unwrap();

    assert_eq!(
        list.all(),
        [medium(1, "one", false), medium(4, "four", true)]
    );
    assert_eq!(list.next_id(), 5);
}

#[test]
fn saved_file_starts_with_version_and_next_id_lines() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    let store = TaskStore::new(&file);
    let mut list = TodoList::new();
    list.add("one", Priority::Medium).unwrap();

    store.save(&list).unwrap();

    assert_eq!(
        lines(&file),
        ["#version\t2", "#next\t2", "1\t0\tmedium\tone"]
    );
}

#[test]
fn invalid_next_id_line_is_rejected_with_line_number() {
    let dir = TempDir::new().unwrap();
    let not_number = dir.path().join("nan.tsv");
    fs::write(&not_number, "#next\tabc\n1\t0\tone\n").unwrap();
    let not_positive = dir.path().join("zero.tsv");
    fs::write(&not_positive, "1\t0\tone\n#next\t0\n").unwrap();

    let e1 = TaskStore::new(&not_number).load().unwrap_err().to_string();
    assert!(e1.contains("line 1"), "{e1}");
    let e2 = TaskStore::new(&not_positive)
        .load()
        .unwrap_err()
        .to_string();
    assert!(e2.contains("line 2"), "{e2}");
}

#[test]
fn priority_round_trips() {
    let dir = TempDir::new().unwrap();
    let store = TaskStore::new(dir.path().join("tasks.tsv"));
    let mut list = TodoList::new();
    list.add("a", Priority::High).unwrap();
    list.add("b", Priority::Medium).unwrap();
    list.add("c", Priority::Low).unwrap();

    store.save(&list).unwrap();

    assert_eq!(
        store.load().unwrap().all(),
        [
            Task::new(1, "a", false, Priority::High),
            Task::new(2, "b", false, Priority::Medium),
            Task::new(3, "c", false, Priority::Low),
        ]
    );
}

#[test]
fn lines_without_priority_load_as_medium_and_can_be_mixed_with_new_lines() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "#next\t4\n1\t0\told task\n2\t1\thigh\tnew task\n").unwrap();

    let list = TaskStore::new(&file).load().unwrap();

    assert_eq!(
        list.all(),
        [
            medium(1, "old task", false),
            Task::new(2, "new task", true, Priority::High)
        ]
    );
}

#[test]
fn legacy_file_is_rewritten_with_priority_on_save() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "1\t0\told task\n").unwrap();
    let store = TaskStore::new(&file);

    store.save(&store.load().unwrap()).unwrap();

    assert_eq!(
        lines(&file),
        ["#version\t2", "#next\t2", "1\t0\tmedium\told task"]
    );
}

#[test]
fn invalid_priority_is_rejected_with_line_number() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "1\t0\thigh\tok\n2\t0\turgent\tbad\n").unwrap();

    let e = TaskStore::new(&file).load().unwrap_err().to_string();
    assert!(e.contains("line 2"), "{e}");
    assert!(e.contains("priority"), "{e}");
}

#[test]
fn duplicate_ids_are_rejected_with_line_number() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "1\t0\tx\n1\t0\ty\n").unwrap();

    let e = TaskStore::new(&file).load().unwrap_err().to_string();
    assert!(e.contains("line 2: duplicate id 1"), "{e}");
}

#[test]
fn invalid_utf8_is_reported_with_file_name() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, [0xFF, b'\n']).unwrap();

    let e = TaskStore::new(&file).load().unwrap_err().to_string();
    assert_eq!(
        e,
        format!(
            "Could not read {}: it is not valid UTF-8 text.",
            file.display()
        )
    );
}

#[test]
fn unreadable_file_is_reported_with_reason() {
    if running_as_root() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "1\t0\tx\n").unwrap();
    chmod(&file, 0o000);

    let e = TaskStore::new(&file).load().unwrap_err().to_string();
    assert_eq!(
        e,
        format!("Could not read {}: permission denied.", file.display())
    );
}

#[test]
fn unwritable_folder_is_reported_with_reason() {
    if running_as_root() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("locked");
    fs::create_dir(&folder).unwrap();
    let file = folder.join("tasks.tsv");
    chmod(&folder, 0o555);
    let mut list = TodoList::new();
    list.add("x", Priority::Medium).unwrap();

    let e = TaskStore::new(&file).save(&list).unwrap_err().to_string();
    chmod(&folder, 0o755);
    assert_eq!(
        e,
        format!("Could not write {}: permission denied.", file.display())
    );
}

#[test]
fn versioned_file_requires_four_field_lines() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "#version\t2\n#next\t3\n1\t0\thigh\tok\n2\t0\thigh\n").unwrap();

    let e = TaskStore::new(&file).load().unwrap_err().to_string();
    assert!(e.contains("line 4: expected 4 tab-separated fields"), "{e}");
}

#[test]
fn unversioned_file_with_wrong_field_count_names_both_accepted_counts() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "1\t0\n").unwrap();

    let e = TaskStore::new(&file).load().unwrap_err().to_string();
    assert!(e.contains("expected 3 or 4 tab-separated fields"), "{e}");
}

#[test]
fn unsupported_version_is_rejected() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "#version\t3\n").unwrap();

    let e = TaskStore::new(&file).load().unwrap_err().to_string();
    assert!(e.contains("line 1: unsupported file version 3"), "{e}");
}

#[test]
fn last_possible_id_survives_save_and_reload() {
    let dir = TempDir::new().unwrap();
    let store = TaskStore::new(dir.path().join("tasks.tsv"));
    let mut list = TodoList::with_tasks(vec![medium(i32::MAX - 1, "a", false)]);
    list.add("b", Priority::Medium).unwrap();

    store.save(&list).unwrap();
    let reloaded = store.load().unwrap();

    assert_eq!(reloaded.all()[1].id(), i32::MAX);
    assert_eq!(reloaded.next_id(), MAX_NEXT_ID);
}

#[test]
fn next_id_beyond_last_possible_id_is_rejected() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, format!("#next\t{}\n", MAX_NEXT_ID + 1)).unwrap();

    let e = TaskStore::new(&file).load().unwrap_err().to_string();
    assert!(e.contains("next id is too large"), "{e}");
}

#[test]
fn save_through_symlink_updates_target_and_keeps_link_and_permissions() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real.tsv");
    fs::write(&real, "1\t0\told\n").unwrap();
    chmod(&real, 0o644);
    let link = dir.path().join("link.tsv");
    symlink("real.tsv", &link).unwrap();
    let store = TaskStore::new(&link);

    let mut list = store.load().unwrap();
    list.add("new", Priority::Medium).unwrap();
    store.save(&list).unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(TaskStore::new(&real).load().unwrap().all().len(), 2);
    assert_eq!(mode(&real), 0o644);
}

#[test]
fn save_through_dangling_symlink_creates_target() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("sub/real.tsv");
    let link = dir.path().join("link.tsv");
    symlink(&real, &link).unwrap();
    let store = TaskStore::new(&link);
    let mut list = TodoList::new();
    list.add("x", Priority::Medium).unwrap();

    store.save(&list).unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(TaskStore::new(&real).load().unwrap().all().len(), 1);
}

#[test]
fn saving_keeps_existing_file_permissions() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    fs::write(&file, "").unwrap();
    chmod(&file, 0o664);
    let mut list = TodoList::new();
    list.add("x", Priority::Medium).unwrap();

    TaskStore::new(&file).save(&list).unwrap();

    assert_eq!(mode(&file), 0o664);
}

#[test]
fn new_file_is_private() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    let mut list = TodoList::new();
    list.add("x", Priority::Medium).unwrap();

    TaskStore::new(&file).save(&list).unwrap();

    assert_eq!(mode(&file), 0o600);
}

#[test]
fn escape_and_unescape_are_inverse() {
    let original = "\\t is not a tab, but \t is";
    let escaped = escape(original);

    assert!(!escaped.contains('\t'));
    assert_eq!(unescape(&escaped), original);
}
