use crate::domain::{Task, TaskError, TaskId, TaskRepository, TaskStatus, TaskTitle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskApplicationError {
    TaskAlreadyExists { task_id: String },
    TaskNotFound { task_id: String },
    Domain(TaskError),
}

impl From<TaskError> for TaskApplicationError {
    fn from(value: TaskError) -> Self {
        Self::Domain(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatusDto {
    Open,
    Completed,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOutput {
    pub id: String,
    pub title: String,
    pub status: TaskStatusDto,
}

pub struct TaskApplicationService<R>
where
    R: TaskRepository,
{
    repository: R,
}

impl<R> TaskApplicationService<R>
where
    R: TaskRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn create_task(
        &mut self,
        task_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Result<TaskOutput, TaskApplicationError> {
        let task_id = TaskId::new(task_id)?;

        if self.repository.find_by_id(&task_id).is_some() {
            return Err(TaskApplicationError::TaskAlreadyExists {
                task_id: task_id.value().to_owned(),
            });
        }

        let task = Task::new(task_id, TaskTitle::new(title)?);
        let output = TaskOutput::from(&task);
        self.repository.save(task);

        Ok(output)
    }

    pub fn get_task(&self, task_id: impl Into<String>) -> Result<TaskOutput, TaskApplicationError> {
        let task_id = TaskId::new(task_id)?;
        let task = self
            .repository
            .find_by_id(&task_id)
            .ok_or_else(|| TaskApplicationError::TaskNotFound {
                task_id: task_id.value().to_owned(),
            })?;

        Ok(TaskOutput::from(&task))
    }

    pub fn rename_task(
        &mut self,
        task_id: impl Into<String>,
        new_title: impl Into<String>,
    ) -> Result<TaskOutput, TaskApplicationError> {
        let task_id = TaskId::new(task_id)?;
        let mut task = self
            .repository
            .find_by_id(&task_id)
            .ok_or_else(|| TaskApplicationError::TaskNotFound {
                task_id: task_id.value().to_owned(),
            })?;

        task.rename(TaskTitle::new(new_title)?)?;
        let output = TaskOutput::from(&task);
        self.repository.save(task);

        Ok(output)
    }

    pub fn complete_task(
        &mut self,
        task_id: impl Into<String>,
    ) -> Result<TaskOutput, TaskApplicationError> {
        let task_id = TaskId::new(task_id)?;
        let mut task = self
            .repository
            .find_by_id(&task_id)
            .ok_or_else(|| TaskApplicationError::TaskNotFound {
                task_id: task_id.value().to_owned(),
            })?;

        task.complete()?;
        let output = TaskOutput::from(&task);
        self.repository.save(task);

        Ok(output)
    }

    pub fn archive_task(
        &mut self,
        task_id: impl Into<String>,
    ) -> Result<TaskOutput, TaskApplicationError> {
        let task_id = TaskId::new(task_id)?;
        let mut task = self
            .repository
            .find_by_id(&task_id)
            .ok_or_else(|| TaskApplicationError::TaskNotFound {
                task_id: task_id.value().to_owned(),
            })?;

        task.archive()?;
        let output = TaskOutput::from(&task);
        self.repository.save(task);

        Ok(output)
    }
}

impl From<&Task> for TaskOutput {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id().value().to_owned(),
            title: task.title().value().to_owned(),
            status: TaskStatusDto::from(task.status()),
        }
    }
}

impl From<TaskStatus> for TaskStatusDto {
    fn from(value: TaskStatus) -> Self {
        match value {
            TaskStatus::Open => Self::Open,
            TaskStatus::Completed => Self::Completed,
            TaskStatus::Archived => Self::Archived,
        }
    }
}