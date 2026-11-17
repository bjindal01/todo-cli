package com.example.todo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Test;

class TodoListTest {

    @Test
    void addAssignsSequentialIdsAndTrimsText() {
        TodoList list = new TodoList();

        Task first = list.add("  Buy milk ");
        Task second = list.add("Walk dog");

        assertEquals(new Task(1, "Buy milk", false), first);
        assertEquals(new Task(2, "Walk dog", false), second);
        assertEquals(List.of(first, second), list.all());
    }

    @Test
    void addRejectsBlankText() {
        TodoList list = new TodoList();

        assertThrows(IllegalArgumentException.class, () -> list.add("   "));
        assertThrows(IllegalArgumentException.class, () -> list.add(null));
        assertTrue(list.all().isEmpty());
    }

    @Test
    void nextIdFollowsHighestExistingIdSoIdsAreNotReused() {
        TodoList list = new TodoList(List.of(new Task(3, "a", false), new Task(7, "b", true)));

        assertEquals(8, list.add("c").id());
    }

    @Test
    void completeMarksOnlyThatTaskDoneAndReturnsPreviousState() {
        TodoList list = new TodoList();
        list.add("one");
        list.add("two");

        Task before = list.complete(2);

        assertFalse(before.done());
        assertFalse(list.all().get(0).done());
        assertTrue(list.all().get(1).done());
    }

    @Test
    void completingTwiceReportsItWasAlreadyDone() {
        TodoList list = new TodoList();
        list.add("one");
        list.complete(1);

        Task before = list.complete(1);

        assertTrue(before.done());
        assertTrue(list.all().get(0).done());
    }

    @Test
    void completeUnknownIdThrows() {
        TodoList list = new TodoList();
        list.add("one");

        TaskNotFoundException e = assertThrows(TaskNotFoundException.class, () -> list.complete(5));
        assertEquals("No task with id 5.", e.getMessage());
    }

    @Test
    void deleteRemovesOnlyThatTaskAndReturnsIt() {
        TodoList list = new TodoList();
        list.add("one");
        list.add("two");
        list.add("three");

        Task removed = list.delete(2);

        assertEquals(new Task(2, "two", false), removed);
        assertEquals(List.of(new Task(1, "one", false), new Task(3, "three", false)), list.all());
    }

    @Test
    void deleteUnknownIdThrowsAndLeavesListUnchanged() {
        TodoList list = new TodoList();
        list.add("one");

        TaskNotFoundException e = assertThrows(TaskNotFoundException.class, () -> list.delete(4));
        assertEquals("No task with id 4.", e.getMessage());
        assertEquals(1, list.all().size());
    }

    @Test
    void deletingNewestTaskDoesNotReuseItsId() {
        TodoList list = new TodoList();
        list.add("one");
        list.add("two");

        list.delete(2);

        assertEquals(3, list.add("three").id());
    }

    @Test
    void deletingEveryTaskStillDoesNotReuseIds() {
        TodoList list = new TodoList();
        list.add("one");
        list.delete(1);

        assertEquals(2, list.add("two").id());
    }

    @Test
    void nextIdIsRaisedAboveExistingTasks() {
        TodoList list = new TodoList(List.of(new Task(5, "a", false)), 2);

        assertEquals(6, list.nextId());
        assertEquals(6, list.add("b").id());
    }

    @Test
    void addDefaultsToMediumPriority() {
        TodoList list = new TodoList();

        assertEquals(Priority.MEDIUM, list.add("x").priority());
    }

    @Test
    void addKeepsGivenPriority() {
        TodoList list = new TodoList();

        assertEquals(new Task(1, "x", false, Priority.HIGH), list.add("x", Priority.HIGH));
        assertEquals(new Task(2, "y", false, Priority.LOW), list.add("y", Priority.LOW));
    }

    @Test
    void byPrioritySortsHighToLowAndKeepsIdOrderWithinAPriority() {
        TodoList list = new TodoList();
        list.add("low one", Priority.LOW);
        list.add("medium one", Priority.MEDIUM);
        list.add("high one", Priority.HIGH);
        list.add("low two", Priority.LOW);
        list.add("high two", Priority.HIGH);

        List<Integer> ids = list.byPriority().stream().map(Task::id).toList();

        assertEquals(List.of(3, 5, 2, 1, 4), ids);
        assertEquals(List.of(1, 2, 3, 4, 5), list.all().stream().map(Task::id).toList());
    }

    @Test
    void completeKeepsPriority() {
        TodoList list = new TodoList();
        list.add("x", Priority.HIGH);

        list.complete(1);

        assertEquals(new Task(1, "x", true, Priority.HIGH), list.all().get(0));
    }

    @Test
    void addFailsCleanlyOnceEveryIdIsUsed() {
        TodoList list = new TodoList(List.of(new Task(Integer.MAX_VALUE - 1, "a", false)));

        assertEquals(Integer.MAX_VALUE, list.add("b").id());
        IllegalStateException e = assertThrows(IllegalStateException.class, () -> list.add("c"));
        assertEquals("No more task ids are available.", e.getMessage());
        assertEquals(2, list.all().size());
    }

    @Test
    void largestExistingIdDoesNotWrapNextIdAround() {
        TodoList list = new TodoList(List.of(new Task(Integer.MAX_VALUE, "a", false)), 1);

        assertEquals(TodoList.MAX_NEXT_ID, list.nextId());
        assertThrows(IllegalStateException.class, () -> list.add("b"));
    }

    @Test
    void allReturnsAnUnmodifiableSnapshot() {
        TodoList list = new TodoList();
        list.add("one");

        List<Task> snapshot = list.all();
        list.add("two");

        assertEquals(1, snapshot.size());
        assertThrows(UnsupportedOperationException.class, () -> snapshot.add(new Task(9, "x", false)));
    }
}
