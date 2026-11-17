package com.example.todo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.Optional;
import org.junit.jupiter.api.Test;

class PriorityTest {

    @Test
    void parsesLabelsIgnoringCaseAndSurroundingSpace() {
        assertEquals(Optional.of(Priority.HIGH), Priority.parse("high"));
        assertEquals(Optional.of(Priority.MEDIUM), Priority.parse("Medium"));
        assertEquals(Optional.of(Priority.LOW), Priority.parse(" LOW "));
    }

    @Test
    void rejectsUnknownValues() {
        assertTrue(Priority.parse("urgent").isEmpty());
        assertTrue(Priority.parse("h").isEmpty());
        assertTrue(Priority.parse("").isEmpty());
        assertTrue(Priority.parse(null).isEmpty());
    }

    @Test
    void labelIsLowerCaseName() {
        assertEquals("high", Priority.HIGH.label());
        assertEquals("medium", Priority.MEDIUM.label());
        assertEquals("low", Priority.LOW.label());
    }
}
