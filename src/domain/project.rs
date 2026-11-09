use std::fmt;

use crate::domain::TaskId;

const MAX_PROJECT_NAME_LENGTH: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    EmptyProjectId,
    EmptyProjectName,
    ProjectNameTooLong { max: usize, actual: usize },
    ProjectAlreadyClosed,
    TaskAlreadyInProject { task_id: String },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyProjectId => write!(formatter, "project id must not be empty"),
            Self::EmptyProjectName => write!(formatter, "project name must not be empty"),
            Self::ProjectNameTooLong { max, actual } => {
                write!(formatter, "project name is too long: max={max}, actual={actual}")
            }
            Self::ProjectAlreadyClosed => write!(formatter, "project is already closed"),
            Self::TaskAlreadyInProject { task_id } => {
                write!(formatter, "task {task_id} is already in the project")
            }
        }
    }
}

impl std::error::Error for ProjectError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProjectError> {
        let normalized = value.into().trim().to_owned();

        if normalized.is_empty() {
            return Err(ProjectError::EmptyProjectId);
        }

        Ok(Self(normalized))
    }

    pub fn value(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectName(String);

impl ProjectName {
    pub fn new(value: impl Into<String>) -> Result<Self, ProjectError> {
        let normalized = value.into().trim().to_owned();
        let actual = normalized.chars().count();

        if normalized.is_empty() {
            return Err(ProjectError::EmptyProjectName);
        }

        if actual > MAX_PROJECT_NAME_LENGTH {
            return Err(ProjectError::ProjectNameTooLong {
                max: MAX_PROJECT_NAME_LENGTH,
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
pub enum ProjectStatus {
    Active,
    Closed,
}

/// タスクをまとめるプロジェクト。`Task` とは別の Aggregate。
///
/// 所属するタスクは `Task` そのものではなく `TaskId` で参照する(第7章-12)。
/// そのため、プロジェクトを読み込んでもタスクは読み込まれず、
/// タスクの名前変更などでプロジェクトを保存し直す必要もない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    id: ProjectId,
    name: ProjectName,
    status: ProjectStatus,
    task_ids: Vec<TaskId>,
}

impl Project {
    pub fn new(id: ProjectId, name: ProjectName) -> Self {
        Self {
            id,
            name,
            status: ProjectStatus::Active,
            task_ids: Vec::new(),
        }
    }

    pub fn restore(
        id: ProjectId,
        name: ProjectName,
        status: ProjectStatus,
        task_ids: Vec<TaskId>,
    ) -> Self {
        Self {
            id,
            name,
            status,
            task_ids,
        }
    }

    pub fn id(&self) -> &ProjectId {
        &self.id
    }

    pub fn name(&self) -> &ProjectName {
        &self.name
    }

    pub fn status(&self) -> ProjectStatus {
        self.status
    }

    pub fn task_ids(&self) -> &[TaskId] {
        &self.task_ids
    }

    /// タスクをプロジェクトに加える。クローズ済みのプロジェクトには加えられない。
    pub fn add_task(&mut self, task_id: TaskId) -> Result<(), ProjectError> {
        if self.status == ProjectStatus::Closed {
            return Err(ProjectError::ProjectAlreadyClosed);
        }

        if self.task_ids.contains(&task_id) {
            return Err(ProjectError::TaskAlreadyInProject {
                task_id: task_id.value().to_owned(),
            });
        }

        self.task_ids.push(task_id);
        Ok(())
    }

    /// プロジェクトをクローズする。
    ///
    /// 「クローズしたら所属タスクをすべてアーカイブする」というルールは
    /// 別の Aggregate(`Task`)の状態変更を伴うため、ここでは扱わない。
    /// どのトランザクションで両者を更新するかは Application Service が決める。
    pub fn close(&mut self) -> Result<(), ProjectError> {
        if self.status == ProjectStatus::Closed {
            return Err(ProjectError::ProjectAlreadyClosed);
        }

        self.status = ProjectStatus::Closed;
        Ok(())
    }
}
