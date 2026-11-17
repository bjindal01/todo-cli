# todo-cli

A small command-line to-do app. Requires Java 21+ and Maven.

## Build and test

```bash
mvn package          # compiles, runs the JUnit tests, builds target/todo.jar
mvn test             # tests only
```

## Use

```bash
java -jar target/todo.jar add Buy milk                 # medium priority by default
java -jar target/todo.jar add -p high Fix the leak     # or --priority high / --priority=high
java -jar target/todo.jar list                         # by id
java -jar target/todo.jar list --sort priority         # high, then medium, then low
java -jar target/todo.jar complete 1
java -jar target/todo.jar delete 1
```

Priorities are `high`, `medium` and `low`, and neither priorities nor `--sort` values are case-sensitive. Within the same priority, `list --sort priority` keeps tasks in id order.

Put `-p` before the task text. Anything after the text is part of it, so `todo add Fix bug -p high` adds the text "Fix bug -p high" at medium priority. To add a task whose text starts with `-p`, put `--` before it: `todo add -- -p is a flag`.

Tasks are saved to `~/.todo.tsv`. Set `TODO_FILE` to use a different file. If that path is a symlink, the file it points to is updated and the link is kept. Files saved by earlier versions still load, with every task at medium priority.

Commands that change tasks take a lock on `<task file>.lock` (for example `~/.todo.tsv.lock`), so commands run at the same time wait for each other and no change is lost. The lock file is left in place; it is safe to delete when no `todo` command is running.

Exit codes: `0` success, `1` error (unknown task id, unreadable or damaged file, no task ids left), `2` usage error.
