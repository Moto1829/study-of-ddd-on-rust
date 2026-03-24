use std::fmt;
use std::str::FromStr;

const MAX_TASK_TITLE_LENGTH: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    EmptyTaskId,
    EmptyTaskTitle,
    TaskTitleTooLong { max: usize, actual: usize },
    TaskAlreadyCompleted,
    TaskAlreadyArchived,
    TaskNotOpen { current: TaskStatus },
    InvalidTaskStatus { value: String },
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
            Self::TaskAlreadyArchived => write!(formatter, "task is already archived"),
            Self::TaskNotOpen { current } => {
                write!(formatter, "task must be open, current status is {current}")
            }
            Self::InvalidTaskStatus { value } => write!(formatter, "invalid task status: {value}"),
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
    Archived,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
            Self::Archived => "archived",
        }
    }
}

impl FromStr for TaskStatus {
    type Err = TaskError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open" => Ok(Self::Open),
            "completed" => Ok(Self::Completed),
            "archived" => Ok(Self::Archived),
            _ => Err(TaskError::InvalidTaskStatus {
                value: value.to_owned(),
            }),
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.as_str())
    }
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

    pub fn restore(id: TaskId, title: TaskTitle, status: TaskStatus) -> Self {
        Self { id, title, status }
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
        if self.status != TaskStatus::Open {
            return Err(TaskError::TaskNotOpen {
                current: self.status,
            });
        }

        self.title = new_title;
        Ok(())
    }

    pub fn complete(&mut self) -> Result<(), TaskError> {
        match self.status {
            TaskStatus::Open => {
                self.status = TaskStatus::Completed;
                Ok(())
            }
            TaskStatus::Completed => Err(TaskError::TaskAlreadyCompleted),
            TaskStatus::Archived => Err(TaskError::TaskNotOpen {
                current: self.status,
            }),
        }
    }

    pub fn archive(&mut self) -> Result<(), TaskError> {
        if self.status == TaskStatus::Archived {
            return Err(TaskError::TaskAlreadyArchived);
        }

        self.status = TaskStatus::Archived;
        Ok(())
    }
}