package com.example.todo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class AppTest {

    @TempDir
    Path dir;

    private Path file;
    private ByteArrayOutputStream out;
    private ByteArrayOutputStream err;

    @BeforeEach
    void setUp() {
        file = dir.resolve("tasks.tsv");
    }

    /** Runs one command in a fresh App, the way separate shell invocations would. */
    private int run(String... args) {
        out = new ByteArrayOutputStream();
        err = new ByteArrayOutputStream();
        App app = new App(
                new TaskStore(file),
                new PrintStream(out, true, StandardCharsets.UTF_8),
                new PrintStream(err, true, StandardCharsets.UTF_8));
        return app.run(args);
    }

    private String out() {
        return normalizeLineEndings(out);
    }

    private String err() {
        return normalizeLineEndings(err);
    }

    /** println uses the platform line separator; compare with \n everywhere. */
    private static String normalizeLineEndings(ByteArrayOutputStream stream) {
        return stream.toString(StandardCharsets.UTF_8).replace(System.lineSeparator(), "\n");
    }

    @Test
    void addListCompleteWorkflowPersistsBetweenRuns() {
        assertEquals(App.OK, run("add", "Buy", "milk"));
        assertEquals("Added task 1 (medium): Buy milk\n", out());

        assertEquals(App.OK, run("add", "Walk the dog"));
        assertEquals("Added task 2 (medium): Walk the dog\n", out());

        assertEquals(App.OK, run("complete", "1"));
        assertEquals("Completed task 1: Buy milk\n", out());

        assertEquals(App.OK, run("list"));
        assertEquals("  1. [x] (medium) Buy milk\n  2. [ ] (medium) Walk the dog\n", out());
        assertEquals("", err());
    }

    @Test
    void listOnEmptyStoreSaysSoAndDoesNotCreateFile() {
        assertEquals(App.OK, run("list"));

        assertTrue(out().startsWith("No tasks yet."));
        assertFalse(Files.exists(file));
    }

    @Test
    void completingAlreadyCompletedTaskIsHarmless() {
        run("add", "x");
        run("complete", "1");

        assertEquals(App.OK, run("complete", "1"));
        assertEquals("Task 1 is already completed.\n", out());
    }

    @Test
    void completeUnknownIdIsAnErrorAndLeavesFileUnchanged() throws IOException {
        run("add", "x");
        String before = Files.readString(file);

        assertEquals(App.ERROR, run("complete", "9"));
        assertEquals("Error: No task with id 9.\n", err());
        assertEquals(before, Files.readString(file));
    }

    @Test
    void noArgumentsPrintsUsage() {
        assertEquals(App.USAGE, run());
        assertTrue(err().contains("Usage: todo"));
    }

    @Test
    void helpPrintsUsageToStdout() {
        assertEquals(App.OK, run("--help"));
        assertTrue(out().contains("Usage: todo"));
    }

    @Test
    void badArgumentsAreUsageErrors() {
        assertEquals(App.USAGE, run("frobnicate"));
        assertTrue(err().contains("Unknown command: frobnicate"));

        assertEquals(App.USAGE, run("add"));
        assertEquals(App.USAGE, run("add", "   "));
        assertEquals(App.USAGE, run("complete"));
        assertEquals(App.USAGE, run("complete", "1", "2"));
        assertEquals(App.USAGE, run("complete", "abc"));
        assertTrue(err().contains("Task id must be a number: abc"));
        assertEquals(App.USAGE, run("list", "extra"));

        assertFalse(Files.exists(file));
    }

    @Test
    void corruptFileIsReportedAsError() throws IOException {
        Files.writeString(file, "garbage\n");

        assertEquals(App.ERROR, run("list"));
        assertTrue(err().startsWith("Error: Could not read"), err());
    }

    @Test
    void deleteRemovesTaskAndKeepsOthersAcrossRuns() {
        run("add", "one");
        run("add", "two");
        run("add", "three");

        assertEquals(App.OK, run("delete", "2"));
        assertEquals("Deleted task 2: two\n", out());
        assertEquals("", err());

        run("list");
        assertEquals("  1. [ ] (medium) one\n  3. [ ] (medium) three\n", out());
    }

    @Test
    void deleteWorksOnCompletedTasks() {
        run("add", "one");
        run("complete", "1");

        assertEquals(App.OK, run("delete", "1"));

        run("list");
        assertTrue(out().startsWith("No tasks yet."));
    }

    @Test
    void idsAreNotReusedAfterDeletingTheNewestTask() {
        run("add", "one");
        run("add", "two");
        run("delete", "2");

        run("add", "three");
        assertEquals("Added task 3 (medium): three\n", out());

        assertEquals(App.ERROR, run("complete", "2"));
    }

    @Test
    void deleteUnknownIdIsAnErrorAndLeavesFileUnchanged() throws IOException {
        run("add", "x");
        String before = Files.readString(file);

        assertEquals(App.ERROR, run("delete", "9"));
        assertEquals("Error: No task with id 9.\n", err());
        assertEquals(before, Files.readString(file));
    }

    @Test
    void deleteTwiceReportsTheSecondAsUnknown() {
        run("add", "x");
        run("delete", "1");

        assertEquals(App.ERROR, run("delete", "1"));
        assertEquals("Error: No task with id 1.\n", err());
    }

    @Test
    void deleteArgumentErrorsAreUsageErrors() {
        assertEquals(App.USAGE, run("delete"));
        assertTrue(err().contains("delete needs exactly one task id"));
        assertEquals(App.USAGE, run("delete", "1", "2"));
        assertEquals(App.USAGE, run("delete", "abc"));
        assertTrue(err().contains("Task id must be a number: abc"));

        assertFalse(Files.exists(file));
    }

    @Test
    void usageListsDeleteCommand() {
        run("--help");
        assertTrue(out().contains("delete <id>"));
    }

    @Test
    void addWithPriorityFlagShowsPriority() {
        assertEquals(App.OK, run("add", "-p", "high", "Fix", "bug"));
        assertEquals("Added task 1 (high): Fix bug\n", out());

        assertEquals(App.OK, run("add", "--priority", "LOW", "Tidy desk"));
        assertEquals("Added task 2 (low): Tidy desk\n", out());

        assertEquals(App.OK, run("add", "--priority=medium", "Read book"));
        assertEquals("Added task 3 (medium): Read book\n", out());

        run("list");
        assertEquals("  1. [ ] (high) Fix bug\n  2. [ ] (low) Tidy desk\n  3. [ ] (medium) Read book\n", out());
    }

    @Test
    void textStartingWithDashIsNotTreatedAsAnOption() {
        assertEquals(App.OK, run("add", "-5", "degrees", "outside"));
        assertEquals("Added task 1 (medium): -5 degrees outside\n", out());

        assertEquals(App.OK, run("add", "--", "-p", "is", "a", "flag"));
        assertEquals("Added task 2 (medium): -p is a flag\n", out());
    }

    @Test
    void invalidPriorityArgumentsAreUsageErrors() {
        assertEquals(App.USAGE, run("add", "-p", "urgent", "x"));
        assertTrue(err().contains("Priority must be high, medium or low: urgent"));

        assertEquals(App.USAGE, run("add", "-p"));
        assertTrue(err().contains("-p needs a priority"));

        assertEquals(App.USAGE, run("add", "-p", "high"));
        assertTrue(err().contains("add needs the task text"));

        assertFalse(Files.exists(file));
    }

    @Test
    void listSortedByPriorityPutsHighFirstAndKeepsIdOrderForTies() {
        run("add", "-p", "low", "a");
        run("add", "b");
        run("add", "-p", "high", "c");
        run("add", "-p", "low", "d");
        run("add", "-p", "high", "e");
        run("complete", "5");
        String expected = "  3. [ ] (high) c\n"
                + "  5. [x] (high) e\n"
                + "  2. [ ] (medium) b\n"
                + "  1. [ ] (low) a\n"
                + "  4. [ ] (low) d\n";

        assertEquals(App.OK, run("list", "--sort", "priority"));
        assertEquals(expected, out());

        assertEquals(App.OK, run("list", "--sort=priority"));
        assertEquals(expected, out());
    }

    @Test
    void listSortById() {
        run("add", "-p", "low", "a");
        run("add", "-p", "high", "b");

        assertEquals(App.OK, run("list", "--sort", "id"));
        assertEquals("  1. [ ] (low) a\n  2. [ ] (high) b\n", out());
    }

    @Test
    void listSortedByPriorityOnEmptyStore() {
        assertEquals(App.OK, run("list", "--sort", "priority"));
        assertTrue(out().startsWith("No tasks yet."));
    }

    @Test
    void badListOptionsAreUsageErrors() {
        assertEquals(App.USAGE, run("list", "--sort", "name"));
        assertTrue(err().contains("Sort must be id or priority: name"));
        assertEquals(App.USAGE, run("list", "--sort"));
        assertEquals(App.USAGE, run("list", "--sort", "priority", "extra"));
        assertEquals(App.USAGE, run("list", "priority"));
    }

    @Test
    void completeKeepsPriority() {
        run("add", "-p", "high", "x");
        run("complete", "1");

        run("list");
        assertEquals("  1. [x] (high) x\n", out());
    }

    @Test
    void existingFileWithoutPrioritiesListsTasksAsMedium() throws IOException {
        Files.writeString(file, "1\t0\told one\n2\t1\told two\n");
        run("add", "-p", "high", "new");

        run("list", "--sort", "priority");
        assertEquals("  3. [ ] (high) new\n  1. [ ] (medium) old one\n  2. [x] (medium) old two\n", out());
    }

    @Test
    void priorityOptionEdgeCases() {
        assertEquals(App.USAGE, run("add", "--priority"));
        assertTrue(err().contains("Error: --priority needs a priority: high, medium or low"), err());

        assertEquals(App.USAGE, run("add", "--priority=", "x"));
        assertTrue(err().contains("Error: Priority must be high, medium or low: \n"), err());

        assertEquals(App.USAGE, run("add", "--priority=urgent", "x"));
        assertTrue(err().contains("Error: Priority must be high, medium or low: urgent"), err());
        assertFalse(Files.exists(file));

        assertEquals(App.OK, run("add", "-p", "high", "-p", "low", "x"));
        assertEquals("Added task 1 (low): x\n", out());

        assertEquals(App.OK, run("add", "--", "-p", "high", "y"));
        assertEquals("Added task 2 (medium): -p high y\n", out());
    }

    @Test
    void optionsAfterTheTextArePartOfTheText() {
        assertEquals(App.OK, run("add", "Fix", "bug", "-p", "high"));
        assertEquals("Added task 1 (medium): Fix bug -p high\n", out());
    }

    @Test
    void usageSaysPriorityGoesBeforeText() {
        run("--help");
        assertTrue(out().contains("put -p before the text"), out());
    }

    @Test
    void sortOptionEdgeCases() {
        run("add", "-p", "low", "a");
        run("add", "-p", "high", "b");

        assertEquals(App.OK, run("list", "--sort=id"));
        assertEquals("  1. [ ] (low) a\n  2. [ ] (high) b\n", out());

        assertEquals(App.OK, run("list", "--sort", "Priority"));
        assertEquals("  2. [ ] (high) b\n  1. [ ] (low) a\n", out());

        assertEquals(App.OK, run("list", "--sort=PRIORITY"));
        assertEquals("  2. [ ] (high) b\n  1. [ ] (low) a\n", out());

        assertEquals(App.USAGE, run("list", "--sort="));
        assertTrue(err().contains("Error: Sort must be id or priority: \n"), err());

        assertEquals(App.USAGE, run("list", "--sort"));
        assertTrue(err().contains("Error: list accepts only --sort id or --sort priority"), err());

        assertEquals(App.USAGE, run("list", "--sort", "priority", "extra"));
        assertTrue(err().contains("Error: list accepts only --sort id or --sort priority"), err());
    }

    @Test
    void runningOutOfIdsIsAnErrorNotAStackTrace() throws IOException {
        Files.writeString(file, "2147483647\t0\tlast\n");
        String before = Files.readString(file);

        assertEquals(App.ERROR, run("add", "one more"));
        assertEquals("Error: No more task ids are available.\n", err());
        assertEquals(before, Files.readString(file));
    }

    @Test
    void duplicateIdsInFileAreReportedNotHalfHandled() throws IOException {
        Files.writeString(file, "1\t0\tx\n1\t0\ty\n");

        assertEquals(App.ERROR, run("complete", "1"));
        assertTrue(err().contains("line 2: duplicate id 1"), err());
    }

    @Test
    void invalidUtf8FileIsReportedClearly() throws IOException {
        Files.write(file, new byte[] {(byte) 0xFF, '\n'});

        assertEquals(App.ERROR, run("list"));
        assertEquals("Error: Could not read " + file + ": it is not valid UTF-8 text.\n", err());
    }

    @Test
    void multiLineTaskIsListedOnOneLine() {
        run("add", "first\nsecond");

        run("list");
        assertEquals("  1. [ ] (medium) first second\n", out());
    }
}
