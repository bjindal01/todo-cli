package com.example.todo;

import java.util.Objects;

public record Task(int id, String text, boolean done, Priority priority) {

    public Task {
        if (id < 1) {
            throw new IllegalArgumentException("Task id must be positive: " + id);
        }
        Objects.requireNonNull(text, "text");
        Objects.requireNonNull(priority, "priority");
    }

    public Task(int id, String text, boolean done) {
        this(id, text, done, Priority.MEDIUM);
    }

    public Task complete() {
        return new Task(id, text, true, priority);
    }
}
