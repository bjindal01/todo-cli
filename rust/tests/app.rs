//! Port of the Java `AppTest`: runs commands in-process against a task file in a temp folder.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use todo_cli::app::{ERROR, OK, USAGE};
use todo_cli::{App, TaskStore};

struct Harness {
    _dir: TempDir,
    file: PathBuf,
    out: String,
    err: String,
}

impl Harness {
    fn new() -> Harness {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("tasks.tsv");
        Harness {
            _dir: dir,
            file,
            out: String::new(),
            err: String::new(),
        }
    }

    /// Runs one command in a fresh App, the way separate shell invocations would.
    fn run(&mut self, args: &[&str]) -> i32 {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = App::new(TaskStore::new(&self.file), &mut out, &mut err).run(args);
        self.out = String::from_utf8(out).unwrap();
        self.err = String::from_utf8(err).unwrap();
        code
    }

    fn read(&self) -> String {
        fs::read_to_string(&self.file).unwrap()
    }
}

#[test]
fn add_list_complete_workflow_persists_between_runs() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["add", "Buy", "milk"]), OK);
    assert_eq!(h.out, "Added task 1 (medium): Buy milk\n");

    assert_eq!(h.run(&["add", "Walk the dog"]), OK);
    assert_eq!(h.out, "Added task 2 (medium): Walk the dog\n");

    assert_eq!(h.run(&["complete", "1"]), OK);
    assert_eq!(h.out, "Completed task 1: Buy milk\n");

    assert_eq!(h.run(&["list"]), OK);
    assert_eq!(
        h.out,
        "  1. [x] (medium) Buy milk\n  2. [ ] (medium) Walk the dog\n"
    );
    assert_eq!(h.err, "");
}

#[test]
fn list_on_empty_store_says_so_and_does_not_create_file() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["list"]), OK);

    assert!(h.out.starts_with("No tasks yet."));
    assert!(!h.file.exists());
}

#[test]
fn completing_already_completed_task_is_harmless() {
    let mut h = Harness::new();
    h.run(&["add", "x"]);
    h.run(&["complete", "1"]);

    assert_eq!(h.run(&["complete", "1"]), OK);
    assert_eq!(h.out, "Task 1 is already completed.\n");
}

#[test]
fn complete_unknown_id_is_an_error_and_leaves_file_unchanged() {
    let mut h = Harness::new();
    h.run(&["add", "x"]);
    let before = h.read();

    assert_eq!(h.run(&["complete", "9"]), ERROR);
    assert_eq!(h.err, "Error: No task with id 9.\n");
    assert_eq!(h.read(), before);
}

#[test]
fn no_arguments_prints_usage() {
    let mut h = Harness::new();
    assert_eq!(h.run(&[]), USAGE);
    assert!(h.err.contains("Usage: todo"));
}

#[test]
fn help_prints_usage_to_stdout() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["--help"]), OK);
    assert!(h.out.contains("Usage: todo"));
}

#[test]
fn bad_arguments_are_usage_errors() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["frobnicate"]), USAGE);
    assert!(h.err.contains("Unknown command: frobnicate"));

    assert_eq!(h.run(&["add"]), USAGE);
    assert_eq!(h.run(&["add", "   "]), USAGE);
    assert_eq!(h.run(&["complete"]), USAGE);
    assert_eq!(h.run(&["complete", "1", "2"]), USAGE);
    assert_eq!(h.run(&["complete", "abc"]), USAGE);
    assert!(h.err.contains("Task id must be a number: abc"));
    assert_eq!(h.run(&["list", "extra"]), USAGE);

    assert!(!h.file.exists());
}

#[test]
fn corrupt_file_is_reported_as_error() {
    let mut h = Harness::new();
    fs::write(&h.file, "garbage\n").unwrap();

    assert_eq!(h.run(&["list"]), ERROR);
    assert!(h.err.starts_with("Error: Could not read"), "{}", h.err);
}

#[test]
fn delete_removes_task_and_keeps_others_across_runs() {
    let mut h = Harness::new();
    h.run(&["add", "one"]);
    h.run(&["add", "two"]);
    h.run(&["add", "three"]);

    assert_eq!(h.run(&["delete", "2"]), OK);
    assert_eq!(h.out, "Deleted task 2: two\n");
    assert_eq!(h.err, "");

    h.run(&["list"]);
    assert_eq!(h.out, "  1. [ ] (medium) one\n  3. [ ] (medium) three\n");
}

#[test]
fn delete_works_on_completed_tasks() {
    let mut h = Harness::new();
    h.run(&["add", "one"]);
    h.run(&["complete", "1"]);

    assert_eq!(h.run(&["delete", "1"]), OK);

    h.run(&["list"]);
    assert!(h.out.starts_with("No tasks yet."));
}

#[test]
fn ids_are_not_reused_after_deleting_the_newest_task() {
    let mut h = Harness::new();
    h.run(&["add", "one"]);
    h.run(&["add", "two"]);
    h.run(&["delete", "2"]);

    h.run(&["add", "three"]);
    assert_eq!(h.out, "Added task 3 (medium): three\n");

    assert_eq!(h.run(&["complete", "2"]), ERROR);
}

#[test]
fn delete_unknown_id_is_an_error_and_leaves_file_unchanged() {
    let mut h = Harness::new();
    h.run(&["add", "x"]);
    let before = h.read();

    assert_eq!(h.run(&["delete", "9"]), ERROR);
    assert_eq!(h.err, "Error: No task with id 9.\n");
    assert_eq!(h.read(), before);
}

#[test]
fn delete_twice_reports_the_second_as_unknown() {
    let mut h = Harness::new();
    h.run(&["add", "x"]);
    h.run(&["delete", "1"]);

    assert_eq!(h.run(&["delete", "1"]), ERROR);
    assert_eq!(h.err, "Error: No task with id 1.\n");
}

#[test]
fn delete_argument_errors_are_usage_errors() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["delete"]), USAGE);
    assert!(h.err.contains("delete needs exactly one task id"));
    assert_eq!(h.run(&["delete", "1", "2"]), USAGE);
    assert_eq!(h.run(&["delete", "abc"]), USAGE);
    assert!(h.err.contains("Task id must be a number: abc"));

    assert!(!h.file.exists());
}

#[test]
fn usage_lists_delete_command() {
    let mut h = Harness::new();
    h.run(&["--help"]);
    assert!(h.out.contains("delete <id>"));
}

#[test]
fn add_with_priority_flag_shows_priority() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["add", "-p", "high", "Fix", "bug"]), OK);
    assert_eq!(h.out, "Added task 1 (high): Fix bug\n");

    assert_eq!(h.run(&["add", "--priority", "LOW", "Tidy desk"]), OK);
    assert_eq!(h.out, "Added task 2 (low): Tidy desk\n");

    assert_eq!(h.run(&["add", "--priority=medium", "Read book"]), OK);
    assert_eq!(h.out, "Added task 3 (medium): Read book\n");

    h.run(&["list"]);
    assert_eq!(
        h.out,
        "  1. [ ] (high) Fix bug\n  2. [ ] (low) Tidy desk\n  3. [ ] (medium) Read book\n"
    );
}

#[test]
fn text_starting_with_dash_is_not_treated_as_an_option() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["add", "-5", "degrees", "outside"]), OK);
    assert_eq!(h.out, "Added task 1 (medium): -5 degrees outside\n");

    assert_eq!(h.run(&["add", "--", "-p", "is", "a", "flag"]), OK);
    assert_eq!(h.out, "Added task 2 (medium): -p is a flag\n");
}

#[test]
fn invalid_priority_arguments_are_usage_errors() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["add", "-p", "urgent", "x"]), USAGE);
    assert!(
        h.err
            .contains("Priority must be high, medium or low: urgent")
    );

    assert_eq!(h.run(&["add", "-p"]), USAGE);
    assert!(h.err.contains("-p needs a priority"));

    assert_eq!(h.run(&["add", "-p", "high"]), USAGE);
    assert!(h.err.contains("add needs the task text"));

    assert!(!h.file.exists());
}

#[test]
fn list_sorted_by_priority_puts_high_first_and_keeps_id_order_for_ties() {
    let mut h = Harness::new();
    h.run(&["add", "-p", "low", "a"]);
    h.run(&["add", "b"]);
    h.run(&["add", "-p", "high", "c"]);
    h.run(&["add", "-p", "low", "d"]);
    h.run(&["add", "-p", "high", "e"]);
    h.run(&["complete", "5"]);
    let expected = "  3. [ ] (high) c\n  5. [x] (high) e\n  2. [ ] (medium) b\n  1. [ ] (low) a\n  4. [ ] (low) d\n";

    assert_eq!(h.run(&["list", "--sort", "priority"]), OK);
    assert_eq!(h.out, expected);

    assert_eq!(h.run(&["list", "--sort=priority"]), OK);
    assert_eq!(h.out, expected);
}

#[test]
fn list_sort_by_id() {
    let mut h = Harness::new();
    h.run(&["add", "-p", "low", "a"]);
    h.run(&["add", "-p", "high", "b"]);

    assert_eq!(h.run(&["list", "--sort", "id"]), OK);
    assert_eq!(h.out, "  1. [ ] (low) a\n  2. [ ] (high) b\n");
}

#[test]
fn list_sorted_by_priority_on_empty_store() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["list", "--sort", "priority"]), OK);
    assert!(h.out.starts_with("No tasks yet."));
}

#[test]
fn bad_list_options_are_usage_errors() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["list", "--sort", "name"]), USAGE);
    assert!(h.err.contains("Sort must be id or priority: name"));
    assert_eq!(h.run(&["list", "--sort"]), USAGE);
    assert_eq!(h.run(&["list", "--sort", "priority", "extra"]), USAGE);
    assert_eq!(h.run(&["list", "priority"]), USAGE);
}

#[test]
fn complete_keeps_priority() {
    let mut h = Harness::new();
    h.run(&["add", "-p", "high", "x"]);
    h.run(&["complete", "1"]);

    h.run(&["list"]);
    assert_eq!(h.out, "  1. [x] (high) x\n");
}

#[test]
fn existing_file_without_priorities_lists_tasks_as_medium() {
    let mut h = Harness::new();
    fs::write(&h.file, "1\t0\told one\n2\t1\told two\n").unwrap();
    h.run(&["add", "-p", "high", "new"]);

    h.run(&["list", "--sort", "priority"]);
    assert_eq!(
        h.out,
        "  3. [ ] (high) new\n  1. [ ] (medium) old one\n  2. [x] (medium) old two\n"
    );
}

#[test]
fn priority_option_edge_cases() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["add", "--priority"]), USAGE);
    assert!(
        h.err
            .contains("Error: --priority needs a priority: high, medium or low"),
        "{}",
        h.err
    );

    assert_eq!(h.run(&["add", "--priority=", "x"]), USAGE);
    assert!(
        h.err
            .contains("Error: Priority must be high, medium or low: \n"),
        "{}",
        h.err
    );

    assert_eq!(h.run(&["add", "--priority=urgent", "x"]), USAGE);
    assert!(
        h.err
            .contains("Error: Priority must be high, medium or low: urgent"),
        "{}",
        h.err
    );
    assert!(!h.file.exists());

    assert_eq!(h.run(&["add", "-p", "high", "-p", "low", "x"]), OK);
    assert_eq!(h.out, "Added task 1 (low): x\n");

    assert_eq!(h.run(&["add", "--", "-p", "high", "y"]), OK);
    assert_eq!(h.out, "Added task 2 (medium): -p high y\n");
}

#[test]
fn options_after_the_text_are_part_of_the_text() {
    let mut h = Harness::new();
    assert_eq!(h.run(&["add", "Fix", "bug", "-p", "high"]), OK);
    assert_eq!(h.out, "Added task 1 (medium): Fix bug -p high\n");
}

#[test]
fn usage_says_priority_goes_before_text() {
    let mut h = Harness::new();
    h.run(&["--help"]);
    assert!(h.out.contains("put -p before the text"), "{}", h.out);
}

#[test]
fn sort_option_edge_cases() {
    let mut h = Harness::new();
    h.run(&["add", "-p", "low", "a"]);
    h.run(&["add", "-p", "high", "b"]);

    assert_eq!(h.run(&["list", "--sort=id"]), OK);
    assert_eq!(h.out, "  1. [ ] (low) a\n  2. [ ] (high) b\n");

    assert_eq!(h.run(&["list", "--sort", "Priority"]), OK);
    assert_eq!(h.out, "  2. [ ] (high) b\n  1. [ ] (low) a\n");

    assert_eq!(h.run(&["list", "--sort=PRIORITY"]), OK);
    assert_eq!(h.out, "  2. [ ] (high) b\n  1. [ ] (low) a\n");

    assert_eq!(h.run(&["list", "--sort="]), USAGE);
    assert!(
        h.err.contains("Error: Sort must be id or priority: \n"),
        "{}",
        h.err
    );

    assert_eq!(h.run(&["list", "--sort"]), USAGE);
    assert!(
        h.err
            .contains("Error: list accepts only --sort id or --sort priority"),
        "{}",
        h.err
    );

    assert_eq!(h.run(&["list", "--sort", "priority", "extra"]), USAGE);
    assert!(
        h.err
            .contains("Error: list accepts only --sort id or --sort priority"),
        "{}",
        h.err
    );
}

#[test]
fn running_out_of_ids_is_an_error_not_a_crash() {
    let mut h = Harness::new();
    fs::write(&h.file, "2147483647\t0\tlast\n").unwrap();
    let before = h.read();

    assert_eq!(h.run(&["add", "one more"]), ERROR);
    assert_eq!(h.err, "Error: No more task ids are available.\n");
    assert_eq!(h.read(), before);
}

#[test]
fn duplicate_ids_in_file_are_reported_not_half_handled() {
    let mut h = Harness::new();
    fs::write(&h.file, "1\t0\tx\n1\t0\ty\n").unwrap();

    assert_eq!(h.run(&["complete", "1"]), ERROR);
    assert!(h.err.contains("line 2: duplicate id 1"), "{}", h.err);
}

#[test]
fn invalid_utf8_file_is_reported_clearly() {
    let mut h = Harness::new();
    fs::write(&h.file, [0xFF, b'\n']).unwrap();

    assert_eq!(h.run(&["list"]), ERROR);
    assert_eq!(
        h.err,
        format!(
            "Error: Could not read {}: it is not valid UTF-8 text.\n",
            h.file.display()
        )
    );
}

#[test]
fn multi_line_task_is_listed_on_one_line() {
    let mut h = Harness::new();
    h.run(&["add", "first\nsecond"]);

    h.run(&["list"]);
    assert_eq!(h.out, "  1. [ ] (medium) first second\n");
}
