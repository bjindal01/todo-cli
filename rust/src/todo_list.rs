use crate::error::{AddError, TaskNotFound};
use crate::priority::Priority;
use crate::task::Task;

/// One past the largest possible task id; reached once id `i32::MAX` is used.
pub const MAX_NEXT_ID: i64 = i32::MAX as i64 + 1;

/// In-memory list of tasks. Ids are never reused: the next id is tracked separately from the
/// tasks, so deleting the newest task does not hand its id to the next one added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoList {
    tasks: Vec<Task>,
    // An i64 so that "one past the last possible id" is representable.
    next_id: i64,
}

impl Default for TodoList {
    fn default() -> Self {
        TodoList::new()
    }
}

impl TodoList {
    pub fn new() -> TodoList {
        TodoList::with_tasks(Vec::new())
    }

    pub fn with_tasks(tasks: Vec<Task>) -> TodoList {
        TodoList::with_next_id(tasks, 1)
    }

    /// `next_id` is the id to give the next added task. It is raised if it would collide with an
    /// existing task, so a hand-edited file cannot cause duplicate ids.
    pub fn with_next_id(tasks: Vec<Task>, next_id: i64) -> TodoList {
        let after_highest = tasks.iter().map(|t| i64::from(t.id())).max().unwrap_or(0) + 1;
        TodoList {
            tasks,
            next_id: next_id.max(after_highest),
        }
    }

    /// Adds a task with the given text, trimmed. Fails if the text is blank or every task id up
    /// to `i32::MAX` has been used.
    pub fn add(&mut self, text: &str, priority: Priority) -> Result<Task, AddError> {
        let trimmed = crate::compat::strip(text);
        if trimmed.is_empty() {
            return Err(AddError::EmptyText);
        }
        let id = i32::try_from(self.next_id).map_err(|_| AddError::NoMoreIds)?;
        let task = Task::new(id, trimmed, false, priority);
        self.next_id += 1;
        self.tasks.push(task.clone());
        Ok(task)
    }

    /// All tasks in the order they were loaded and added.
    pub fn all(&self) -> &[Task] {
        &self.tasks
    }

    /// All tasks from high to low priority; tasks with the same priority stay in id order.
    pub fn by_priority(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self.tasks.iter().collect();
        tasks.sort_by_key(|task| (task.priority(), task.id()));
        tasks
    }

    pub fn next_id(&self) -> i64 {
        self.next_id
    }

    /// Marks the task as done and returns it as it was before, so callers can tell whether it
    /// was already done.
    pub fn complete(&mut self, id: i32) -> Result<Task, TaskNotFound> {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id() == id)
            .ok_or(TaskNotFound(id))?;
        let before = task.clone();
        *task = before.completed();
        Ok(before)
    }

    /// Removes the task and returns it.
    pub fn delete(&mut self, id: i32) -> Result<Task, TaskNotFound> {
        let index = self
            .tasks
            .iter()
            .position(|task| task.id() == id)
            .ok_or(TaskNotFound(id))?;
        Ok(self.tasks.remove(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn medium(id: i32, text: &str, done: bool) -> Task {
        Task::new(id, text, done, Priority::Medium)
    }

    #[test]
    fn add_assigns_sequential_ids_and_trims_text() {
        let mut list = TodoList::new();

        let first = list.add("  Buy milk ", Priority::Medium).unwrap();
        let second = list.add("Walk dog", Priority::Medium).unwrap();

        assert_eq!(first, medium(1, "Buy milk", false));
        assert_eq!(second, medium(2, "Walk dog", false));
        assert_eq!(list.all(), [first, second]);
    }

    #[test]
    fn add_rejects_blank_text() {
        let mut list = TodoList::new();

        assert_eq!(list.add("   ", Priority::Medium), Err(AddError::EmptyText));
        assert_eq!(list.add("", Priority::Medium), Err(AddError::EmptyText));
        assert!(list.all().is_empty());
    }

    #[test]
    fn next_id_follows_highest_existing_id_so_ids_are_not_reused() {
        let mut list = TodoList::with_tasks(vec![medium(3, "a", false), medium(7, "b", true)]);

        assert_eq!(list.add("c", Priority::Medium).unwrap().id(), 8);
    }

    #[test]
    fn complete_marks_only_that_task_done_and_returns_previous_state() {
        let mut list = TodoList::new();
        list.add("one", Priority::Medium).unwrap();
        list.add("two", Priority::Medium).unwrap();

        let before = list.complete(2).unwrap();

        assert!(!before.done());
        assert!(!list.all()[0].done());
        assert!(list.all()[1].done());
    }

    #[test]
    fn completing_twice_reports_it_was_already_done() {
        let mut list = TodoList::new();
        list.add("one", Priority::Medium).unwrap();
        list.complete(1).unwrap();

        let before = list.complete(1).unwrap();

        assert!(before.done());
        assert!(list.all()[0].done());
    }

    #[test]
    fn complete_unknown_id_fails() {
        let mut list = TodoList::new();
        list.add("one", Priority::Medium).unwrap();

        let e = list.complete(5).unwrap_err();
        assert_eq!(e.to_string(), "No task with id 5.");
    }

    #[test]
    fn delete_removes_only_that_task_and_returns_it() {
        let mut list = TodoList::new();
        list.add("one", Priority::Medium).unwrap();
        list.add("two", Priority::Medium).unwrap();
        list.add("three", Priority::Medium).unwrap();

        let removed = list.delete(2).unwrap();

        assert_eq!(removed, medium(2, "two", false));
        assert_eq!(
            list.all(),
            [medium(1, "one", false), medium(3, "three", false)]
        );
    }

    #[test]
    fn delete_unknown_id_fails_and_leaves_list_unchanged() {
        let mut list = TodoList::new();
        list.add("one", Priority::Medium).unwrap();

        let e = list.delete(4).unwrap_err();
        assert_eq!(e.to_string(), "No task with id 4.");
        assert_eq!(list.all().len(), 1);
    }

    #[test]
    fn deleting_newest_task_does_not_reuse_its_id() {
        let mut list = TodoList::new();
        list.add("one", Priority::Medium).unwrap();
        list.add("two", Priority::Medium).unwrap();

        list.delete(2).unwrap();

        assert_eq!(list.add("three", Priority::Medium).unwrap().id(), 3);
    }

    #[test]
    fn deleting_every_task_still_does_not_reuse_ids() {
        let mut list = TodoList::new();
        list.add("one", Priority::Medium).unwrap();
        list.delete(1).unwrap();

        assert_eq!(list.add("two", Priority::Medium).unwrap().id(), 2);
    }

    #[test]
    fn next_id_is_raised_above_existing_tasks() {
        let mut list = TodoList::with_next_id(vec![medium(5, "a", false)], 2);

        assert_eq!(list.next_id(), 6);
        assert_eq!(list.add("b", Priority::Medium).unwrap().id(), 6);
    }

    #[test]
    fn default_priority_is_medium() {
        let mut list = TodoList::new();

        assert_eq!(
            list.add("x", Priority::default()).unwrap().priority(),
            Priority::Medium
        );
    }

    #[test]
    fn add_keeps_given_priority() {
        let mut list = TodoList::new();

        assert_eq!(
            list.add("x", Priority::High).unwrap(),
            Task::new(1, "x", false, Priority::High)
        );
        assert_eq!(
            list.add("y", Priority::Low).unwrap(),
            Task::new(2, "y", false, Priority::Low)
        );
    }

    #[test]
    fn by_priority_sorts_high_to_low_and_keeps_id_order_within_a_priority() {
        let mut list = TodoList::new();
        list.add("low one", Priority::Low).unwrap();
        list.add("medium one", Priority::Medium).unwrap();
        list.add("high one", Priority::High).unwrap();
        list.add("low two", Priority::Low).unwrap();
        list.add("high two", Priority::High).unwrap();

        let ids: Vec<i32> = list.by_priority().iter().map(|t| t.id()).collect();

        assert_eq!(ids, [3, 5, 2, 1, 4]);
        let all: Vec<i32> = list.all().iter().map(Task::id).collect();
        assert_eq!(all, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn complete_keeps_priority() {
        let mut list = TodoList::new();
        list.add("x", Priority::High).unwrap();

        list.complete(1).unwrap();

        assert_eq!(list.all()[0], Task::new(1, "x", true, Priority::High));
    }

    #[test]
    fn add_fails_cleanly_once_every_id_is_used() {
        let mut list = TodoList::with_tasks(vec![medium(i32::MAX - 1, "a", false)]);

        assert_eq!(list.add("b", Priority::Medium).unwrap().id(), i32::MAX);
        let e = list.add("c", Priority::Medium).unwrap_err();
        assert_eq!(e.to_string(), "No more task ids are available.");
        assert_eq!(list.all().len(), 2);
    }

    #[test]
    fn largest_existing_id_does_not_wrap_next_id_around() {
        let mut list = TodoList::with_next_id(vec![medium(i32::MAX, "a", false)], 1);

        assert_eq!(list.next_id(), MAX_NEXT_ID);
        assert_eq!(list.add("b", Priority::Medium), Err(AddError::NoMoreIds));
    }

    #[test]
    fn snapshot_of_all_is_not_affected_by_later_adds() {
        let mut list = TodoList::new();
        list.add("one", Priority::Medium).unwrap();

        let snapshot = list.all().to_vec();
        list.add("two", Priority::Medium).unwrap();

        assert_eq!(snapshot.len(), 1);
    }
}
