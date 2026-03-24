use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::application::{FindTasksQuery, TaskStatusDto, TaskSummary, TaskSummaryReader};
use crate::domain::{Task, TaskId, TaskRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryTaskRepository {
    tasks: Arc<RwLock<HashMap<TaskId, Task>>>,
}

impl InMemoryTaskRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TaskRepository for InMemoryTaskRepository {
    fn save(&mut self, task: Task) {
        self.tasks.write().unwrap().insert(task.id().clone(), task);
    }

    fn find_by_id(&self, task_id: &TaskId) -> Option<Task> {
        self.tasks.read().unwrap().get(task_id).cloned()
    }
}

impl TaskSummaryReader for InMemoryTaskRepository {
    fn find_tasks(&self, query: &FindTasksQuery) -> Vec<TaskSummary> {
        let mut tasks = self
            .tasks
            .read()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();

        tasks.sort_by(|left, right| left.id().value().cmp(right.id().value()));

        tasks
            .into_iter()
            .filter(|task| {
                query.status.is_none_or(|status| TaskStatusDto::from(task.status()) == status)
            })
            .skip(query.offset())
            .take(query.limit())
            .map(|task| TaskSummary::from(&task))
            .collect()
    }
}