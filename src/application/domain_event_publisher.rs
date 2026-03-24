use crate::domain::TaskEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainEventPublishError {
    PublishFailed { reason: String },
}

pub trait DomainEventPublisher {
    fn publish(&self, event: &TaskEvent) -> Result<(), DomainEventPublishError>;
}