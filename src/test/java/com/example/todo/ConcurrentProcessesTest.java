package com.example.todo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import java.util.stream.IntStream;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/** Runs the real CLI in separate JVMs at the same time, the way overlapping shell commands would. */
class ConcurrentProcessesTest {

    private static final int PROCESSES = 8;

    @TempDir
    Path dir;

    @Test
    void simultaneousAddsAllSurviveWithDistinctIds() throws Exception {
        Path file = dir.resolve("tasks.tsv");

        List<Process> processes = new ArrayList<>();
        for (int i = 1; i <= PROCESSES; i++) {
            processes.add(start(file, "add", "task " + i));
        }
        for (Process process : processes) {
            assertTrue(process.waitFor(60, TimeUnit.SECONDS), "todo add did not finish");
            assertEquals(0, process.exitValue(), new String(process.getErrorStream().readAllBytes()));
        }

        List<Task> tasks = new TaskStore(file).load().all();
        assertEquals(
                IntStream.rangeClosed(1, PROCESSES).boxed().toList(),
                tasks.stream().map(Task::id).sorted().toList());
        assertEquals(
                IntStream.rangeClosed(1, PROCESSES).mapToObj(i -> "task " + i).sorted().toList(),
                tasks.stream().map(Task::text).sorted().toList());
    }

    @Test
    void simultaneousCompletesAndDeletesKeepEveryOtherChange() throws Exception {
        Path file = dir.resolve("tasks.tsv");
        TaskStore store = new TaskStore(file);
        TodoList list = new TodoList();
        for (int i = 1; i <= PROCESSES; i++) {
            list.add("task " + i);
        }
        store.save(list);

        List<Process> processes = new ArrayList<>();
        for (int i = 1; i <= PROCESSES; i++) {
            String command = i % 2 == 0 ? "delete" : "complete";
            processes.add(start(file, command, String.valueOf(i)));
        }
        for (Process process : processes) {
            assertTrue(process.waitFor(60, TimeUnit.SECONDS), "todo did not finish");
            assertEquals(0, process.exitValue(), new String(process.getErrorStream().readAllBytes()));
        }

        List<Task> remaining = store.load().all();
        assertEquals(List.of(1, 3, 5, 7), remaining.stream().map(Task::id).toList());
        assertTrue(remaining.stream().allMatch(Task::done), remaining.toString());
    }

    private static Process start(Path file, String... args) throws IOException {
        String java = ProcessHandle.current().info().command().orElse("java");
        List<String> command = new ArrayList<>(List.of(
                java, "-cp", System.getProperty("java.class.path"), App.class.getName()));
        command.addAll(List.of(args));
        ProcessBuilder builder = new ProcessBuilder(command);
        builder.environment().put("TODO_FILE", file.toString());
        builder.redirectOutput(ProcessBuilder.Redirect.DISCARD);
        return builder.start();
    }
}
