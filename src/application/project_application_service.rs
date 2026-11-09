use crate::domain::{
    Project, ProjectError, ProjectId, ProjectName, ProjectRepository, ProjectStatus, TaskError,
    TaskEvent, TaskId, TaskRepository, TaskStatus,
};

use super::{
    DomainEventPublishError, DomainEventPublisher, UnitOfWork, UnitOfWorkError, UnitOfWorkFactory,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectApplicationError {
    ProjectAlreadyExists { project_id: String },
    ProjectNotFound { project_id: String },
    TaskNotFound { task_id: String },
    Project(ProjectError),
    Task(TaskError),
    CommitFailed { reason: String },
    EventPublishFailed { reason: String },
}

impl From<ProjectError> for ProjectApplicationError {
    fn from(value: ProjectError) -> Self {
        Self::Project(value)
    }
}

impl From<TaskError> for ProjectApplicationError {
    fn from(value: TaskError) -> Self {
        Self::Task(value)
    }
}

impl From<UnitOfWorkError> for ProjectApplicationError {
    fn from(value: UnitOfWorkError) -> Self {
        match value {
            UnitOfWorkError::CommitFailed { reason } => Self::CommitFailed { reason },
        }
    }
}

impl From<DomainEventPublishError> for ProjectApplicationError {
    fn from(value: DomainEventPublishError) -> Self {
        match value {
            DomainEventPublishError::PublishFailed { reason } => Self::EventPublishFailed { reason },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectOutput {
    pub id: String,
    pub name: String,
    pub closed: bool,
    pub task_ids: Vec<String>,
}

impl From<&Project> for ProjectOutput {
    fn from(project: &Project) -> Self {
        Self {
            id: project.id().value().to_owned(),
            name: project.name().value().to_owned(),
            closed: project.status() == ProjectStatus::Closed,
            task_ids: project
                .task_ids()
                .iter()
                .map(|task_id| task_id.value().to_owned())
                .collect(),
        }
    }
}

/// プロジェクトに関するユースケース。
///
/// Repository を直接持たず、`UnitOfWorkFactory` からユースケースごとに Unit of Work を開始する。
/// 1つのユースケースで `Project` と `Task` の両方を更新するため、
/// 両方の Repository が同じトランザクションに属している必要があるからである。
pub struct ProjectApplicationService<F, P>
where
    F: UnitOfWorkFactory,
    P: DomainEventPublisher,
{
    unit_of_work_factory: F,
    event_publisher: P,
}

impl<F, P> ProjectApplicationService<F, P>
where
    F: UnitOfWorkFactory,
    P: DomainEventPublisher,
{
    pub fn new(unit_of_work_factory: F, event_publisher: P) -> Self {
        Self {
            unit_of_work_factory,
            event_publisher,
        }
    }

    pub fn create_project(
        &self,
        project_id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<ProjectOutput, ProjectApplicationError> {
        let project_id = ProjectId::new(project_id)?;
        let name = ProjectName::new(name)?;

        let mut uow = self.unit_of_work_factory.begin();

        if uow.projects().find_by_id(&project_id).is_some() {
            return Err(ProjectApplicationError::ProjectAlreadyExists {
                project_id: project_id.value().to_owned(),
            });
        }

        let project = Project::new(project_id, name);
        let output = ProjectOutput::from(&project);
        uow.projects().save(project);
        uow.commit()?;

        Ok(output)
    }

    /// 既存のタスクをプロジェクトに加える。更新する Aggregate は `Project` だけ。
    pub fn add_task_to_project(
        &self,
        project_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Result<ProjectOutput, ProjectApplicationError> {
        let project_id = ProjectId::new(project_id)?;
        let task_id = TaskId::new(task_id)?;

        let mut uow = self.unit_of_work_factory.begin();

        // タスクは参照するだけで変更しない(存在確認のみ)
        if uow.tasks().find_by_id(&task_id).is_none() {
            return Err(ProjectApplicationError::TaskNotFound {
                task_id: task_id.value().to_owned(),
            });
        }

        let mut project = find_project(&mut uow, &project_id)?;
        project.add_task(task_id)?;
        let output = ProjectOutput::from(&project);
        uow.projects().save(project);
        uow.commit()?;

        Ok(output)
    }

    /// プロジェクトをクローズし、所属するタスクをすべてアーカイブする。
    ///
    /// 「クローズしたプロジェクトに、アーカイブされていないタスクが一瞬でも残ってはいけない」
    /// という業務ルールを前提に、`Project` と複数の `Task` を1つの Unit of Work で更新する。
    /// これは「1トランザクション = 1 Aggregate」の原則に対する、意識した例外である。
    /// 数秒のずれが許されるなら、`ProjectClosed` イベントを発行して
    /// タスクのアーカイブを別トランザクションで行う結果整合性の方が望ましい(第7章-12)。
    pub fn close_project(
        &self,
        project_id: impl Into<String>,
    ) -> Result<ProjectOutput, ProjectApplicationError> {
        let project_id = ProjectId::new(project_id)?;

        let mut uow = self.unit_of_work_factory.begin();

        let mut project = find_project(&mut uow, &project_id)?;
        project.close()?;

        let mut events = Vec::new();
        for task_id in project.task_ids() {
            let mut task = uow.tasks().find_by_id(task_id).ok_or_else(|| {
                // ここで抜けると uow は commit されずに drop され、
                // それまでにアーカイブしたタスクも含めてすべて破棄される
                ProjectApplicationError::TaskNotFound {
                    task_id: task_id.value().to_owned(),
                }
            })?;

            // すでにアーカイブ済みのタスクは、目的の状態なので何もしない
            if task.status() == TaskStatus::Archived {
                continue;
            }

            task.archive()?;
            events.push(TaskEvent::TaskArchived {
                task_id: task.id().clone(),
            });
            uow.tasks().save(task);
        }

        let output = ProjectOutput::from(&project);
        uow.projects().save(project);

        // Project と Task の変更を、ここでまとめて確定する
        uow.commit()?;

        // 確定してから、起きた事実を発行する
        for event in &events {
            self.event_publisher.publish(event)?;
        }

        Ok(output)
    }
}

fn find_project<U: UnitOfWork>(
    uow: &mut U,
    project_id: &ProjectId,
) -> Result<Project, ProjectApplicationError> {
    uow.projects()
        .find_by_id(project_id)
        .ok_or_else(|| ProjectApplicationError::ProjectNotFound {
            project_id: project_id.value().to_owned(),
        })
}
