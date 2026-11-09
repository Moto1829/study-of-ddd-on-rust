use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::domain::{Project, ProjectId, ProjectRepository};

/// メモリ上に `Project` を保持する Repository 実装。
#[derive(Debug, Clone, Default)]
pub struct InMemoryProjectRepository {
    projects: Arc<RwLock<HashMap<ProjectId, Project>>>,
}

impl InMemoryProjectRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) fn snapshot(&self) -> HashMap<ProjectId, Project> {
        self.projects.read().unwrap().clone()
    }

    pub(super) fn from_snapshot(projects: HashMap<ProjectId, Project>) -> Self {
        Self {
            projects: Arc::new(RwLock::new(projects)),
        }
    }

    pub(super) fn restore(&self, snapshot: HashMap<ProjectId, Project>) {
        *self.projects.write().unwrap() = snapshot;
    }
}

impl ProjectRepository for InMemoryProjectRepository {
    fn save(&mut self, project: Project) {
        self.projects
            .write()
            .unwrap()
            .insert(project.id().clone(), project);
    }

    fn find_by_id(&self, project_id: &ProjectId) -> Option<Project> {
        self.projects.read().unwrap().get(project_id).cloned()
    }
}
