package com.example.todo;

public final class TaskNotFoundException extends RuntimeException {

    public TaskNotFoundException(int id) {
        super("No task with id " + id + ".");
    }
}
