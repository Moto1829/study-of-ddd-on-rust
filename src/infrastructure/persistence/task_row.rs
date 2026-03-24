use crate::domain::{Task, TaskError, TaskId, TaskStatus, TaskTitle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub status: String,
}

impl From<&Task> for TaskRow {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id().value().to_owned(),
            title: task.title().value().to_owned(),
            status: task.status().as_str().to_owned(),
        }
    }
}

impl TryFrom<TaskRow> for Task {
    type Error = TaskError;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        let id = TaskId::new(row.id)?;
        let title = TaskTitle::new(row.title)?;
        let status = row.status.parse::<TaskStatus>()?;

        Ok(Task::restore(id, title, status))
    }
}