//! Port of the Java `ConcurrentProcessesTest`: runs the real binary in separate processes at the
//! same time, the way overlapping shell commands would.

use std::path::Path;
use std::process::{Child, Command, Stdio};

use tempfile::TempDir;
use todo_cli::{Priority, Task, TaskStore, TodoList};

const PROCESSES: i32 = 8;

fn start(file: &Path, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_todo"))
        .args(args)
        .env("TODO_FILE", file)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn wait_all(processes: Vec<Child>) {
    for process in processes {
        let output = process.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn simultaneous_adds_all_survive_with_distinct_ids() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");

    let processes = (1..=PROCESSES)
        .map(|i| start(&file, &["add", &format!("task {i}")]))
        .collect();
    wait_all(processes);

    let list = TaskStore::new(&file).load().unwrap();
    let mut ids: Vec<i32> = list.all().iter().map(Task::id).collect();
    ids.sort();
    assert_eq!(ids, (1..=PROCESSES).collect::<Vec<_>>());
    let mut texts: Vec<&str> = list.all().iter().map(Task::text).collect();
    texts.sort();
    let mut expected: Vec<String> = (1..=PROCESSES).map(|i| format!("task {i}")).collect();
    expected.sort();
    assert_eq!(texts, expected);
}

#[test]
fn simultaneous_completes_and_deletes_keep_every_other_change() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    let store = TaskStore::new(&file);
    let mut list = TodoList::new();
    for i in 1..=PROCESSES {
        list.add(&format!("task {i}"), Priority::Medium).unwrap();
    }
    store.save(&list).unwrap();

    let processes = (1..=PROCESSES)
        .map(|i| {
            let command = if i % 2 == 0 { "delete" } else { "complete" };
            start(&file, &[command, &i.to_string()])
        })
        .collect();
    wait_all(processes);

    let remaining = store.load().unwrap();
    let ids: Vec<i32> = remaining.all().iter().map(Task::id).collect();
    assert_eq!(ids, [1, 3, 5, 7]);
    assert!(remaining.all().iter().all(Task::done), "{remaining:?}");
}
