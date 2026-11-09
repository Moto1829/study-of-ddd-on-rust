use crate::domain::{Project, ProjectId, Task, TaskId};

pub trait TaskRepository {
    fn save(&mut self, task: Task);
    fn find_by_id(&self, task_id: &TaskId) -> Option<Task>;
    /// Open 状態のタスク数。作成上限ポリシーの判定材料として使う。
    fn count_open_tasks(&self) -> usize;
}

pub trait ProjectRepository {
    fn save(&mut self, project: Project);
    fn find_by_id(&self, project_id: &ProjectId) -> Option<Project>;
}
