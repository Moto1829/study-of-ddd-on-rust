use crate::domain::{ProjectRepository, TaskRepository};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitOfWorkError {
    CommitFailed { reason: String },
}

/// 1つのトランザクションの中で使う Repository の組。
///
/// `tasks()` と `projects()` が返す Repository は、どちらも同じトランザクションに属する。
/// そこで行った変更は `commit` するまで確定せず、`commit` せずに drop すると破棄される(ロールバック)。
///
/// `TransactionRunner` が「どこからどこまでを包むか」という境界だけを表すのに対して、
/// Unit of Work は「その中で、どの Repository への変更をまとめて確定するか」まで表す。
/// 複数の Aggregate を1つのトランザクションで更新する必要があるときに使う。
pub trait UnitOfWork {
    type Tasks: TaskRepository;
    type Projects: ProjectRepository;

    fn tasks(&mut self) -> &mut Self::Tasks;
    fn projects(&mut self) -> &mut Self::Projects;

    /// 変更をまとめて確定する。`self` を消費するので、確定後に使い続けることはできない。
    fn commit(self) -> Result<(), UnitOfWorkError>;
}

/// ユースケースごとに新しい Unit of Work(= トランザクション)を開始する。
pub trait UnitOfWorkFactory {
    type UnitOfWork: UnitOfWork;

    fn begin(&self) -> Self::UnitOfWork;
}
