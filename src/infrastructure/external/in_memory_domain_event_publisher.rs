use std::sync::{Arc, RwLock};

use crate::application::{DomainEventPublishError, DomainEventPublisher};
use crate::domain::TaskEvent;

#[derive(Debug, Clone, Default)]
pub struct InMemoryDomainEventPublisher {
    events: Arc<RwLock<Vec<TaskEvent>>>,
}

impl InMemoryDomainEventPublisher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<TaskEvent> {
        self.events.read().unwrap().clone()
    }
}

impl DomainEventPublisher for InMemoryDomainEventPublisher {
    fn publish(&self, event: &TaskEvent) -> Result<(), DomainEventPublishError> {
        self.events.write().unwrap().push(event.clone());
        Ok(())
    }
}