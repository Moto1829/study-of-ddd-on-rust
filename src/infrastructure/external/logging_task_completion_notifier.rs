use crate::application::{NotificationError, TaskCompletionNotifier};

/// 完了通知を標準エラー出力へのログで代用する、Infrastructure層の最小実装。
#[derive(Debug, Default, Clone, Copy)]
pub struct LoggingTaskCompletionNotifier;

impl TaskCompletionNotifier for LoggingTaskCompletionNotifier {
    fn notify_task_completed(&self, task_id: &str) -> Result<(), NotificationError> {
        if task_id.trim().is_empty() {
            return Err(NotificationError::SendFailed {
                reason: "task id must not be empty".to_owned(),
            });
        }

        // 実際のメール送信やWebhookの代わりに、標準エラー出力へログを書く
        eprintln!("[通知] タスク {task_id} が完了しました");
        Ok(())
    }
}