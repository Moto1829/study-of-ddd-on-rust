use crate::domain::TaskId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEvent {
    TaskCreated { task_id: TaskId },
    TaskCompleted { task_id: TaskId },
    TaskArchived { task_id: TaskId },
}