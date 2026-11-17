package com.example.todo;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/**
 * In-memory list of tasks. Ids are never reused: the next id is tracked separately from the
 * tasks, so deleting the newest task does not hand its id to the next one added.
 */
public final class TodoList {

    /** One past the largest possible task id; reached once id {@link Integer#MAX_VALUE} is used. */
    static final long MAX_NEXT_ID = Integer.MAX_VALUE + 1L;

    private final List<Task> tasks;
    // A long so that "one past the last possible id" is representable without overflowing.
    private long nextId;

    public TodoList() {
        this(List.of());
    }

    public TodoList(List<Task> tasks) {
        this(tasks, 1);
    }

    /**
     * @param nextId the id to give the next added task; raised if it would collide with an
     *     existing task, so a hand-edited file cannot cause duplicate ids
     */
    public TodoList(List<Task> tasks, long nextId) {
        this.tasks = new ArrayList<>(tasks);
        long afterHighest = this.tasks.stream().mapToLong(Task::id).max().orElse(0) + 1;
        this.nextId = Math.max(nextId, afterHighest);
    }

    public Task add(String text) {
        return add(text, Priority.MEDIUM);
    }

    /** @throws IllegalStateException if every task id up to {@link Integer#MAX_VALUE} is used */
    public Task add(String text, Priority priority) {
        String trimmed = text == null ? "" : text.strip();
        if (trimmed.isEmpty()) {
            throw new IllegalArgumentException("Task text cannot be empty.");
        }
        if (nextId >= MAX_NEXT_ID) {
            throw new IllegalStateException("No more task ids are available.");
        }
        Task task = new Task((int) nextId, trimmed, false, priority);
        nextId++;
        tasks.add(task);
        return task;
    }

    public List<Task> all() {
        return List.copyOf(tasks);
    }

    /** All tasks from high to low priority; tasks with the same priority stay in id order. */
    public List<Task> byPriority() {
        return tasks.stream()
                .sorted(Comparator.comparing(Task::priority).thenComparingInt(Task::id))
                .toList();
    }

    public long nextId() {
        return nextId;
    }

    /**
     * Marks the task as done.
     *
     * @return the task as it was before this call, so callers can tell whether it was already done
     * @throws TaskNotFoundException if no task has this id
     */
    public Task complete(int id) {
        for (int i = 0; i < tasks.size(); i++) {
            Task task = tasks.get(i);
            if (task.id() == id) {
                tasks.set(i, task.complete());
                return task;
            }
        }
        throw new TaskNotFoundException(id);
    }

    /**
     * Removes the task.
     *
     * @return the removed task
     * @throws TaskNotFoundException if no task has this id
     */
    public Task delete(int id) {
        for (int i = 0; i < tasks.size(); i++) {
            if (tasks.get(i).id() == id) {
                return tasks.remove(i);
            }
        }
        throw new TaskNotFoundException(id);
    }
}
