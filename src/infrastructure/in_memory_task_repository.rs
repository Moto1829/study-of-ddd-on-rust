use std::collections::HashMap;

use crate::domain::{Task, TaskId, TaskRepository};

#[derive(Debug, Default)]
pub struct InMemoryTaskRepository {
    tasks: HashMap<TaskId, Task>,
}

impl InMemoryTaskRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TaskRepository for InMemoryTaskRepository {
    fn save(&mut self, task: Task) {
        self.tasks.insert(task.id().clone(), task);
    }

    fn find_by_id(&self, task_id: &TaskId) -> Option<Task> {
        self.tasks.get(task_id).cloned()
    }
}