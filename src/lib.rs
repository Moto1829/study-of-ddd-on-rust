pub mod application;
pub mod domain;
pub mod infrastructure;

#[cfg(test)]
mod tests {
    use crate::application::{
        DomainEventPublisher, FindTasksQuery, TaskApplicationError, TaskApplicationService,
        TaskCompletionNotifier, TaskStatusDto, TaskSummaryReader,
    };
    use crate::domain::{
        OpenTaskLimitPolicy, Task, TaskCreationPolicyError, TaskError, TaskId, TaskStatus,
        TaskEvent, TaskTitle, UserId,
    };
    use crate::infrastructure::{
        InMemoryDomainEventPublisher, InMemoryTaskRepository, LoggingTaskCompletionNotifier,
        TaskRow,
    };

    #[test]
    fn task_title_rejects_empty_input() {
        let result = TaskTitle::new("   ");

        assert_eq!(result, Err(TaskError::EmptyTaskTitle));
    }

    #[test]
    fn task_title_rejects_too_long_input() {
        let title = "a".repeat(101);
        let result = TaskTitle::new(title);

        assert_eq!(
            result,
            Err(TaskError::TaskTitleTooLong {
                max: 100,
                actual: 101,
            })
        );
    }

    #[test]
    fn task_can_change_title_while_open() {
        let id = TaskId::new("task-1").unwrap();
        let title = TaskTitle::new("Write chapter draft").unwrap();
        let mut task = Task::new(id, title);

        task.rename(TaskTitle::new("Write introduction chapter").unwrap())
            .unwrap();

        assert_eq!(task.title().value(), "Write introduction chapter");
        assert_eq!(task.status(), TaskStatus::Open);
    }

    #[test]
    fn completed_task_cannot_be_completed_twice() {
        let id = TaskId::new("task-1").unwrap();
        let title = TaskTitle::new("Publish mdBook").unwrap();
        let mut task = Task::new(id, title);

        task.complete().unwrap();
        let result = task.complete();

        assert_eq!(result, Err(TaskError::TaskAlreadyCompleted));
    }

    #[test]
    fn archived_task_cannot_be_renamed() {
        let id = TaskId::new("task-1").unwrap();
        let title = TaskTitle::new("Archive me").unwrap();
        let mut task = Task::new(id, title);

        task.archive().unwrap();
        let result = task.rename(TaskTitle::new("Still editable?").unwrap());

        assert_eq!(
            result,
            Err(TaskError::TaskNotOpen {
                current: TaskStatus::Archived,
            })
        );
    }

    #[test]
    fn application_service_creates_task() {
        let repository = InMemoryTaskRepository::new();
        let mut service = TaskApplicationService::new(repository);

        let created = service.create_task("task-1", "Write repository chapter").unwrap();

        assert_eq!(created.id, "task-1");
        assert_eq!(created.title, "Write repository chapter");
        assert_eq!(created.status, TaskStatusDto::Open);
    }

    #[test]
    fn application_service_rejects_duplicate_task_ids() {
        let repository = InMemoryTaskRepository::new();
        let mut service = TaskApplicationService::new(repository);

        service.create_task("task-1", "Write repository chapter").unwrap();
        let result = service.create_task("task-1", "Write service chapter");

        assert_eq!(
            result,
            Err(TaskApplicationError::TaskAlreadyExists {
                task_id: "task-1".to_owned(),
            })
        );
    }

    #[test]
    fn application_service_completes_existing_task() {
        let repository = InMemoryTaskRepository::new();
        let mut service = TaskApplicationService::new(repository);

        service.create_task("task-1", "Finish sample app").unwrap();
        let completed = service.complete_task("task-1").unwrap();

        assert_eq!(completed.status, TaskStatusDto::Completed);
    }

    #[test]
    fn application_service_archives_existing_task() {
        let repository = InMemoryTaskRepository::new();
        let mut service = TaskApplicationService::new(repository);

        service.create_task("task-1", "Archive sample").unwrap();
        let archived = service.archive_task("task-1").unwrap();

        assert_eq!(archived.status, TaskStatusDto::Archived);
    }

    #[test]
    fn application_service_returns_not_found_for_unknown_task() {
        let repository = InMemoryTaskRepository::new();
        let service = TaskApplicationService::new(repository);

        let result = service.get_task("missing-task");

        assert_eq!(
            result,
            Err(TaskApplicationError::TaskNotFound {
                task_id: "missing-task".to_owned(),
            })
        );
    }

    #[test]
    fn query_returns_only_open_tasks() {
        let repository = InMemoryTaskRepository::new();
        let mut service = TaskApplicationService::new(repository.clone());

        service.create_task("task-1", "Open task").unwrap();
        service.create_task("task-2", "Done task").unwrap();
        service.create_task("task-3", "Archived task").unwrap();
        service.complete_task("task-2").unwrap();
        service.archive_task("task-3").unwrap();

        let summaries = repository.find_tasks(&FindTasksQuery {
            status: Some(TaskStatusDto::Open),
            page: 1,
            per_page: 10,
        });

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "task-1");
        assert_eq!(summaries[0].status, TaskStatusDto::Open);
    }

    #[test]
    fn task_row_round_trip_preserves_status() {
        let id = TaskId::new("task-1").unwrap();
        let title = TaskTitle::new("Persist task").unwrap();
        let task = Task::restore(id, title, TaskStatus::Archived);

        let row = TaskRow::from(&task);
        let restored = Task::try_from(row).unwrap();

        assert_eq!(restored.status(), TaskStatus::Archived);
        assert_eq!(restored.title().value(), "Persist task");
    }

    #[test]
    fn task_row_rejects_invalid_status() {
        let row = TaskRow {
            id: "task-1".to_owned(),
            title: "Persist task".to_owned(),
            status: "unknown".to_owned(),
        };

        let result = Task::try_from(row);

        assert_eq!(
            result,
            Err(TaskError::InvalidTaskStatus {
                value: "unknown".to_owned(),
            })
        );
    }

    #[test]
    fn open_task_limit_policy_rejects_user_over_quota() {
        let policy = OpenTaskLimitPolicy::new(3);
        let user_id = UserId::new("user-1").unwrap();

        let result = policy.ensure_can_create(&user_id, 3);

        assert_eq!(
            result,
            Err(TaskCreationPolicyError::OpenTaskLimitExceeded {
                user_id: "user-1".to_owned(),
                max: 3,
                actual: 3,
            })
        );
    }

    #[test]
    fn logging_notifier_accepts_non_empty_task_id() {
        let notifier = LoggingTaskCompletionNotifier;

        let result = notifier.notify_task_completed("task-1");

        assert!(result.is_ok());
    }

    #[test]
    fn in_memory_event_publisher_stores_published_events() {
        let publisher = InMemoryDomainEventPublisher::new();
        let event = TaskEvent::TaskCompleted {
            task_id: TaskId::new("task-1").unwrap(),
        };

        publisher.publish(&event).unwrap();

        assert_eq!(publisher.events(), vec![event]);
    }
}