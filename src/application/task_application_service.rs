use crate::domain::{
    OpenTaskLimitPolicy, Task, TaskCreationPolicyError, TaskError, TaskEvent, TaskId,
    TaskRepository, TaskStatus, TaskTitle, UserId,
};

use super::{DomainEventPublishError, DomainEventPublisher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskApplicationError {
    TaskAlreadyExists { task_id: String },
    TaskNotFound { task_id: String },
    Domain(TaskError),
    /// Domain Service(作成上限ポリシー)による拒否
    CreationPolicy(TaskCreationPolicyError),
    /// 状態の保存後に、イベントの発行だけが失敗した
    EventPublishFailed { reason: String },
}

impl From<TaskError> for TaskApplicationError {
    fn from(value: TaskError) -> Self {
        Self::Domain(value)
    }
}

impl From<TaskCreationPolicyError> for TaskApplicationError {
    fn from(value: TaskCreationPolicyError) -> Self {
        Self::CreationPolicy(value)
    }
}

impl From<DomainEventPublishError> for TaskApplicationError {
    fn from(value: DomainEventPublishError) -> Self {
        match value {
            DomainEventPublishError::PublishFailed { reason } => Self::EventPublishFailed { reason },
        }
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

/// タスクに関するユースケースの進行役。
///
/// - `repository`: Aggregate(`Task`)の保存と取得
/// - `event_publisher`: 状態変更の事実(`TaskEvent`)を外へ伝える
/// - `creation_policy` / `owner`: 未完了タスク数の上限を判定する Domain Service と、その対象ユーザー
///
/// このアプリは「個人向けタスク管理」という1つの Bounded Context なので、
/// 1つの Repository に入っているタスクはすべて `owner` のものとして扱う。
pub struct TaskApplicationService<R, P>
where
    R: TaskRepository,
    P: DomainEventPublisher,
{
    repository: R,
    event_publisher: P,
    creation_policy: OpenTaskLimitPolicy,
    owner: UserId,
}

impl<R, P> TaskApplicationService<R, P>
where
    R: TaskRepository,
    P: DomainEventPublisher,
{
    pub fn new(
        repository: R,
        event_publisher: P,
        creation_policy: OpenTaskLimitPolicy,
        owner: UserId,
    ) -> Self {
        Self {
            repository,
            event_publisher,
            creation_policy,
            owner,
        }
    }

    pub fn create_task(
        &mut self,
        task_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Result<TaskOutput, TaskApplicationError> {
        let task_id = TaskId::new(task_id)?;
        let title = TaskTitle::new(title)?;

        if self.repository.find_by_id(&task_id).is_some() {
            return Err(TaskApplicationError::TaskAlreadyExists {
                task_id: task_id.value().to_owned(),
            });
        }

        // 件数を数えるのは Repository、作成してよいかを判断するのは Domain Service
        let open_task_count = self.repository.count_open_tasks();
        self.creation_policy
            .ensure_can_create(&self.owner, open_task_count)?;

        let task = Task::new(task_id, title);
        let output = TaskOutput::from(&task);
        let event = TaskEvent::TaskCreated {
            task_id: task.id().clone(),
        };
        self.repository.save(task);
        // 状態を確定させてから、起きた事実を発行する
        self.event_publisher.publish(&event)?;

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
        let event = TaskEvent::TaskCompleted {
            task_id: task.id().clone(),
        };
        self.repository.save(task);
        self.event_publisher.publish(&event)?;

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
        let event = TaskEvent::TaskArchived {
            task_id: task.id().clone(),
        };
        self.repository.save(task);
        self.event_publisher.publish(&event)?;

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