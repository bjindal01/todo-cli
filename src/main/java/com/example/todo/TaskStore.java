package com.example.todo;

import java.io.Closeable;
import java.io.IOException;
import java.nio.channels.FileChannel;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.StandardCharsets;
import java.nio.file.AccessDeniedException;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.FileSystemException;
import java.nio.file.Files;
import java.nio.file.NoSuchFileException;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.nio.file.attribute.PosixFileAttributeView;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * Reads and writes tasks as UTF-8 text. A file starts with two header lines,
 * {@code #version<TAB>2} and {@code #next<TAB>id} (the next id to hand out), followed by one task
 * per line: {@code id<TAB>done<TAB>priority<TAB>text}. Backslashes, tabs and line breaks in the
 * text are escaped so each task stays on one line.
 *
 * <p>Older files still load. Files without {@code #version} may have 3-field task lines
 * ({@code id<TAB>done<TAB>text}, read as medium priority) and may lack {@code #next}, in which case
 * the next id follows the highest task id.
 */
public final class TaskStore {

    private static final String VERSION_PREFIX = "#version\t";
    private static final String NEXT_ID_PREFIX = "#next\t";
    private static final String VERSION = "2";
    private static final int MAX_SYMLINK_HOPS = 40;

    private final Path file;

    public TaskStore(Path file) {
        this.file = file;
    }

    /**
     * Takes an exclusive lock that other todo processes also take, so a load-change-save sequence
     * cannot overwrite another process's save. Blocks until the lock is free; close to release.
     * The lock lives in a {@code .lock} file next to the task file, which is left in place because
     * deleting it while another process waits on it would break the exclusion.
     */
    public Closeable lock() throws IOException {
        FileChannel channel;
        try {
            Path target = target();
            Path lockFile = target.resolveSibling(target.getFileName() + ".lock");
            Files.createDirectories(lockFile.getParent());
            channel = FileChannel.open(lockFile, StandardOpenOption.CREATE, StandardOpenOption.WRITE);
        } catch (IOException e) {
            throw failure("lock", e);
        }
        try {
            channel.lock();
        } catch (IOException e) {
            channel.close();
            throw failure("lock", e);
        } catch (RuntimeException e) {
            channel.close();
            throw e;
        }
        return channel;
    }

    public TodoList load() throws IOException {
        if (!Files.exists(file)) {
            return new TodoList();
        }
        List<String> lines;
        try {
            lines = Files.readAllLines(file, StandardCharsets.UTF_8);
        } catch (CharacterCodingException e) {
            throw new IOException("Could not read " + file + ": it is not valid UTF-8 text.", e);
        } catch (IOException e) {
            throw failure("read", e);
        }
        boolean versioned = false;
        long nextId = 1;
        List<Task> tasks = new ArrayList<>();
        Set<Integer> ids = new HashSet<>();
        for (int i = 0; i < lines.size(); i++) {
            String line = lines.get(i);
            int lineNumber = i + 1;
            if (line.isBlank()) {
                continue;
            }
            if (line.startsWith(VERSION_PREFIX)) {
                String version = line.substring(VERSION_PREFIX.length());
                if (!version.equals(VERSION)) {
                    throw corrupt(lineNumber, "unsupported file version " + version);
                }
                versioned = true;
                continue;
            }
            if (line.startsWith(NEXT_ID_PREFIX)) {
                nextId = parseNextId(line.substring(NEXT_ID_PREFIX.length()), lineNumber);
                continue;
            }
            Task task = parse(line, lineNumber, versioned);
            if (!ids.add(task.id())) {
                throw corrupt(lineNumber, "duplicate id " + task.id());
            }
            tasks.add(task);
        }
        return new TodoList(tasks, nextId);
    }

    public void save(TodoList list) throws IOException {
        StringBuilder out = new StringBuilder();
        out.append(VERSION_PREFIX).append(VERSION).append('\n');
        out.append(NEXT_ID_PREFIX).append(list.nextId()).append('\n');
        for (Task task : list.all()) {
            out.append(task.id()).append('\t')
               .append(task.done() ? '1' : '0').append('\t')
               .append(task.priority().label()).append('\t')
               .append(escape(task.text())).append('\n');
        }
        Path tmp = null;
        try {
            // Replace the file a symlink points to, not the symlink itself.
            Path target = target();
            Path dir = target.getParent();
            Files.createDirectories(dir);
            // Write to a sibling temp file first so a crash mid-write cannot truncate the existing list.
            tmp = Files.createTempFile(dir, target.getFileName().toString(), ".tmp");
            Files.writeString(tmp, out, StandardCharsets.UTF_8);
            copyPermissions(target, tmp);
            try {
                Files.move(tmp, target, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
            } catch (AtomicMoveNotSupportedException e) {
                Files.move(tmp, target, StandardCopyOption.REPLACE_EXISTING);
            }
        } catch (IOException e) {
            throw failure("write", e);
        } finally {
            if (tmp != null) {
                Files.deleteIfExists(tmp);
            }
        }
    }

    /** The real file behind {@link #file}, following symlinks even when the final target does not exist yet. */
    private Path target() throws IOException {
        Path path = file.toAbsolutePath();
        for (int hops = 0; Files.isSymbolicLink(path); hops++) {
            if (hops == MAX_SYMLINK_HOPS) {
                throw new FileSystemException(file.toString(), null, "too many levels of symbolic links");
            }
            path = path.resolveSibling(Files.readSymbolicLink(path));
        }
        return path;
    }

    /** New temp files are private (0600); keep the permissions the task file already had. */
    private static void copyPermissions(Path from, Path to) throws IOException {
        if (!Files.exists(from)) {
            return;
        }
        PosixFileAttributeView view = Files.getFileAttributeView(from, PosixFileAttributeView.class);
        if (view != null) {
            Files.setPosixFilePermissions(to, view.readAttributes().permissions());
        }
    }

    private Task parse(String line, int lineNumber, boolean versioned) throws IOException {
        // Task text never contains a raw tab (it is escaped), so in unversioned files the field
        // count tells 3-field lines (before priorities) from 4-field lines.
        String[] parts = line.split("\t", 4);
        if (versioned && parts.length != 4) {
            throw corrupt(lineNumber, "expected 4 tab-separated fields");
        }
        if (parts.length != 3 && parts.length != 4) {
            throw corrupt(lineNumber, "expected 3 or 4 tab-separated fields");
        }
        int id;
        try {
            id = Integer.parseInt(parts[0]);
        } catch (NumberFormatException e) {
            throw corrupt(lineNumber, "id is not a number");
        }
        if (id < 1) {
            throw corrupt(lineNumber, "id must be positive");
        }
        boolean done = switch (parts[1]) {
            case "1" -> true;
            case "0" -> false;
            default -> throw corrupt(lineNumber, "done flag must be 0 or 1");
        };
        if (parts.length == 3) {
            return new Task(id, unescape(parts[2]), done, Priority.MEDIUM);
        }
        Priority priority = Priority.parse(parts[2])
                .orElseThrow(() -> corrupt(lineNumber, "priority must be high, medium or low"));
        return new Task(id, unescape(parts[3]), done, priority);
    }

    private long parseNextId(String value, int lineNumber) throws IOException {
        long nextId;
        try {
            nextId = Long.parseLong(value);
        } catch (NumberFormatException e) {
            throw corrupt(lineNumber, "next id is not a number");
        }
        if (nextId < 1) {
            throw corrupt(lineNumber, "next id must be positive");
        }
        if (nextId > TodoList.MAX_NEXT_ID) {
            throw corrupt(lineNumber, "next id is too large");
        }
        return nextId;
    }

    private IOException corrupt(int lineNumber, String reason) {
        return new IOException("Could not read " + file + " line " + lineNumber + ": " + reason + ".");
    }

    private IOException failure(String action, IOException e) {
        return new IOException("Could not " + action + " " + file + ": " + reason(e) + ".", e);
    }

    private static String reason(IOException e) {
        if (e instanceof AccessDeniedException) {
            return "permission denied";
        }
        if (e instanceof NoSuchFileException) {
            return "no such file or folder";
        }
        if (e instanceof FileSystemException fse && fse.getReason() != null) {
            return fse.getReason();
        }
        return e.getMessage() != null ? e.getMessage() : e.getClass().getSimpleName();
    }

    static String escape(String text) {
        StringBuilder sb = new StringBuilder(text.length());
        for (char c : text.toCharArray()) {
            switch (c) {
                case '\\' -> sb.append("\\\\");
                case '\t' -> sb.append("\\t");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                default -> sb.append(c);
            }
        }
        return sb.toString();
    }

    static String unescape(String text) {
        StringBuilder sb = new StringBuilder(text.length());
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            if (c == '\\' && i + 1 < text.length()) {
                char next = text.charAt(++i);
                switch (next) {
                    case 't' -> sb.append('\t');
                    case 'n' -> sb.append('\n');
                    case 'r' -> sb.append('\r');
                    default -> sb.append(next);
                }
            } else {
                sb.append(c);
            }
        }
        return sb.toString();
    }
}
