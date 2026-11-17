//! Differential tests: each scenario runs the same commands against the Java build and the Rust
//! binary, each in its own fresh folder, and compares stdout, stderr, exit codes and the
//! resulting files (names, types, permissions, symlink targets and exact bytes).
//!
//! The Java side runs `java -jar <jar>`, where the jar is `$TODO_JAVA_JAR` or `../target/todo.jar`
//! (built with `mvn package` in the repository root). Without Java or the jar these tests are
//! skipped with a message, unless `TODO_REQUIRE_PARITY=1` is set, which makes them fail instead.

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;

#[derive(Clone)]
struct Implementation {
    name: &'static str,
    program: OsString,
    prefix: Vec<OsString>,
}

fn rust() -> Implementation {
    Implementation {
        name: "rust",
        program: env!("CARGO_BIN_EXE_todo").into(),
        prefix: Vec::new(),
    }
}

fn java() -> Option<Implementation> {
    static JAVA: OnceLock<Option<Implementation>> = OnceLock::new();
    JAVA.get_or_init(|| {
        let jar = std::env::var_os("TODO_JAVA_JAR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/todo.jar"));
        let java_runs = Command::new("java")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        (jar.is_file() && java_runs).then(|| Implementation {
            name: "java",
            program: "java".into(),
            prefix: vec!["-jar".into(), jar.into()],
        })
    })
    .clone()
}

fn java_or_skip(test: &str) -> Option<Implementation> {
    let java = java();
    if java.is_none() {
        let message = format!(
            "{test}: Java or the Java jar is missing (build it with `mvn package` in the repository root)"
        );
        if std::env::var_os("TODO_REQUIRE_PARITY").is_some() {
            panic!("{message}");
        }
        eprintln!("skipping {message}");
    }
    java
}

/// One run of a scenario against one implementation. Every observation goes into `log`, with
/// the scenario folder's path replaced by `<DIR>` so the two runs can be compared.
struct Ctx {
    imp: Implementation,
    dir: TempDir,
    root: PathBuf,
    todo_file: Option<OsString>,
    cwd: PathBuf,
    env: Vec<(&'static str, String)>,
    log: Vec<String>,
}

impl Ctx {
    fn new(imp: Implementation) -> Ctx {
        let dir = TempDir::new().unwrap();
        let root = dir.path().canonicalize().unwrap();
        Ctx {
            imp,
            todo_file: Some(root.join("tasks.tsv").into()),
            cwd: root.clone(),
            env: vec![("LC_ALL", "C.UTF-8".to_owned())],
            dir,
            root,
            log: Vec::new(),
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Points `TODO_FILE` at `rel` inside the scenario folder.
    fn todo_file(&mut self, rel: &str) {
        let mut value = self.root.clone().into_os_string();
        value.push("/");
        value.push(rel);
        self.todo_file = Some(value);
    }

    /// Sets `TODO_FILE` to exactly this value.
    fn todo_file_raw(&mut self, value: &str) {
        self.todo_file = Some(value.into());
    }

    /// Replaces the locale variables (by default `LC_ALL=C.UTF-8`).
    fn locale(&mut self, vars: &[(&'static str, &str)]) {
        self.env = vars.iter().map(|&(k, v)| (k, v.to_owned())).collect();
    }

    fn cwd(&mut self, rel: &str) {
        self.cwd = self.path(rel);
    }

    fn write(&mut self, rel: &str, bytes: impl AsRef<[u8]>) {
        let path = self.path(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    fn mkdir(&mut self, rel: &str) {
        fs::create_dir_all(self.path(rel)).unwrap();
    }

    fn symlink(&mut self, target: &str, rel: &str) {
        symlink(target, self.path(rel)).unwrap();
    }

    fn chmod(&mut self, rel: &str, mode: u32) {
        fs::set_permissions(self.path(rel), fs::Permissions::from_mode(mode)).unwrap();
    }

    fn run(&mut self, args: &[&str]) {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        self.run_os(&args);
    }

    fn run_os(&mut self, args: &[OsString]) {
        let mut command = Command::new(&self.imp.program);
        command
            .args(&self.imp.prefix)
            .args(args)
            .current_dir(&self.cwd)
            .env_remove("LC_ALL")
            .env_remove("LANG")
            .env_remove("LC_CTYPE")
            .env_remove("LC_MESSAGES")
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .env("HOME", self.root.join("home"))
            .env_remove("TODO_FILE")
            .stdin(Stdio::null());
        if let Some(file) = &self.todo_file {
            command.env("TODO_FILE", file);
        }
        let output = command.output().unwrap();
        let shown: Vec<String> = args.iter().map(|a| format!("{a:?}")).collect();
        let entry = format!(
            "$ todo {}\n[exit {:?}]\n[stdout]\n{}[stderr]\n{}",
            shown.join(" "),
            output.status.code(),
            self.scrub(&output.stdout),
            self.scrub(&output.stderr)
        );
        self.log.push(entry);
    }

    fn scrub(&self, bytes: &[u8]) -> String {
        let root = self.root.to_string_lossy();
        String::from_utf8_lossy(bytes).replace(root.as_ref(), "<DIR>")
    }

    /// Records every file under the scenario folder.
    fn snapshot(&mut self) {
        let mut entries = Vec::new();
        walk(&self.root, &self.root, &mut entries);
        let root = self.root.to_string_lossy().into_owned();
        let tree = entries.join("\n").replace(&root, "<DIR>");
        self.log.push(format!("[files]\n{tree}"));
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        restore_permissions(self.dir.path());
    }
}

fn walk(root: &Path, dir: &Path, entries: &mut Vec<String>) {
    let Ok(read) = fs::read_dir(dir) else {
        entries.push(format!("{} (unreadable)", rel(root, dir)));
        return;
    };
    let mut children: Vec<PathBuf> = read.map(|e| e.unwrap().path()).collect();
    children.sort();
    for path in children {
        let name = rel(root, &path);
        if name == "home" {
            continue;
        }
        let meta = fs::symlink_metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o7777;
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&path).unwrap();
            entries.push(format!("L {name} -> {}", target.display()));
        } else if meta.is_dir() {
            entries.push(format!("D {name} {mode:o}"));
            walk(root, &path, entries);
        } else {
            let content = fs::read(&path)
                .map(|b| b.escape_ascii().to_string())
                .unwrap_or_else(|_| "(unreadable)".to_owned());
            entries.push(format!("F {name} {mode:o} {content}"));
        }
    }
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap().display().to_string()
}

fn restore_permissions(dir: &Path) {
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
    if let Ok(read) = fs::read_dir(dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                restore_permissions(&path);
            } else if entry.file_type().is_ok_and(|t| t.is_file()) {
                let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o644));
            }
        }
    }
}

/// Runs `scenario` against both implementations and fails on the first observation that differs.
fn check(name: &str, scenario: impl Fn(&mut Ctx)) {
    let Some(java) = java_or_skip(name) else {
        return;
    };
    let mut logs = Vec::new();
    for imp in [java, rust()] {
        let mut ctx = Ctx::new(imp);
        scenario(&mut ctx);
        ctx.snapshot();
        logs.push((ctx.imp.name, std::mem::take(&mut ctx.log)));
    }
    let (java_log, rust_log) = (&logs[0].1, &logs[1].1);
    for (i, (j, r)) in java_log.iter().zip(rust_log).enumerate() {
        assert_eq!(
            j, r,
            "{name}: step {i} differs\n--- java ---\n{j}\n--- rust ---\n{r}"
        );
    }
    assert_eq!(java_log.len(), rust_log.len(), "{name}: step counts differ");
    let commands = java_log.iter().filter(|e| e.starts_with("$ ")).count();
    eprintln!(
        "parity {name}: {commands} commands and {} file snapshots match",
        java_log.len() - commands
    );
}

/// Runs the usual set of commands against whatever task file the scenario set up.
fn exercise(c: &mut Ctx) {
    c.run(&["list"]);
    c.run(&["list", "--sort", "priority"]);
    c.run(&["complete", "1"]);
    c.run(&["delete", "2"]);
    c.run(&["add", "-p", "high", "new task"]);
    c.run(&["list"]);
    c.snapshot();
}

#[test]
fn usage_and_help() {
    check("usage_and_help", |c| {
        c.run(&[]);
        c.run(&["help"]);
        c.run(&["-h"]);
        c.run(&["--help"]);
        c.run(&["help", "extra", "args"]);
        c.run(&["frobnicate"]);
        c.run(&[""]);
        c.run(&["ADD", "x"]);
        c.run(&["--version"]);
        c.run(&["list", "--help"]);
    });
}

#[test]
fn empty_store() {
    check("empty_store", |c| {
        c.run(&["list"]);
        c.run(&["list", "--sort", "priority"]);
        c.run(&["list", "--sort=id"]);
        c.snapshot();
        c.run(&["complete", "1"]);
        c.run(&["delete", "1"]);
        c.snapshot();
    });
}

#[test]
fn workflow() {
    check("workflow", |c| {
        c.run(&["add", "Buy", "milk"]);
        c.run(&["add", "Walk the dog"]);
        c.run(&["complete", "1"]);
        c.run(&["complete", "1"]);
        c.run(&["list"]);
        c.run(&["delete", "2"]);
        c.run(&["delete", "2"]);
        c.run(&["add", "three"]);
        c.run(&["delete", "3"]);
        c.run(&["add", "four"]);
        c.run(&["list"]);
        c.run(&["delete", "1"]);
        c.run(&["delete", "4"]);
        c.run(&["list"]);
        c.run(&["add", "five"]);
        c.run(&["list"]);
    });
}

#[test]
fn priorities_and_sorting() {
    check("priorities_and_sorting", |c| {
        c.run(&["add", "-p", "low", "a"]);
        c.run(&["add", "b"]);
        c.run(&["add", "-p", "high", "c"]);
        c.run(&["add", "--priority", "LOW", "d"]);
        c.run(&["add", "--priority=High", "e"]);
        c.run(&["add", "-p", " medium ", "f"]);
        c.run(&["add", "--priority=\tlow\t", "g"]);
        c.run(&["complete", "5"]);
        for args in [
            &["list"][..],
            &["list", "--sort", "priority"],
            &["list", "--sort=priority"],
            &["list", "--sort", "Priority"],
            &["list", "--sort=PRIORITY"],
            &["list", "--sort", " priority "],
            &["list", "--sort", "id"],
            &["list", "--sort=ID"],
        ] {
            c.run(args);
        }
    });
}

#[test]
fn add_arguments() {
    check("add_arguments", |c| {
        for args in [
            &["add"][..],
            &["add", "   "],
            &["add", "", ""],
            &["add", "-p"],
            &["add", "--priority"],
            &["add", "-p", "urgent", "x"],
            &["add", "--priority=", "x"],
            &["add", "--priority=urgent", "x"],
            &["add", "-p", "high"],
            &["add", "--"],
            &["add", "-p", "high", "--"],
            &["add", "-p", "high", "-p"],
            &["add", "-P", "high", "x"],
            &["add", "--priority", "", "x"],
        ] {
            c.run(args);
        }
        c.snapshot();
        for args in [
            &["add", "-5", "degrees", "outside"][..],
            &["add", "--", "-p", "is", "a", "flag"],
            &["add", "Fix", "bug", "-p", "high"],
            &["add", "-p", "high", "-p", "low", "x"],
            &["add", "--", "--"],
            &["add", "--", "--", "x"],
            &["add", "  spaced  ", "  words  "],
            &["add", "-p", "low", "--priority=high", "--", "-p", "low"],
            &["add", "--priority", "low", "--priority", "high", "z"],
            &["add", "-", "dash"],
            &["add", "--priorityx", "y"],
        ] {
            c.run(args);
        }
        c.run(&["list"]);
        c.run(&["list", "--sort", "priority"]);
    });
}

#[test]
fn list_arguments() {
    check("list_arguments", |c| {
        c.run(&["add", "x"]);
        for args in [
            &["list", "extra"][..],
            &["list", "--sort"],
            &["list", "--sort", "name"],
            &["list", "--sort="],
            &["list", "--sort= "],
            &["list", "--sort", "priority", "extra"],
            &["list", "priority"],
            &["list", "--sort", "--sort=id"],
            &["list", "--sort=id", "--sort=id"],
            &["list", "-s", "id"],
            &["list", "--SORT", "id"],
        ] {
            c.run(args);
        }
    });
}

#[test]
fn task_id_arguments() {
    check("task_id_arguments", |c| {
        for i in 1..=3 {
            c.run(&["add", &format!("task {i}")]);
        }
        for command in ["complete", "delete"] {
            for args in [
                &[][..],
                &["1", "2"],
                &["abc"],
                &[""],
                &[" 1"],
                &["1 "],
                &["1.0"],
                &["0x1"],
                &["0"],
                &["-0"],
                &["-1"],
                &["99"],
                &["2147483647"],
                &["2147483648"],
                &["-2147483648"],
                &["-2147483649"],
                &["99999999999999999999"],
                &["+"],
                &["-"],
                &["\u{1D7CF}"],
            ] {
                let mut full = vec![command];
                full.extend_from_slice(args);
                c.run(&full);
            }
        }
        c.run(&["complete", "+1"]);
        c.run(&["complete", "02"]);
        c.run(&["delete", "\u{663}"]);
        c.run(&["add", "four"]);
        c.run(&["complete", "\u{FF14}"]);
        c.run(&["list"]);
    });
}

#[test]
fn special_characters() {
    check("special_characters", |c| {
        for text in [
            "tab\there",
            "line\nbreak",
            "carriage\rreturn",
            "windows\r\nline",
            "back\\slash",
            "literal \\t and \\n and \\\\",
            "ends with backslash\\",
            "café ✓ naïve 日本語 中文 한국어",
            "emoji 🎉👍🏽 family 👨‍👩‍👧",
            "arabic مرحبا hebrew שלום",
            "quotes \"double\" 'single' `back`",
            "shell $HOME $(echo hi) ; | & > <",
            "format %d %s %n {} {0}",
            "#version\t2",
            "#next\t1",
            "nbsp\u{A0}inside\u{A0}",
            "\u{A0}lead and trail nbsp\u{A0}",
            "\u{1F}unit separators\u{1C}",
            "\u{3000}ideographic space\u{3000}",
            "next line\u{85}",
            "zero\u{200B}width\u{FEFF}",
            "vertical\u{B}tab\u{C}formfeed",
            "line sep\u{2028}para sep\u{2029}",
            "bell\u{7} escape\u{1B}[31m",
            "\u{10FFFF} max code point",
        ] {
            c.run(&["add", text]);
        }
        c.run(&[
            "add",
            "-p",
            "high",
            "multi",
            "word\twith\ttabs",
            "and\nlines",
        ]);
        c.run(&["list"]);
        c.run(&["list", "--sort", "priority"]);
        c.run(&["complete", "2"]);
        c.run(&["delete", "4"]);
        c.run(&["complete", "6"]);
        c.run(&["delete", "14"]);
        c.run(&["list"]);
    });
}

#[test]
fn invalid_utf8_arguments() {
    check("invalid_utf8_arguments", |c| {
        for bytes in [
            &b"caf\xff\xfe x"[..],
            b"truncated \xe2\x9c end",
            b"overlong \xc0\xaf end",
            b"surrogate \xed\xa0\x80 end",
            b"four byte truncated \xf0\x9f\x98 end",
            b"lone continuation \x80\x80 end",
            b"\xf5\x80\x80\x80 too high",
        ] {
            c.run_os(&["add".into(), OsString::from_vec(bytes.to_vec())]);
        }
        c.run_os(&[
            "add".into(),
            "-p".into(),
            OsString::from_vec(b"hi\xffgh".to_vec()),
            "x".into(),
        ]);
        c.run_os(&["complete".into(), OsString::from_vec(b"1\xff".to_vec())]);
        c.run(&["list"]);
    });
}

#[test]
fn corrupt_files() {
    let cases: &[&[u8]] = &[
        b"garbage\n",
        b"1\t0\n",
        b"1\n",
        b"1\t0\tone\nnot-a-task\n",
        b"1\tyes\tone\n",
        b"1\t2\tone\n",
        b"1\t\tone\n",
        b"0\t0\tone\n",
        b"-1\t0\tone\n",
        b"abc\t0\tone\n",
        b" 1\t0\tone\n",
        b"2147483648\t0\tone\n",
        b"1\t0\turgent\tbad\n",
        b"1\t0\t\tempty priority\n",
        b"1\t0\tx\n1\t0\ty\n",
        b"1\t0\tx\n01\t0\ty\n",
        b"1\t0\tx\n+1\t0\ty\n",
        b"#version\t2\n#next\t3\n1\t0\thigh\tok\n2\t0\thigh\n",
        b"#version\t3\n",
        b"#version\t\n",
        b"#version\t2 \n",
        b"#version\n",
        b"#next\tabc\n1\t0\tone\n",
        b"1\t0\tone\n#next\t0\n",
        b"#next\t-5\n",
        b"#next\t\n",
        b"#next\t2147483649\n",
        b"#next\t9223372036854775807\n",
        b"#next\t9223372036854775808\n",
        b"#comment\n",
        b"\xff\n",
        b"ok line\n\xc3\x28\n",
        b"1\t0\tsurrogate \xed\xa0\x80\n",
        b"1\t0\toverlong \xc0\xaf\n",
        b"\xef\xbb\xbf#version\t2\n#next\t2\n1\t0\tmedium\tbom\n",
        b"\xef\xbb\xbf1\t0\tbom\n",
        b"1\t0\tone\r\n2\t0\t\r\n3\tx\tthree\r\n",
    ];
    for (i, bytes) in cases.iter().enumerate() {
        check(&format!("corrupt_files[{i}]"), |c| {
            c.write("tasks.tsv", bytes);
            exercise(c);
        });
    }
}

#[test]
fn readable_file_variants() {
    let cases: &[&[u8]] = &[
        b"",
        b"\n\n\n",
        b"   \n\t\n\x0b\x0c\n\x1c\x1d\x1e\x1f\n",
        b"#version\t2\n#next\t1\n",
        b"#version\t2\n#next\t7\n",
        b"1\t0\told one\n2\t1\told two\n",
        b"1\t0\tlegacy\n2\t1\thigh\tnew style\n3\t0\tLOW\tcase\n4\t0\t Medium \tspaced\n",
        b"#next\t4\n1\t0\told task\n2\t1\thigh\tnew task\n",
        b"3\t0\tthird\n1\t1\tfirst\n2\t0\tsecond\n",
        b"3\t0\thigh\tthird\n1\t1\tlow\tfirst\n2\t0\thigh\tsecond\n",
        b"1\t0\tone\n#next\t10\n2\t0\ttwo\n#next\t5\n",
        b"1\t0\tone\n#version\t2\n2\t0\thigh\ttwo\n",
        b"1\t0\tone\r\n2\t1\ttwo\r\n",
        b"1\t0\tone\r2\t1\ttwo\r",
        b"1\t0\tno trailing newline",
        b"1\t0\t\n2\t0\tmedium\t\n",
        b"1\t0\tescapes \\t \\n \\r \\\\ \\x \\\n",
        b"1\t0\tmedium\tfour\tfields\twith tabs\n",
        b"\xd9\xa1\t0\tarabic digit id\n\xef\xbc\x92\t1\tfullwidth digit id\n",
        b"+5\t0\tplus id\n007\t0\tleading zeros\n",
        b"#next\t+3\n1\t0\tone\n",
        b"#next\t\xd9\xa9\n1\t0\tone\n",
        b"1\t0\tunicode caf\xc3\xa9 \xe2\x9c\x93 \xf0\x9f\x8e\x89\n",
        b"1\t1\thigh\tdone already\n",
        b"1\t0\tone\n99\t0\tninety-nine\n100\t0\thundred\n1000\t1\tthousand\n123456\t0\tbig\n",
        b"#vErSiOn\t2\n",
        b" #version\t2\n1\t0\tx\n",
    ];
    for (i, bytes) in cases.iter().enumerate() {
        check(&format!("readable_file_variants[{i}]"), |c| {
            c.write("tasks.tsv", bytes);
            exercise(c);
        });
    }
}

#[test]
fn id_limits() {
    check("id_limits_last_id_used", |c| {
        c.write("tasks.tsv", "2147483647\t0\tlast\n");
        c.run(&["add", "one more"]);
        c.run(&["complete", "2147483647"]);
        c.run(&["delete", "2147483647"]);
        c.run(&["add", "after delete"]);
        c.snapshot();
    });
    check("id_limits_next_is_max", |c| {
        c.write("tasks.tsv", "#next\t2147483648\n");
        c.run(&["add", "x"]);
        c.run(&["list"]);
        c.snapshot();
    });
    check("id_limits_second_to_last", |c| {
        c.write("tasks.tsv", "#version\t2\n#next\t2147483647\n");
        c.run(&["add", "last one"]);
        c.run(&["add", "too many"]);
        c.run(&["list"]);
        c.snapshot();
    });
}

#[test]
fn task_file_paths() {
    check("paths_nested_missing_folders", |c| {
        c.todo_file("a/b/c/tasks.tsv");
        c.run(&["list"]);
        c.run(&["add", "x"]);
        c.run(&["list"]);
    });
    check("paths_relative", |c| {
        c.mkdir("work");
        c.cwd("work");
        c.todo_file_raw("rel/tasks.tsv");
        c.run(&["add", "x"]);
        c.todo_file_raw("./dot.tsv");
        c.run(&["add", "y"]);
        c.todo_file_raw("../up.tsv");
        c.run(&["add", "z"]);
        c.todo_file_raw("plain.tsv");
        c.run(&["add", "w"]);
        c.run(&["list"]);
    });
    check("paths_redundant_slashes", |c| {
        let root = c.root.display().to_string();
        c.todo_file_raw(&format!("{root}//sub///tasks.tsv/"));
        c.run(&["add", "x"]);
        c.run(&["list"]);
        c.todo_file_raw(&format!("{root}/sub/./../sub/tasks.tsv"));
        c.run(&["list"]);
        c.run(&["add", "y"]);
    });
    check("paths_parent_is_a_file", |c| {
        c.write("afile", "not a folder");
        c.todo_file("afile/tasks.tsv");
        c.run(&["list"]);
        c.run(&["add", "x"]);
        c.run(&["complete", "1"]);
        c.todo_file("afile/sub/tasks.tsv");
        c.run(&["list"]);
        c.run(&["add", "x"]);
    });
    check("paths_task_file_is_a_folder", |c| {
        c.mkdir("folder");
        c.todo_file("folder");
        exercise(c);
    });
    check("paths_lock_path_is_a_folder", |c| {
        c.mkdir("tasks.tsv.lock");
        exercise(c);
    });
    check("paths_dot_names", |c| {
        c.mkdir("d");
        c.todo_file("d/..");
        c.run(&["list"]);
        c.run(&["add", "x"]);
        c.todo_file("d/.");
        c.run(&["add", "x"]);
    });
    check("paths_blank_todo_file_is_ignored", |c| {
        // Both fall back to the real home folder, so only run a command that reads nothing
        // unless a ~/.todo.tsv exists; usage errors never touch the file.
        c.todo_file_raw("   ");
        c.run(&["complete", "abc"]);
    });
}

#[test]
fn symlinks() {
    check("symlink_to_existing_file", |c| {
        c.write("real.tsv", "1\t0\told\n");
        c.chmod("real.tsv", 0o644);
        c.symlink("real.tsv", "link.tsv");
        c.todo_file("link.tsv");
        exercise(c);
    });
    check("symlink_absolute_into_other_folder", |c| {
        c.write("data/real.tsv", "1\t0\told\n");
        let target = c.path("data/real.tsv").display().to_string();
        c.symlink(&target, "link.tsv");
        c.todo_file("link.tsv");
        exercise(c);
    });
    check("symlink_dangling_into_missing_folder", |c| {
        c.symlink("sub/deeper/real.tsv", "link.tsv");
        c.todo_file("link.tsv");
        c.run(&["list"]);
        c.run(&["add", "x"]);
        c.run(&["list"]);
    });
    check("symlink_chain", |c| {
        c.write("real.tsv", "#version\t2\n#next\t2\n1\t0\tlow\tchained\n");
        c.mkdir("x");
        c.symlink("../real.tsv", "x/l1");
        c.symlink("x/l1", "l2");
        c.symlink("l2", "l3");
        c.todo_file("l3");
        exercise(c);
    });
    check("symlink_loop", |c| {
        c.symlink("loop_b", "loop_a");
        c.symlink("loop_a", "loop_b");
        c.todo_file("loop_a");
        exercise(c);
    });
    check("symlink_chain_of_40", |c| {
        c.write("real.tsv", "1\t0\tdeep\n");
        c.symlink("real.tsv", "l1");
        for i in 2..=40 {
            c.symlink(&format!("l{}", i - 1), &format!("l{i}"));
        }
        c.todo_file("l40");
        c.run(&["add", "x"]);
        c.run(&["list"]);
    });
    check("symlink_chain_of_41", |c| {
        c.write("real.tsv", "1\t0\tdeep\n");
        c.symlink("real.tsv", "l1");
        for i in 2..=41 {
            c.symlink(&format!("l{}", i - 1), &format!("l{i}"));
        }
        c.todo_file("l41");
        c.run(&["add", "x"]);
        c.run(&["list"]);
    });
    check("symlink_to_folder", |c| {
        c.mkdir("folder");
        c.symlink("folder", "link");
        c.todo_file("link");
        exercise(c);
    });
    check("symlink_parent_folder", |c| {
        c.mkdir("real_dir");
        c.symlink("real_dir", "dir_link");
        c.todo_file("dir_link/tasks.tsv");
        exercise(c);
    });
}

#[test]
fn permissions() {
    check("permissions_existing_file_mode_kept", |c| {
        c.write("tasks.tsv", "1\t0\tx\n");
        c.chmod("tasks.tsv", 0o664);
        c.run(&["add", "y"]);
        c.snapshot();
        c.chmod("tasks.tsv", 0o640);
        c.run(&["complete", "1"]);
        c.snapshot();
        c.chmod("tasks.tsv", 0o4755);
        c.run(&["delete", "1"]);
    });
    check("permissions_new_file_is_private", |c| {
        c.run(&["add", "x"]);
    });
    check("permissions_unreadable_file", |c| {
        c.write("tasks.tsv", "1\t0\tx\n");
        c.chmod("tasks.tsv", 0o000);
        exercise(c);
    });
    check("permissions_read_only_file_in_writable_folder", |c| {
        c.write("tasks.tsv", "1\t0\tx\n");
        c.chmod("tasks.tsv", 0o444);
        exercise(c);
    });
    check("permissions_unwritable_folder_without_lock", |c| {
        c.write("ro/tasks.tsv", "1\t0\tx\n");
        c.chmod("ro", 0o555);
        c.todo_file("ro/tasks.tsv");
        exercise(c);
    });
    check("permissions_unwritable_folder_with_lock", |c| {
        c.write("ro/tasks.tsv", "1\t0\tx\n2\t1\tdone\n");
        c.write("ro/tasks.tsv.lock", "");
        c.chmod("ro", 0o555);
        c.todo_file("ro/tasks.tsv");
        c.run(&["complete", "2"]);
        exercise(c);
    });
    check("permissions_unwritable_lock_file", |c| {
        c.write("tasks.tsv", "1\t0\tx\n");
        c.write("tasks.tsv.lock", "");
        c.chmod("tasks.tsv.lock", 0o444);
        exercise(c);
    });
    check("permissions_unsearchable_folder", |c| {
        c.write("closed/tasks.tsv", "1\t0\tx\n");
        c.chmod("closed", 0o000);
        c.todo_file("closed/tasks.tsv");
        c.run(&["list"]);
        c.run(&["add", "x"]);
        c.todo_file("closed/sub/tasks.tsv");
        c.run(&["list"]);
        c.run(&["add", "x"]);
    });
    check("permissions_cannot_create_missing_folder", |c| {
        c.mkdir("ro");
        c.chmod("ro", 0o555);
        c.todo_file("ro/new/tasks.tsv");
        c.run(&["list"]);
        c.run(&["add", "x"]);
    });
}

#[test]
fn stdout_closed() {
    check("stdout_closed", |c| {
        c.run(&["add", "x"]);
        // Exit codes stay the same when output cannot be written.
        for args in [&["list"][..], &["add", "y"], &["help"]] {
            let status = Command::new(&c.imp.program)
                .args(&c.imp.prefix)
                .args(args)
                .env("TODO_FILE", c.todo_file.as_ref().unwrap())
                .stdout(closed_pipe())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            c.log
                .push(format!("closed stdout {args:?} -> {:?}", status.code()));
        }
    });
}

/// A pipe whose read end is already closed, so writes fail with EPIPE.
fn closed_pipe() -> Stdio {
    let mut fds = [0; 2];
    // SAFETY: fds has room for the two descriptors pipe() writes.
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    // SAFETY: fds[0] was just returned by pipe() and is closed exactly once.
    unsafe { libc::close(fds[0]) };
    // SAFETY: fds[1] is a valid descriptor owned by nothing else.
    unsafe { <Stdio as std::os::fd::FromRawFd>::from_raw_fd(fds[1]) }
}

#[test]
fn simultaneous_java_and_rust_processes_do_not_lose_changes() {
    let Some(java) = java_or_skip("simultaneous_java_and_rust") else {
        return;
    };
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("tasks.tsv");
    let per_side = 8;
    let mut children = Vec::new();
    for i in 0..per_side {
        for imp in [&java, &rust()] {
            let child = Command::new(&imp.program)
                .args(&imp.prefix)
                .args(["add", &format!("{} {i}", imp.name)])
                .env("TODO_FILE", &file)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            children.push(child);
        }
    }
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let content = fs::read_to_string(&file).unwrap();
    let mut ids: Vec<i32> = content
        .lines()
        .skip(2)
        .map(|line| line.split('\t').next().unwrap().parse().unwrap())
        .collect();
    ids.sort();
    assert_eq!(ids, (1..=2 * per_side).collect::<Vec<_>>(), "{content}");
    assert!(content.starts_with(&format!("#version\t2\n#next\t{}\n", 2 * per_side + 1)));
}

/// Holds an fcntl lock on the lock file from this process and checks that both versions wait
/// for it, which shows they use the same kind of lock and so exclude each other.
#[test]
fn both_versions_wait_for_the_same_lock() {
    let Some(java) = java_or_skip("both_versions_wait_for_the_same_lock") else {
        return;
    };
    for imp in [java, rust()] {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("tasks.tsv");
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.path().join("tasks.tsv.lock"))
            .unwrap();
        set_lock(&lock, libc::F_WRLCK);

        let mut child = Command::new(&imp.program)
            .args(&imp.prefix)
            .args(["add", "waits"])
            .env("TODO_FILE", &file)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        thread::sleep(Duration::from_millis(2500));
        assert!(
            child.try_wait().unwrap().is_none(),
            "{} did not wait for the lock",
            imp.name
        );
        assert!(!file.exists(), "{} wrote while the lock was held", imp.name);

        set_lock(&lock, libc::F_UNLCK);
        assert!(child.wait().unwrap().success());
        assert!(fs::read_to_string(&file).unwrap().contains("waits"));
    }
}

fn set_lock(file: &fs::File, kind: i32) {
    use std::os::fd::AsRawFd;
    // SAFETY: flock is a plain C struct; zeroed is a valid value.
    let mut request: libc::flock = unsafe { std::mem::zeroed() };
    request.l_type = kind as _;
    request.l_whence = libc::SEEK_SET as _;
    // SAFETY: valid open descriptor and initialized request.
    assert_ne!(
        unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLKW, &request) },
        -1
    );
}

/// Compares Java's UTF-8 decoding of many byte strings, weighted towards malformed input, with
/// `compat::decode_utf8_lossy`, which the Rust binary uses for its arguments.
#[test]
fn argument_decoding_matches_java_on_random_bytes() {
    if java_or_skip("argument_decoding_matches_java_on_random_bytes").is_none() {
        return;
    }
    let interesting = [
        0x00, 0x20, 0x41, 0x7F, 0x80, 0x8F, 0x90, 0x9F, 0xA0, 0xBF, 0xC0, 0xC1, 0xC2, 0xC3, 0xDF,
        0xE0, 0xE1, 0xED, 0xEE, 0xEF, 0xF0, 0xF1, 0xF3, 0xF4, 0xF5, 0xF7, 0xF8, 0xFE, 0xFF,
    ];
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut samples: Vec<Vec<u8>> = Vec::new();
    for _ in 0..20_000 {
        let len = (next() % 9) as usize;
        let bytes = (0..len)
            .map(|_| {
                let r = next();
                if r % 4 == 0 {
                    (r >> 8) as u8
                } else {
                    interesting[(r >> 8) as usize % interesting.len()]
                }
            })
            .collect();
        samples.push(bytes);
    }
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("samples.hex");
    let to_hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let mut lines: Vec<String> = samples.iter().map(|s| to_hex(s)).collect();
    lines.push(String::new());
    fs::write(&input, lines.join("\n")).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/java/DecodeUtf8.java");
    let output = Command::new("java")
        .arg(source)
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let java_lines: Vec<&str> = std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .collect();
    assert_eq!(java_lines.len(), samples.len());
    for (bytes, java_hex) in samples.iter().zip(java_lines) {
        let rust_hex = to_hex(todo_cli::compat::decode_utf8_lossy(bytes).as_bytes());
        assert_eq!(rust_hex, java_hex, "input {}", to_hex(bytes));
    }
    eprintln!(
        "parity argument decoding: {} byte strings match",
        samples.len()
    );
}

const NATIVE_DIGIT_LOCALES: &[&str] = &[
    "ar_BH", "ar_EG", "ar_IN", "ar_IQ", "ar_JO", "ar_KW", "ar_LB", "ar_OM", "ar_QA", "ar_SA",
    "ar_SD", "ar_SS", "ar_SY", "ar_YE", "as_IN", "bho_IN", "bho_NP", "bn_BD", "bn_IN", "ckb_IQ",
    "dz_BT", "fa_IR", "ks_IN", "mni_IN", "mr_IN", "my_MM", "ne_NP", "ps_AF", "raj_IN", "sa_IN",
    "sat_IN", "sd_IN", "sd_PK", "ur_IN",
];

const ASCII_DIGIT_LOCALES: &[&str] = &[
    "ar_AE.UTF-8",
    "ar_DZ.UTF-8",
    "ar_LY.UTF-8",
    "ar_MA.UTF-8",
    "ar_TN.UTF-8",
    "pa_IN.UTF-8",
    "pa_PK.UTF-8",
    "ur_PK.UTF-8",
    "th_TH.UTF-8",
    "tr_TR.UTF-8",
    "uz_UZ.UTF-8",
    "ks_IN.UTF-8@devanagari",
    "sd_IN.UTF-8@devanagari",
    "uz_UZ.UTF-8@cyrillic",
    "de_DE.UTF-8",
    "en_US.UTF-8",
    "C.UTF-8",
];

fn locale_scenario(c: &mut Ctx) {
    c.write(
        "tasks.tsv",
        "1\t0\tone\n12\t1\thigh\ttwelve\n123\t0\tlow\tLOW\n1234\t0\tİi\n",
    );
    c.run(&["list"]);
    c.run(&["list", "--sort", "PRIORITY"]);
    c.run(&["add", "-p", "HIGH", "x"]);
    c.run(&["complete", "\u{661}"]);
    c.run(&["list", "--sort", "priority"]);
    c.run(&["delete", "7"]);
}

/// Task ids in `list` use native digits in some locales; checks every glibc locale for which
/// Java does that (in UTF-8 and with the encoding left out) and a sample of those where it does
/// not. Only UTF-8 locales are compared; see `non_utf8_locale_difference_is_pinned`.
#[test]
fn locale_digits() {
    let mut names: Vec<String> = Vec::new();
    for base in NATIVE_DIGIT_LOCALES {
        names.push(format!("{base}.UTF-8"));
    }
    names.extend(["ar_SA.utf8".to_owned(), "fa_IR.utf8".to_owned()]);
    names.extend(ASCII_DIGIT_LOCALES.iter().map(|s| (*s).to_owned()));
    let installed = Command::new("locale")
        .arg("-a")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    // A locale that is not installed falls back to C, whose character set is not UTF-8.
    names.retain(|name| {
        let base = name.split(['.', '@']).next().unwrap();
        let present = name == "C.UTF-8"
            || installed
                .lines()
                .any(|l| l.starts_with(&format!("{base}.utf8")));
        if !present {
            eprintln!("note: locale {name} is not installed; skipping it");
        }
        present
    });
    thread::scope(|scope| {
        for chunk in names.chunks(names.len().div_ceil(4)) {
            scope.spawn(move || {
                for name in chunk {
                    check(&format!("locale_digits[{name}]"), |c| {
                        c.locale(&[("LC_ALL", name)]);
                        locale_scenario(c);
                    });
                }
            });
        }
    });
}

/// Java formats with the LC_CTYPE locale and translates system error messages with LC_MESSAGES.
#[test]
fn locale_categories_and_translated_errors() {
    let combos: &[&[(&'static str, &str)]] = &[
        &[("LANG", "ar_EG.UTF-8")],
        &[("LANG", "ar_EG.UTF-8"), ("LC_CTYPE", "en_US.UTF-8")],
        &[("LANG", "en_US.UTF-8"), ("LC_CTYPE", "ar_EG.UTF-8")],
        &[("LANG", "en_US.UTF-8"), ("LC_NUMERIC", "ar_EG.UTF-8")],
        &[("LC_ALL", "de_DE.UTF-8")],
        &[("LANG", "de_DE.UTF-8"), ("LC_MESSAGES", "C")],
        &[("LANG", "C.UTF-8"), ("LC_MESSAGES", "de_DE.UTF-8")],
        &[("LC_ALL", "fr_FR.UTF-8")],
    ];
    for (i, vars) in combos.iter().enumerate() {
        check(&format!("locale_categories[{i}]"), |c| {
            c.locale(vars);
            locale_scenario(c);
            c.write("afile", "x");
            c.todo_file("afile/sub/tasks.tsv");
            c.run(&["add", "x"]);
            c.todo_file("afile");
            c.run(&["list"]);
            c.mkdir("folder");
            c.todo_file("folder");
            c.run(&["list"]);
            c.mkdir("lockdir.tsv.lock");
            c.todo_file("lockdir.tsv");
            c.run(&["add", "x"]);
        });
    }
}

/// Known, intended difference: when the locale's character set is not UTF-8 (for example
/// `LC_ALL=C`, or a locale that is not installed), Java decodes arguments and encodes output in
/// that character set, so non-ASCII text is printed as `?` and saved as U+FFFD. The Rust version
/// always uses UTF-8, matching the task file's encoding.
#[test]
fn non_utf8_locale_difference_is_pinned() {
    let Some(java) = java_or_skip("non_utf8_locale_difference_is_pinned") else {
        return;
    };
    let mut outputs = Vec::new();
    for imp in [java, rust()] {
        let mut c = Ctx::new(imp);
        c.locale(&[("LC_ALL", "C")]);
        c.run(&["add", "café ✓"]);
        let saved = fs::read_to_string(c.path("tasks.tsv")).unwrap();
        outputs.push((c.log.pop().unwrap(), saved));
    }
    assert!(
        outputs[0].0.contains("Added task 1 (medium): caf?? ???"),
        "{}",
        outputs[0].0
    );
    let replaced = "\tcaf\u{FFFD}\u{FFFD} \u{FFFD}\u{FFFD}\u{FFFD}\n";
    assert!(outputs[0].1.ends_with(replaced), "{}", outputs[0].1);
    assert!(
        outputs[1].0.contains("Added task 1 (medium): café ✓"),
        "{}",
        outputs[1].0
    );
    assert!(outputs[1].1.ends_with("\tcafé ✓\n"), "{}", outputs[1].1);
}
