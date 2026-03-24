use crate::domain::Task;

use super::TaskStatusDto;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindTasksQuery {
    pub status: Option<TaskStatusDto>,
    pub page: u64,
    pub per_page: u64,
}

impl FindTasksQuery {
    pub fn offset(&self) -> usize {
        let page = self.page.max(1);
        let per_page = self.per_page.max(1);

        ((page - 1) * per_page) as usize
    }

    pub fn limit(&self) -> usize {
        self.per_page.max(1) as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub status: TaskStatusDto,
}

pub trait TaskSummaryReader {
    fn find_tasks(&self, query: &FindTasksQuery) -> Vec<TaskSummary>;
}

impl From<&Task> for TaskSummary {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id().value().to_owned(),
            title: task.title().value().to_owned(),
            status: TaskStatusDto::from(task.status()),
        }
    }
}