use std::fmt;

const MAX_TASK_TITLE_LENGTH: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    EmptyTaskId,
    EmptyTaskTitle,
    TaskTitleTooLong { max: usize, actual: usize },
    TaskAlreadyCompleted,
}

impl fmt::Display for TaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTaskId => write!(formatter, "task id must not be empty"),
            Self::EmptyTaskTitle => write!(formatter, "task title must not be empty"),
            Self::TaskTitleTooLong { max, actual } => {
                write!(formatter, "task title is too long: max={max}, actual={actual}")
            }
            Self::TaskAlreadyCompleted => write!(formatter, "task is already completed"),
        }
    }
}

impl std::error::Error for TaskError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(String);

impl TaskId {
    pub fn new(value: impl Into<String>) -> Result<Self, TaskError> {
        let value = value.into();
        let normalized = value.trim().to_owned();

        if normalized.is_empty() {
            return Err(TaskError::EmptyTaskId);
        }

        Ok(Self(normalized))
    }

    pub fn value(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTitle(String);

impl TaskTitle {
    pub fn new(value: impl Into<String>) -> Result<Self, TaskError> {
        let value = value.into();
        let normalized = value.trim().to_owned();
        let actual = normalized.chars().count();

        if normalized.is_empty() {
            return Err(TaskError::EmptyTaskTitle);
        }

        if actual > MAX_TASK_TITLE_LENGTH {
            return Err(TaskError::TaskTitleTooLong {
                max: MAX_TASK_TITLE_LENGTH,
                actual,
            });
        }

        Ok(Self(normalized))
    }

    pub fn value(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Open,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    id: TaskId,
    title: TaskTitle,
    status: TaskStatus,
}

impl Task {
    pub fn new(id: TaskId, title: TaskTitle) -> Self {
        Self {
            id,
            title,
            status: TaskStatus::Open,
        }
    }

    pub fn id(&self) -> &TaskId {
        &self.id
    }

    pub fn title(&self) -> &TaskTitle {
        &self.title
    }

    pub fn status(&self) -> TaskStatus {
        self.status
    }

    pub fn rename(&mut self, new_title: TaskTitle) -> Result<(), TaskError> {
        if self.status == TaskStatus::Completed {
            return Err(TaskError::TaskAlreadyCompleted);
        }

        self.title = new_title;
        Ok(())
    }

    pub fn complete(&mut self) -> Result<(), TaskError> {
        if self.status == TaskStatus::Completed {
            return Err(TaskError::TaskAlreadyCompleted);
        }

        self.status = TaskStatus::Completed;
        Ok(())
    }
}