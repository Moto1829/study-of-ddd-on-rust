use crate::domain::TaskEvent;

use super::{DomainEventPublishError, DomainEventPublisher, TaskCompletionNotifier};

/// `TaskCompleted` イベントを受け取って、完了通知を送る購読者。
///
/// 同じプロセスの中で同期的にイベントを届ける最小構成では、
/// 購読者そのものを `DomainEventPublisher` の実装として Application Service に渡せる。
/// こうすると Application Service は「完了したという事実を発行する」ことだけを知り、
/// 「完了したら通知する」という後続の処理を知らずに済む。
///
/// 通知の手段(ログ、メール、Webhookなど)は `TaskCompletionNotifier` の実装に任せる。
pub struct TaskCompletedNotificationHandler<N>
where
    N: TaskCompletionNotifier,
{
    notifier: N,
}

impl<N> TaskCompletedNotificationHandler<N>
where
    N: TaskCompletionNotifier,
{
    pub fn new(notifier: N) -> Self {
        Self { notifier }
    }
}

impl<N> DomainEventPublisher for TaskCompletedNotificationHandler<N>
where
    N: TaskCompletionNotifier,
{
    fn publish(&self, event: &TaskEvent) -> Result<(), DomainEventPublishError> {
        match event {
            TaskEvent::TaskCompleted { task_id } => self
                .notifier
                .notify_task_completed(task_id.value())
                .map_err(|error| DomainEventPublishError::PublishFailed {
                    reason: format!("{error:?}"),
                }),
            // 完了以外のイベントには反応しない
            TaskEvent::TaskCreated { .. } | TaskEvent::TaskArchived { .. } => Ok(()),
        }
    }
}
