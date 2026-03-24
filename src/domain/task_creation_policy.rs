use std::fmt;

const DEFAULT_MAX_OPEN_TASKS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(String);

impl UserId {
    pub fn new(value: impl Into<String>) -> Result<Self, TaskCreationPolicyError> {
        let value = value.into();
        let normalized = value.trim().to_owned();

        if normalized.is_empty() {
            return Err(TaskCreationPolicyError::EmptyUserId);
        }

        Ok(Self(normalized))
    }

    pub fn value(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskCreationPolicyError {
    EmptyUserId,
    OpenTaskLimitExceeded {
        user_id: String,
        max: usize,
        actual: usize,
    },
}

impl fmt::Display for TaskCreationPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyUserId => write!(formatter, "user id must not be empty"),
            Self::OpenTaskLimitExceeded {
                user_id,
                max,
                actual,
            } => write!(
                formatter,
                "user {user_id} exceeded open task limit: max={max}, actual={actual}"
            ),
        }
    }
}

impl std::error::Error for TaskCreationPolicyError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenTaskLimitPolicy {
    max_open_tasks: usize,
}

impl Default for OpenTaskLimitPolicy {
    fn default() -> Self {
        Self {
            max_open_tasks: DEFAULT_MAX_OPEN_TASKS,
        }
    }
}

impl OpenTaskLimitPolicy {
    pub fn new(max_open_tasks: usize) -> Self {
        Self { max_open_tasks }
    }

    pub fn ensure_can_create(
        &self,
        user_id: &UserId,
        open_task_count: usize,
    ) -> Result<(), TaskCreationPolicyError> {
        if open_task_count >= self.max_open_tasks {
            return Err(TaskCreationPolicyError::OpenTaskLimitExceeded {
                user_id: user_id.value().to_owned(),
                max: self.max_open_tasks,
                actual: open_task_count,
            });
        }

        Ok(())
    }
}