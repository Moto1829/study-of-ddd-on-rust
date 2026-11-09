use crate::domain::{TaskId, TaskRepository};

use super::{TaskApplicationError, TaskOutput, TransactionRunner};

/// 複数タスクをまとめて完了するユースケース。
///
/// 「1件でも完了できなければ、すべて取り消す」という全件成功のみ確定する方針を採る。
/// どこからどこまでを1つのトランザクションで包むかはユースケースの設計判断なので、
/// トランザクション境界は Application Service が `TransactionRunner` を使って決める。
/// ドメイン層(`Task`)はトランザクションの存在を知らない。
pub struct BulkTaskCompletionService<R, T>
where
    R: TaskRepository,
    T: TransactionRunner<TaskApplicationError>,
{
    repository: R,
    transaction_runner: T,
}

impl<R, T> BulkTaskCompletionService<R, T>
where
    R: TaskRepository,
    T: TransactionRunner<TaskApplicationError>,
{
    pub fn new(repository: R, transaction_runner: T) -> Self {
        Self {
            repository,
            transaction_runner,
        }
    }

    pub fn complete_tasks<I, S>(&mut self, task_ids: I) -> Result<Vec<TaskOutput>, TaskApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        // クロージャは repository だけを可変借用し、transaction_runner は不変借用する。
        // 別々のフィールドなので、同時に借用しても衝突しない。
        let repository = &mut self.repository;

        self.transaction_runner.run(|| {
            let mut outputs = Vec::new();

            for task_id in task_ids {
                let task_id = TaskId::new(task_id)?;
                let mut task = repository.find_by_id(&task_id).ok_or_else(|| {
                    TaskApplicationError::TaskNotFound {
                        task_id: task_id.value().to_owned(),
                    }
                })?;

                // ここで失敗すると、それまでに保存した分も TransactionRunner が取り消す
                task.complete()?;
                outputs.push(TaskOutput::from(&task));
                repository.save(task);
            }

            Ok(outputs)
        })
    }
}
