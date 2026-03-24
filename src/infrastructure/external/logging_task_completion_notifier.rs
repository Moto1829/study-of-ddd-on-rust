use crate::application::{NotificationError, TaskCompletionNotifier};

#[derive(Debug, Default, Clone, Copy)]
pub struct LoggingTaskCompletionNotifier;

impl TaskCompletionNotifier for LoggingTaskCompletionNotifier {
    fn notify_task_completed(&self, task_id: &str) -> Result<(), NotificationError> {
        if task_id.trim().is_empty() {
            return Err(NotificationError::SendFailed {
                reason: "task id must not be empty".to_owned(),
            });
        }

        Ok(())
    }
}