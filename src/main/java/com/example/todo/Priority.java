package com.example.todo;

import java.util.Locale;
import java.util.Optional;

/** Declared from most to least urgent; sorting by priority relies on this order. */
public enum Priority {
    HIGH,
    MEDIUM,
    LOW;

    public String label() {
        return name().toLowerCase(Locale.ROOT);
    }

    /** Parses {@code high}, {@code medium} or {@code low}, ignoring case. */
    public static Optional<Priority> parse(String value) {
        if (value == null) {
            return Optional.empty();
        }
        for (Priority priority : values()) {
            if (priority.label().equals(value.strip().toLowerCase(Locale.ROOT))) {
                return Optional.of(priority);
            }
        }
        return Optional.empty();
    }
}
