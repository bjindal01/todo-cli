package com.example.todo;

import java.io.Closeable;
import java.io.IOException;
import java.io.PrintStream;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.Optional;

public final class App {

    static final int OK = 0;
    static final int ERROR = 1;
    static final int USAGE = 2;

    static final String USAGE_TEXT = """
            Usage: todo <command> [arguments]

            Commands:
              add [-p high|medium|low] <text...>   Add a task (priority defaults to medium;
                                                   put -p before the text)
              list [--sort id|priority]            Show all tasks, by id (default) or priority
              complete <id>                        Mark a task as done
              delete <id>                          Remove a task

            Tasks are stored in the file named by TODO_FILE,
            or ~/.todo.tsv if TODO_FILE is not set.""";

    private final TaskStore store;
    private final PrintStream out;
    private final PrintStream err;

    public App(TaskStore store, PrintStream out, PrintStream err) {
        this.store = store;
        this.out = out;
        this.err = err;
    }

    public static void main(String[] args) {
        App app = new App(new TaskStore(defaultFile()), System.out, System.err);
        System.exit(app.run(args));
    }

    static Path defaultFile() {
        String configured = System.getenv("TODO_FILE");
        if (configured != null && !configured.isBlank()) {
            return Path.of(configured);
        }
        return Path.of(System.getProperty("user.home"), ".todo.tsv");
    }

    public int run(String... args) {
        if (args.length == 0) {
            err.println(USAGE_TEXT);
            return USAGE;
        }
        String command = args[0];
        List<String> rest = Arrays.asList(args).subList(1, args.length);
        try {
            return switch (command) {
                case "add" -> add(rest);
                case "list" -> list(rest);
                case "complete" -> complete(rest);
                case "delete" -> delete(rest);
                case "help", "-h", "--help" -> {
                    out.println(USAGE_TEXT);
                    yield OK;
                }
                default -> usage("Unknown command: " + command);
            };
        } catch (IOException | RuntimeException e) {
            err.println("Error: " + e.getMessage());
            return ERROR;
        }
    }

    private int add(List<String> rest) throws IOException {
        Priority priority = Priority.MEDIUM;
        int i = 0;
        // Only the priority option is recognized; any other word, even one starting with "-",
        // begins the task text, so "todo add -5 degrees" keeps working.
        while (i < rest.size()) {
            String arg = rest.get(i);
            String value;
            if (arg.equals("--")) {
                i++;
                break;
            } else if (arg.equals("-p") || arg.equals("--priority")) {
                if (i + 1 >= rest.size()) {
                    return usage(arg + " needs a priority: high, medium or low");
                }
                value = rest.get(i + 1);
                i += 2;
            } else if (arg.startsWith("--priority=")) {
                value = arg.substring("--priority=".length());
                i++;
            } else {
                break;
            }
            Optional<Priority> parsed = Priority.parse(value);
            if (parsed.isEmpty()) {
                return usage("Priority must be high, medium or low: " + value);
            }
            priority = parsed.get();
        }
        String text = String.join(" ", rest.subList(i, rest.size())).strip();
        if (text.isEmpty()) {
            return usage("add needs the task text, for example: todo add -p high Buy milk");
        }
        Task task;
        try (Closeable ignored = store.lock()) {
            TodoList list = store.load();
            task = list.add(text, priority);
            store.save(list);
        }
        out.println("Added task " + task.id() + " (" + task.priority().label() + "): " + task.text());
        return OK;
    }

    private int list(List<String> rest) throws IOException {
        String sort;
        if (rest.isEmpty()) {
            sort = "id";
        } else if (rest.size() == 2 && rest.get(0).equals("--sort")) {
            sort = rest.get(1);
        } else if (rest.size() == 1 && rest.get(0).startsWith("--sort=")) {
            sort = rest.get(0).substring("--sort=".length());
        } else {
            return usage("list accepts only --sort id or --sort priority");
        }
        String normalized = sort.strip().toLowerCase(Locale.ROOT);
        if (!normalized.equals("id") && !normalized.equals("priority")) {
            return usage("Sort must be id or priority: " + sort);
        }
        TodoList list = store.load();
        List<Task> tasks = normalized.equals("priority") ? list.byPriority() : list.all();
        if (tasks.isEmpty()) {
            out.println("No tasks yet. Add one with: todo add <text>");
            return OK;
        }
        for (Task task : tasks) {
            out.println(format(task));
        }
        return OK;
    }

    private int complete(List<String> rest) throws IOException {
        if (rest.size() != 1) {
            return usage("complete needs exactly one task id, for example: todo complete 2");
        }
        Integer id = parseId(rest.get(0));
        if (id == null) {
            return usage("Task id must be a number: " + rest.get(0));
        }
        Task before;
        try (Closeable ignored = store.lock()) {
            TodoList list = store.load();
            try {
                before = list.complete(id);
            } catch (TaskNotFoundException e) {
                err.println("Error: " + e.getMessage());
                return ERROR;
            }
            if (before.done()) {
                out.println("Task " + id + " is already completed.");
                return OK;
            }
            store.save(list);
        }
        out.println("Completed task " + id + ": " + before.text());
        return OK;
    }

    private int delete(List<String> rest) throws IOException {
        if (rest.size() != 1) {
            return usage("delete needs exactly one task id, for example: todo delete 2");
        }
        Integer id = parseId(rest.get(0));
        if (id == null) {
            return usage("Task id must be a number: " + rest.get(0));
        }
        Task removed;
        try (Closeable ignored = store.lock()) {
            TodoList list = store.load();
            try {
                removed = list.delete(id);
            } catch (TaskNotFoundException e) {
                err.println("Error: " + e.getMessage());
                return ERROR;
            }
            store.save(list);
        }
        out.println("Deleted task " + id + ": " + removed.text());
        return OK;
    }

    private static Integer parseId(String value) {
        try {
            return Integer.parseInt(value);
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private int usage(String message) {
        err.println("Error: " + message);
        err.println();
        err.println(USAGE_TEXT);
        return USAGE;
    }

    static String format(Task task) {
        String text = task.text().replace("\r", " ").replace("\n", " ");
        return String.format("%3d. [%s] (%s) %s", task.id(), task.done() ? "x" : " ", task.priority().label(), text);
    }
}
