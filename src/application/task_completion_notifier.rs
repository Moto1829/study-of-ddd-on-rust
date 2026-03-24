#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationError {
    SendFailed { reason: String },
}

pub trait TaskCompletionNotifier {
    fn notify_task_completed(&self, task_id: &str) -> Result<(), NotificationError>;
}