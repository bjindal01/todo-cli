package com.example.todo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.FileSystems;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.PosixFilePermissions;
import java.util.List;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class TaskStoreTest {

    @TempDir
    Path dir;

    @Test
    void missingFileLoadsAsEmptyList() throws IOException {
        TaskStore store = new TaskStore(dir.resolve("none.tsv"));

        assertTrue(store.load().all().isEmpty());
    }

    @Test
    void saveThenLoadRoundTripsTasks() throws IOException {
        TaskStore store = new TaskStore(dir.resolve("tasks.tsv"));
        TodoList list = new TodoList();
        list.add("Buy milk");
        list.add("Pay rent");
        list.complete(1);

        store.save(list);

        assertEquals(
                List.of(new Task(1, "Buy milk", true), new Task(2, "Pay rent", false)),
                store.load().all());
    }

    @Test
    void textWithTabsNewlinesBackslashesAndUnicodeSurvivesRoundTrip() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        TaskStore store = new TaskStore(file);
        String tricky = "a\tb\nc\\d\\te \r café ✓";
        TodoList list = new TodoList(List.of(new Task(1, tricky, false)));

        store.save(list);

        // Two header lines plus one task line: the escaping kept the task on a single line.
        assertEquals(3, Files.readAllLines(file, StandardCharsets.UTF_8).size());
        assertEquals(tricky, store.load().all().get(0).text());
    }

    @Test
    void saveCreatesMissingParentDirectoriesAndLeavesNoTempFiles() throws IOException {
        Path file = dir.resolve("nested/deeper/tasks.tsv");
        TaskStore store = new TaskStore(file);
        TodoList list = new TodoList();
        list.add("x");

        store.save(list);
        store.save(list);

        assertTrue(Files.exists(file));
        try (var entries = Files.list(file.getParent())) {
            assertEquals(List.of(file), entries.toList());
        }
    }

    @Test
    void blankLinesAreIgnored() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "\n1\t0\tone\n\n2\t1\ttwo\n");

        assertEquals(
                List.of(new Task(1, "one", false), new Task(2, "two", true)),
                new TaskStore(file).load().all());
    }

    @Test
    void corruptLineFailsWithLineNumber() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "1\t0\tone\nnot-a-task\n");

        IOException e = assertThrows(IOException.class, () -> new TaskStore(file).load());
        assertTrue(e.getMessage().contains("line 2"), e.getMessage());
    }

    @Test
    void invalidDoneFlagAndIdAreRejected() throws IOException {
        Path badFlag = dir.resolve("flag.tsv");
        Files.writeString(badFlag, "1\tyes\tone\n");
        Path badId = dir.resolve("id.tsv");
        Files.writeString(badId, "0\t0\tone\n");

        assertThrows(IOException.class, () -> new TaskStore(badFlag).load());
        assertThrows(IOException.class, () -> new TaskStore(badId).load());
    }

    @Test
    void nextIdSurvivesDeletingTheNewestTask() throws IOException {
        TaskStore store = new TaskStore(dir.resolve("tasks.tsv"));
        TodoList list = new TodoList();
        list.add("one");
        list.add("two");
        list.delete(2);

        store.save(list);
        TodoList reloaded = store.load();

        assertEquals(List.of(new Task(1, "one", false)), reloaded.all());
        assertEquals(3, reloaded.add("three").id());
    }

    @Test
    void fileWithoutNextIdLineLoadsWithIdAfterHighestTask() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "1\t0\tone\n4\t1\tfour\n");

        TodoList list = new TaskStore(file).load();

        assertEquals(List.of(new Task(1, "one", false), new Task(4, "four", true)), list.all());
        assertEquals(5, list.nextId());
    }

    @Test
    void savedFileStartsWithVersionAndNextIdLines() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        TaskStore store = new TaskStore(file);
        TodoList list = new TodoList();
        list.add("one");

        store.save(list);

        assertEquals(List.of("#version\t2", "#next\t2", "1\t0\tmedium\tone"), Files.readAllLines(file));
    }

    @Test
    void invalidNextIdLineIsRejectedWithLineNumber() throws IOException {
        Path notNumber = dir.resolve("nan.tsv");
        Files.writeString(notNumber, "#next\tabc\n1\t0\tone\n");
        Path notPositive = dir.resolve("zero.tsv");
        Files.writeString(notPositive, "1\t0\tone\n#next\t0\n");

        IOException e1 = assertThrows(IOException.class, () -> new TaskStore(notNumber).load());
        assertTrue(e1.getMessage().contains("line 1"), e1.getMessage());
        IOException e2 = assertThrows(IOException.class, () -> new TaskStore(notPositive).load());
        assertTrue(e2.getMessage().contains("line 2"), e2.getMessage());
    }

    @Test
    void priorityRoundTrips() throws IOException {
        TaskStore store = new TaskStore(dir.resolve("tasks.tsv"));
        TodoList list = new TodoList();
        list.add("a", Priority.HIGH);
        list.add("b", Priority.MEDIUM);
        list.add("c", Priority.LOW);

        store.save(list);

        assertEquals(
                List.of(
                        new Task(1, "a", false, Priority.HIGH),
                        new Task(2, "b", false, Priority.MEDIUM),
                        new Task(3, "c", false, Priority.LOW)),
                store.load().all());
    }

    @Test
    void linesWithoutPriorityLoadAsMediumAndCanBeMixedWithNewLines() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "#next\t4\n1\t0\told task\n2\t1\thigh\tnew task\n");

        TodoList list = new TaskStore(file).load();

        assertEquals(
                List.of(new Task(1, "old task", false, Priority.MEDIUM), new Task(2, "new task", true, Priority.HIGH)),
                list.all());
    }

    @Test
    void legacyFileIsRewrittenWithPriorityOnSave() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "1\t0\told task\n");
        TaskStore store = new TaskStore(file);

        store.save(store.load());

        assertEquals(List.of("#version\t2", "#next\t2", "1\t0\tmedium\told task"), Files.readAllLines(file));
    }

    @Test
    void invalidPriorityIsRejectedWithLineNumber() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "1\t0\thigh\tok\n2\t0\turgent\tbad\n");

        IOException e = assertThrows(IOException.class, () -> new TaskStore(file).load());
        assertTrue(e.getMessage().contains("line 2"), e.getMessage());
        assertTrue(e.getMessage().contains("priority"), e.getMessage());
    }

    @Test
    void duplicateIdsAreRejectedWithLineNumber() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "1\t0\tx\n1\t0\ty\n");

        IOException e = assertThrows(IOException.class, () -> new TaskStore(file).load());
        assertTrue(e.getMessage().contains("line 2: duplicate id 1"), e.getMessage());
    }

    @Test
    void invalidUtf8IsReportedWithFileName() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.write(file, new byte[] {(byte) 0xFF, '\n'});

        IOException e = assertThrows(IOException.class, () -> new TaskStore(file).load());
        assertEquals("Could not read " + file + ": it is not valid UTF-8 text.", e.getMessage());
    }

    @Test
    void unreadableFileIsReportedWithReason() throws IOException {
        assumeTrue(posix(), "needs POSIX permissions");
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "1\t0\tx\n");
        Files.setPosixFilePermissions(file, PosixFilePermissions.fromString("---------"));
        assumeTrue(!Files.isReadable(file), "running as a user who can read anything");

        IOException e = assertThrows(IOException.class, () -> new TaskStore(file).load());
        assertEquals("Could not read " + file + ": permission denied.", e.getMessage());
    }

    @Test
    void unwritableFolderIsReportedWithReason() throws IOException {
        assumeTrue(posix(), "needs POSIX permissions");
        Path folder = Files.createDirectory(dir.resolve("locked"));
        Path file = folder.resolve("tasks.tsv");
        Files.setPosixFilePermissions(folder, PosixFilePermissions.fromString("r-xr-xr-x"));
        try {
            assumeTrue(!Files.isWritable(folder), "running as a user who can write anything");
            TodoList list = new TodoList();
            list.add("x");

            IOException e = assertThrows(IOException.class, () -> new TaskStore(file).save(list));
            assertEquals("Could not write " + file + ": permission denied.", e.getMessage());
        } finally {
            Files.setPosixFilePermissions(folder, PosixFilePermissions.fromString("rwxr-xr-x"));
        }
    }

    @Test
    void versionedFileRequiresFourFieldLines() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "#version\t2\n#next\t3\n1\t0\thigh\tok\n2\t0\thigh\n");

        IOException e = assertThrows(IOException.class, () -> new TaskStore(file).load());
        assertTrue(e.getMessage().contains("line 4: expected 4 tab-separated fields"), e.getMessage());
    }

    @Test
    void unversionedFileWithWrongFieldCountNamesBothAcceptedCounts() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "1\t0\n");

        IOException e = assertThrows(IOException.class, () -> new TaskStore(file).load());
        assertTrue(e.getMessage().contains("expected 3 or 4 tab-separated fields"), e.getMessage());
    }

    @Test
    void unsupportedVersionIsRejected() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "#version\t3\n");

        IOException e = assertThrows(IOException.class, () -> new TaskStore(file).load());
        assertTrue(e.getMessage().contains("line 1: unsupported file version 3"), e.getMessage());
    }

    @Test
    void lastPossibleIdSurvivesSaveAndReload() throws IOException {
        TaskStore store = new TaskStore(dir.resolve("tasks.tsv"));
        TodoList list = new TodoList(List.of(new Task(Integer.MAX_VALUE - 1, "a", false)));
        list.add("b");

        store.save(list);
        TodoList reloaded = store.load();

        assertEquals(Integer.MAX_VALUE, reloaded.all().get(1).id());
        assertEquals(TodoList.MAX_NEXT_ID, reloaded.nextId());
    }

    @Test
    void nextIdBeyondLastPossibleIdIsRejected() throws IOException {
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "#next\t" + (TodoList.MAX_NEXT_ID + 1) + "\n");

        IOException e = assertThrows(IOException.class, () -> new TaskStore(file).load());
        assertTrue(e.getMessage().contains("next id is too large"), e.getMessage());
    }

    @Test
    void saveThroughSymlinkUpdatesTargetAndKeepsLinkAndPermissions() throws IOException {
        assumeTrue(posix(), "needs POSIX symlinks and permissions");
        Path real = dir.resolve("real.tsv");
        Files.writeString(real, "1\t0\told\n");
        Files.setPosixFilePermissions(real, PosixFilePermissions.fromString("rw-r--r--"));
        Path link = Files.createSymbolicLink(dir.resolve("link.tsv"), real.getFileName());
        TaskStore store = new TaskStore(link);

        TodoList list = store.load();
        list.add("new");
        store.save(list);

        assertTrue(Files.isSymbolicLink(link));
        assertEquals(2, new TaskStore(real).load().all().size());
        assertEquals("rw-r--r--", PosixFilePermissions.toString(Files.getPosixFilePermissions(real)));
    }

    @Test
    void saveThroughDanglingSymlinkCreatesTarget() throws IOException {
        assumeTrue(posix(), "needs POSIX symlinks");
        Path real = dir.resolve("sub/real.tsv");
        Path link = Files.createSymbolicLink(dir.resolve("link.tsv"), real);
        TaskStore store = new TaskStore(link);
        TodoList list = new TodoList();
        list.add("x");

        store.save(list);

        assertTrue(Files.isSymbolicLink(link));
        assertEquals(1, new TaskStore(real).load().all().size());
    }

    @Test
    void savingKeepsExistingFilePermissions() throws IOException {
        assumeTrue(posix(), "needs POSIX permissions");
        Path file = dir.resolve("tasks.tsv");
        Files.writeString(file, "");
        Files.setPosixFilePermissions(file, PosixFilePermissions.fromString("rw-rw-r--"));
        TodoList list = new TodoList();
        list.add("x");

        new TaskStore(file).save(list);

        assertEquals("rw-rw-r--", PosixFilePermissions.toString(Files.getPosixFilePermissions(file)));
    }

    private static boolean posix() {
        return FileSystems.getDefault().supportedFileAttributeViews().contains("posix");
    }

    @Test
    void escapeAndUnescapeAreInverse() {
        String original = "\\t is not a tab, but \t is";
        String escaped = TaskStore.escape(original);

        assertFalse(escaped.contains("\t"));
        assertEquals(original, TaskStore.unescape(escaped));
    }
}
