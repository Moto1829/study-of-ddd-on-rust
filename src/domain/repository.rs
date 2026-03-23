use crate::domain::{Task, TaskId};

pub trait TaskRepository {
    fn save(&mut self, task: Task);
    fn find_by_id(&self, task_id: &TaskId) -> Option<Task>;
}