use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::application::{FindTasksQuery, TaskStatusDto, TaskSummary, TaskSummaryReader};
use crate::domain::{Task, TaskId, TaskRepository, TaskStatus};

use super::TaskRow;

/// メモリ上の `HashMap` をDBのテーブルに見立てた Repository 実装。
///
/// 中身は `Task`(ドメインモデル)ではなく `TaskRow`(永続化モデル)で保持する。
/// 保存時は `From<&Task> for TaskRow`、取得時は `TryFrom<TaskRow> for Task` で変換し、
/// 実際のDB実装と同じ「行 ⇔ ドメインモデル」の境界をメモリ上で再現している。
#[derive(Debug, Clone, Default)]
pub struct InMemoryTaskRepository {
    rows: Arc<RwLock<HashMap<String, TaskRow>>>,
}

impl InMemoryTaskRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// トランザクション開始時点の状態を複製する(InMemoryTransactionRunner、InMemoryUnitOfWork 用)。
    pub(super) fn snapshot(&self) -> HashMap<String, TaskRow> {
        self.rows.read().unwrap().clone()
    }

    /// 渡した状態を持つ、独立した保存領域の Repository を作る(InMemoryUnitOfWork 用)。
    pub(super) fn from_snapshot(rows: HashMap<String, TaskRow>) -> Self {
        Self {
            rows: Arc::new(RwLock::new(rows)),
        }
    }

    /// 複製しておいた状態へ戻す(InMemoryTransactionRunner、InMemoryUnitOfWork 用)。
    pub(super) fn restore(&self, snapshot: HashMap<String, TaskRow>) {
        *self.rows.write().unwrap() = snapshot;
    }

    fn to_task(row: TaskRow) -> Task {
        // このテーブルへ書き込む経路は `save`(= 妥当な Task からの変換)だけなので、変換は失敗しない。
        // 外部から書き込まれる本物のDBでは不正な行がありうるため、
        // ここは `RepositoryError` などのエラーとして返すべきところ(第5章-7、第7章-4)。
        Task::try_from(row).expect("rows are written only from valid Task values")
    }
}

impl TaskRepository for InMemoryTaskRepository {
    fn save(&mut self, task: Task) {
        let row = TaskRow::from(&task);
        self.rows.write().unwrap().insert(row.id.clone(), row);
    }

    fn find_by_id(&self, task_id: &TaskId) -> Option<Task> {
        self.rows
            .read()
            .unwrap()
            .get(task_id.value())
            .cloned()
            .map(Self::to_task)
    }

    fn count_open_tasks(&self) -> usize {
        // DBなら SELECT COUNT(*) FROM tasks WHERE status = 'open' にあたる
        self.rows
            .read()
            .unwrap()
            .values()
            .filter(|row| row.status == TaskStatus::Open.as_str())
            .count()
    }
}

impl TaskSummaryReader for InMemoryTaskRepository {
    fn find_tasks(&self, query: &FindTasksQuery) -> Vec<TaskSummary> {
        let mut tasks = self
            .rows
            .read()
            .unwrap()
            .values()
            .cloned()
            .map(Self::to_task)
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
