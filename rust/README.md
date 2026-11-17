# todo-cli (Rust)

A Rust port of the Java `todo-cli` in the parent folder. It has the same commands, options,
messages, exit codes and task file format, so it can replace the Java version and keep using
the same `~/.todo.tsv`. Requires a stable Rust toolchain (1.85 or newer) on Linux or macOS.

## Build and test

```bash
cargo build --release      # builds target/release/todo
cargo test                 # unit, integration and Java parity tests
```

The parity tests in `tests/parity.rs` run the same commands against the Java build and the Rust
binary and compare stdout, stderr, exit codes and the resulting files. They need Java 21 and the
jar from `mvn package` in the repository root (or set `TODO_JAVA_JAR`). Without them those tests
are skipped with a message; set `TODO_REQUIRE_PARITY=1` to make that an error.

## Use

```bash
todo add Buy milk                 # medium priority by default
todo add -p high Fix the leak     # or --priority high / --priority=high
todo list                         # by id
todo list --sort priority         # high, then medium, then low
todo complete 1
todo delete 1
```

Everything in the [main README](../README.md) about priorities, `--`, `TODO_FILE`, symlinks,
locking and exit codes applies unchanged. The lock is an `fcntl` lock on `<task file>.lock`,
the same kind Java takes, so the Rust and Java versions can safely run at the same time.

## Differences from the Java version

- Text is always read from arguments and written to the terminal as UTF-8. Java uses the
  locale's character set, so under a non-UTF-8 locale (such as `LC_ALL=C`) it printed non-ASCII
  text as `?` and saved it as U+FFFD. Under UTF-8 locales the output is identical.
- `TODO_FILE` is used byte for byte. Java decoded it with the locale's character set first, so a
  path that is not valid in that character set named a different file.
- `TODO_FILE=/`, which names no file, makes `add`, `complete` and `delete` print
  `Error: Could not lock /: the path does not name a file.` Java printed an internal
  `NullPointerException` message. Both exit with code 1.
- Unix-like systems only (Linux, macOS); the Java version also ran on Windows.

## Layout

- `src/app.rs`: command-line parsing, messages and exit codes
- `src/store.rs`: task file reading, writing, locking and symlink handling
- `src/todo_list.rs`, `src/task.rs`, `src/priority.rs`: the task model
- `src/compat.rs`, `src/locale.rs`: Java library behavior the original relied on (whitespace
  trimming, number parsing, path handling, argument decoding, locale digits)
- `tests/`: ports of the Java tests plus the Java/Rust parity tests
