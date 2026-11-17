use std::fmt;

use crate::compat;

/// Declared from most to least urgent; sorting by priority relies on this order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    High,
    #[default]
    Medium,
    Low,
}

impl Priority {
    pub const ALL: [Priority; 3] = [Priority::High, Priority::Medium, Priority::Low];

    pub fn label(self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }

    /// Parses `high`, `medium` or `low`, ignoring case and surrounding whitespace.
    pub fn parse(value: &str) -> Option<Priority> {
        let normalized = compat::strip(value).to_lowercase();
        Priority::ALL
            .into_iter()
            .find(|priority| priority.label() == normalized)
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_labels_ignoring_case_and_surrounding_space() {
        assert_eq!(Priority::parse("high"), Some(Priority::High));
        assert_eq!(Priority::parse("Medium"), Some(Priority::Medium));
        assert_eq!(Priority::parse(" LOW "), Some(Priority::Low));
    }

    #[test]
    fn rejects_unknown_values() {
        assert_eq!(Priority::parse("urgent"), None);
        assert_eq!(Priority::parse("h"), None);
        assert_eq!(Priority::parse(""), None);
    }

    #[test]
    fn label_is_lower_case_name() {
        assert_eq!(Priority::High.label(), "high");
        assert_eq!(Priority::Medium.label(), "medium");
        assert_eq!(Priority::Low.label(), "low");
    }

    #[test]
    fn sorts_from_most_to_least_urgent() {
        let mut priorities = vec![Priority::Low, Priority::High, Priority::Medium];
        priorities.sort();
        assert_eq!(priorities, Priority::ALL);
    }
}
