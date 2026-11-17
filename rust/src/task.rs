use crate::priority::Priority;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    id: i32,
    text: String,
    done: bool,
    priority: Priority,
}

impl Task {
    /// # Panics
    ///
    /// If `id` is not positive.
    pub fn new(id: i32, text: impl Into<String>, done: bool, priority: Priority) -> Task {
        assert!(id >= 1, "Task id must be positive: {id}");
        Task {
            id,
            text: text.into(),
            done,
            priority,
        }
    }

    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn done(&self) -> bool {
        self.done
    }

    pub fn priority(&self) -> Priority {
        self.priority
    }

    /// This task marked as done.
    pub fn completed(&self) -> Task {
        Task {
            done: true,
            ..self.clone()
        }
    }
}
