use crate::domain::{TaskEvent, TaskId, TaskRepository};

use super::{DomainEventPublisher, TaskApplicationError, TaskOutput, TransactionRunner};

/// 複数タスクをまとめて完了するユースケース。
///
/// 「1件でも完了できなければ、すべて取り消す」という全件成功のみ確定する方針を採る。
/// どこからどこまでを1つのトランザクションで包むかはユースケースの設計判断なので、
/// トランザクション境界は Application Service が `TransactionRunner` を使って決める。
/// ドメイン層(`Task`)はトランザクションの存在を知らない。
///
/// `TaskCompleted` イベントは、トランザクションが確定したあとでまとめて発行する。
/// 途中で取り消された完了について、通知が飛んでしまうことを防ぐためである。
pub struct BulkTaskCompletionService<R, T, P>
where
    R: TaskRepository,
    T: TransactionRunner<TaskApplicationError>,
    P: DomainEventPublisher,
{
    repository: R,
    transaction_runner: T,
    event_publisher: P,
}

impl<R, T, P> BulkTaskCompletionService<R, T, P>
where
    R: TaskRepository,
    T: TransactionRunner<TaskApplicationError>,
    P: DomainEventPublisher,
{
    pub fn new(repository: R, transaction_runner: T, event_publisher: P) -> Self {
        Self {
            repository,
            transaction_runner,
            event_publisher,
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

        let (outputs, events) = self.transaction_runner.run(|| {
            let mut outputs = Vec::new();
            let mut events = Vec::new();

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
                events.push(TaskEvent::TaskCompleted {
                    task_id: task.id().clone(),
                });
                repository.save(task);
            }

            Ok((outputs, events))
        })?;

        // トランザクションが確定してから、起きた事実を発行する
        for event in &events {
            self.event_publisher.publish(event)?;
        }

        Ok(outputs)
    }
}
