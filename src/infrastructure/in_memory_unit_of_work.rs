use crate::application::{UnitOfWork, UnitOfWorkError, UnitOfWorkFactory};

use super::{InMemoryProjectRepository, InMemoryTaskRepository};

/// 学習用の Unit of Work 実装。
///
/// 開始時に、共有の保存領域(タスクとプロジェクト)を複製した「作業用の Repository」を作る。
/// ユースケースはこの作業用の Repository だけを読み書きするので、
/// 変更は `commit` するまで他の利用者から見えない。
/// `commit` で作業用の内容を共有の保存領域へ書き戻し、
/// `commit` せずに drop したら作業用の内容を捨てる(= ロールバック)。
///
/// DBのトランザクションの「確定するまで見えない」「確定しなければ残らない」をメモリ上で再現している。
/// ただし同時に別の Unit of Work が同じデータを変更した場合の競合検出は行わず、後から commit した方が勝つ。
/// 本番のDB実装では、`begin` で `BEGIN` し、各 Repository が同じ接続(トランザクション)を使い、
/// `commit` で `COMMIT` を発行する。
pub struct InMemoryUnitOfWork {
    shared_tasks: InMemoryTaskRepository,
    shared_projects: InMemoryProjectRepository,
    tasks: InMemoryTaskRepository,
    projects: InMemoryProjectRepository,
}

impl UnitOfWork for InMemoryUnitOfWork {
    type Tasks = InMemoryTaskRepository;
    type Projects = InMemoryProjectRepository;

    fn tasks(&mut self) -> &mut Self::Tasks {
        &mut self.tasks
    }

    fn projects(&mut self) -> &mut Self::Projects {
        &mut self.projects
    }

    fn commit(self) -> Result<(), UnitOfWorkError> {
        self.shared_tasks.restore(self.tasks.snapshot());
        self.shared_projects.restore(self.projects.snapshot());
        Ok(())
    }
}

/// 共有の保存領域を持ち、ユースケースごとに `InMemoryUnitOfWork` を開始する。
#[derive(Debug, Clone)]
pub struct InMemoryUnitOfWorkFactory {
    tasks: InMemoryTaskRepository,
    projects: InMemoryProjectRepository,
}

impl InMemoryUnitOfWorkFactory {
    /// 渡した Repository と同じ保存領域を対象にする。
    pub fn new(tasks: &InMemoryTaskRepository, projects: &InMemoryProjectRepository) -> Self {
        Self {
            tasks: tasks.clone(),
            projects: projects.clone(),
        }
    }
}

impl UnitOfWorkFactory for InMemoryUnitOfWorkFactory {
    type UnitOfWork = InMemoryUnitOfWork;

    fn begin(&self) -> Self::UnitOfWork {
        InMemoryUnitOfWork {
            shared_tasks: self.tasks.clone(),
            shared_projects: self.projects.clone(),
            tasks: InMemoryTaskRepository::from_snapshot(self.tasks.snapshot()),
            projects: InMemoryProjectRepository::from_snapshot(self.projects.snapshot()),
        }
    }
}
