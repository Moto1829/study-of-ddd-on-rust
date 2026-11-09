pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use crate::application::{
        BulkTaskCompletionService, DomainEventPublisher, FindTasksQuery, NotificationError,
        TaskApplicationError, TaskApplicationService, TaskCompletedNotificationHandler,
        TaskCompletionNotifier, TaskStatusDto, TaskSummaryReader, TransactionRunner,
    };
    use crate::domain::{
        OpenTaskLimitPolicy, Task, TaskCreationPolicyError, TaskError, TaskId, TaskRepository,
        TaskStatus,
        TaskEvent, TaskTitle, UserId,
    };
    use crate::infrastructure::{
        InMemoryDomainEventPublisher, InMemoryTaskRepository, InMemoryTransactionRunner,
        LoggingTaskCompletionNotifier, TaskRow,
    };
    use crate::presentation::{CliApp, Command, CommandParseError, error_message, parse_command};

    type TestService = TaskApplicationService<InMemoryTaskRepository, InMemoryDomainEventPublisher>;

    /// テスト用の小さなコンポジションルート。発行されたイベントは publisher に記録される。
    fn new_service_with_limit(
        repository: InMemoryTaskRepository,
        max_open_tasks: usize,
    ) -> (TestService, InMemoryDomainEventPublisher) {
        let publisher = InMemoryDomainEventPublisher::new();
        let service = TaskApplicationService::new(
            repository,
            publisher.clone(),
            OpenTaskLimitPolicy::new(max_open_tasks),
            UserId::new("user-1").unwrap(),
        );
        (service, publisher)
    }

    fn new_service(repository: InMemoryTaskRepository) -> TestService {
        new_service_with_limit(repository, 100).0
    }

    /// 通知されたタスクIDを記録する Spy。
    #[derive(Default)]
    struct SpyNotifier {
        notified: RefCell<Vec<String>>,
    }

    impl TaskCompletionNotifier for &SpyNotifier {
        fn notify_task_completed(&self, task_id: &str) -> Result<(), NotificationError> {
            self.notified.borrow_mut().push(task_id.to_owned());
            Ok(())
        }
    }

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
        let mut service = new_service(repository);

        let created = service.create_task("task-1", "Write repository chapter").unwrap();

        assert_eq!(created.id, "task-1");
        assert_eq!(created.title, "Write repository chapter");
        assert_eq!(created.status, TaskStatusDto::Open);
    }

    #[test]
    fn application_service_rejects_duplicate_task_ids() {
        let repository = InMemoryTaskRepository::new();
        let mut service = new_service(repository);

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
        let mut service = new_service(repository);

        service.create_task("task-1", "Finish sample app").unwrap();
        let completed = service.complete_task("task-1").unwrap();

        assert_eq!(completed.status, TaskStatusDto::Completed);
    }

    #[test]
    fn application_service_archives_existing_task() {
        let repository = InMemoryTaskRepository::new();
        let mut service = new_service(repository);

        service.create_task("task-1", "Archive sample").unwrap();
        let archived = service.archive_task("task-1").unwrap();

        assert_eq!(archived.status, TaskStatusDto::Archived);
    }

    #[test]
    fn application_service_returns_not_found_for_unknown_task() {
        let repository = InMemoryTaskRepository::new();
        let service = new_service(repository);

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
        let mut service = new_service(repository.clone());

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

    #[test]
    fn cli_parses_create_command_with_multi_word_title() {
        let command = parse_command("create task-1 Write chapter draft").unwrap();

        assert_eq!(
            command,
            Command::Create {
                id: "task-1".to_owned(),
                title: "Write chapter draft".to_owned(),
            }
        );
    }

    #[test]
    fn cli_rejects_command_without_required_argument() {
        let result = parse_command("complete");

        assert_eq!(
            result,
            Err(CommandParseError::MissingArgument {
                usage: "complete <id>",
            })
        );
    }

    #[test]
    fn cli_app_shares_state_between_service_and_reader() {
        let repository = InMemoryTaskRepository::new();
        let service = new_service(repository.clone());
        let mut app = CliApp::new(service, repository, 20);

        app.execute(Command::Create {
            id: "task-1".to_owned(),
            title: "Write chapter draft".to_owned(),
        })
        .unwrap();
        let listed = app.execute(Command::List { status: None }).unwrap();

        assert_eq!(listed, "[task-1] Write chapter draft (open)");
    }

    #[test]
    fn cli_converts_application_error_to_user_message() {
        let repository = InMemoryTaskRepository::new();
        let service = new_service(repository.clone());
        let mut app = CliApp::new(service, repository, 20);

        let error = app
            .execute(Command::Complete {
                id: "missing".to_owned(),
            })
            .unwrap_err();

        assert_eq!(error_message(&error), "タスク missing が見つかりません");
    }

    #[test]
    fn bulk_completion_completes_all_tasks_when_every_task_is_open() {
        let repository = InMemoryTaskRepository::new();
        let mut task_service = new_service(repository.clone());
        task_service.create_task("task-1", "Buy milk").unwrap();
        task_service.create_task("task-2", "Buy eggs").unwrap();

        let runner = InMemoryTransactionRunner::new(&repository);
        let mut bulk_service = BulkTaskCompletionService::new(repository.clone(), runner, InMemoryDomainEventPublisher::new());

        let outputs = bulk_service.complete_tasks(["task-1", "task-2"]).unwrap();

        assert_eq!(outputs.len(), 2);
        assert!(
            outputs
                .iter()
                .all(|output| output.status == TaskStatusDto::Completed)
        );
    }

    #[test]
    fn bulk_completion_rolls_back_earlier_tasks_when_a_later_task_fails() {
        let repository = InMemoryTaskRepository::new();
        let mut task_service = new_service(repository.clone());
        task_service.create_task("task-1", "Buy milk").unwrap();
        task_service.create_task("task-2", "Buy eggs").unwrap();
        task_service.complete_task("task-2").unwrap();

        let runner = InMemoryTransactionRunner::new(&repository);
        let mut bulk_service = BulkTaskCompletionService::new(repository.clone(), runner, InMemoryDomainEventPublisher::new());

        // task-1 は完了・保存されるが、task-2 は完了済みなので失敗する
        let result = bulk_service.complete_tasks(["task-1", "task-2"]);

        assert_eq!(
            result,
            Err(TaskApplicationError::Domain(TaskError::TaskAlreadyCompleted))
        );
        // 全件成功のみ確定する方針なので、task-1 の完了も取り消されている
        assert_eq!(
            task_service.get_task("task-1").unwrap().status,
            TaskStatusDto::Open
        );
    }

    #[test]
    fn bulk_completion_rolls_back_when_a_task_is_missing() {
        let repository = InMemoryTaskRepository::new();
        let mut task_service = new_service(repository.clone());
        task_service.create_task("task-1", "Buy milk").unwrap();

        let runner = InMemoryTransactionRunner::new(&repository);
        let mut bulk_service = BulkTaskCompletionService::new(repository.clone(), runner, InMemoryDomainEventPublisher::new());

        let result = bulk_service.complete_tasks(["task-1", "missing"]);

        assert_eq!(
            result,
            Err(TaskApplicationError::TaskNotFound {
                task_id: "missing".to_owned(),
            })
        );
        assert_eq!(
            task_service.get_task("task-1").unwrap().status,
            TaskStatusDto::Open
        );
    }

    #[test]
    fn in_memory_transaction_runner_keeps_changes_on_success() {
        let repository = InMemoryTaskRepository::new();
        let runner = InMemoryTransactionRunner::new(&repository);
        let mut writer = repository.clone();

        let result: Result<(), String> = runner.run(|| {
            writer.save(Task::new(
                TaskId::new("task-1").unwrap(),
                TaskTitle::new("Buy milk").unwrap(),
            ));
            Ok(())
        });

        assert!(result.is_ok());
        assert!(repository.find_by_id(&TaskId::new("task-1").unwrap()).is_some());
    }

    #[test]
    fn application_service_publishes_events_for_state_changes() {
        let (mut service, publisher) = new_service_with_limit(InMemoryTaskRepository::new(), 100);

        service.create_task("task-1", "Buy milk").unwrap();
        service.complete_task("task-1").unwrap();
        service.archive_task("task-1").unwrap();

        let task_id = TaskId::new("task-1").unwrap();
        assert_eq!(
            publisher.events(),
            vec![
                TaskEvent::TaskCreated {
                    task_id: task_id.clone()
                },
                TaskEvent::TaskCompleted {
                    task_id: task_id.clone()
                },
                TaskEvent::TaskArchived { task_id },
            ]
        );
    }

    #[test]
    fn application_service_does_not_publish_event_when_domain_rule_fails() {
        let (mut service, publisher) = new_service_with_limit(InMemoryTaskRepository::new(), 100);
        service.create_task("task-1", "Buy milk").unwrap();
        service.complete_task("task-1").unwrap();

        let result = service.complete_task("task-1");

        assert!(result.is_err());
        // TaskCreated と 1回目の TaskCompleted だけ
        assert_eq!(publisher.events().len(), 2);
    }

    #[test]
    fn application_service_rejects_creation_over_open_task_limit() {
        let (mut service, _) = new_service_with_limit(InMemoryTaskRepository::new(), 2);
        service.create_task("task-1", "Buy milk").unwrap();
        service.create_task("task-2", "Buy eggs").unwrap();

        let result = service.create_task("task-3", "Buy bread");

        assert_eq!(
            result,
            Err(TaskApplicationError::CreationPolicy(
                TaskCreationPolicyError::OpenTaskLimitExceeded {
                    user_id: "user-1".to_owned(),
                    max: 2,
                    actual: 2,
                }
            ))
        );
    }

    #[test]
    fn completing_a_task_frees_up_the_open_task_limit() {
        let (mut service, _) = new_service_with_limit(InMemoryTaskRepository::new(), 2);
        service.create_task("task-1", "Buy milk").unwrap();
        service.create_task("task-2", "Buy eggs").unwrap();
        service.complete_task("task-1").unwrap();

        let result = service.create_task("task-3", "Buy bread");

        assert!(result.is_ok());
    }

    #[test]
    fn repository_counts_only_open_tasks() {
        let repository = InMemoryTaskRepository::new();
        let mut service = new_service(repository.clone());
        service.create_task("task-1", "Open task").unwrap();
        service.create_task("task-2", "Done task").unwrap();
        service.complete_task("task-2").unwrap();

        assert_eq!(repository.count_open_tasks(), 1);
    }

    #[test]
    fn notification_handler_notifies_only_on_task_completed() {
        let spy = SpyNotifier::default();
        let handler = TaskCompletedNotificationHandler::new(&spy);
        let task_id = TaskId::new("task-1").unwrap();

        handler
            .publish(&TaskEvent::TaskCreated {
                task_id: task_id.clone(),
            })
            .unwrap();
        handler
            .publish(&TaskEvent::TaskCompleted {
                task_id: task_id.clone(),
            })
            .unwrap();
        handler
            .publish(&TaskEvent::TaskArchived { task_id })
            .unwrap();

        assert_eq!(*spy.notified.borrow(), vec!["task-1".to_owned()]);
    }

    #[test]
    fn completing_task_through_application_service_sends_notification() {
        let spy = SpyNotifier::default();
        let mut service = TaskApplicationService::new(
            InMemoryTaskRepository::new(),
            TaskCompletedNotificationHandler::new(&spy),
            OpenTaskLimitPolicy::default(),
            UserId::new("user-1").unwrap(),
        );

        service.create_task("task-1", "Buy milk").unwrap();
        service.complete_task("task-1").unwrap();

        assert_eq!(*spy.notified.borrow(), vec!["task-1".to_owned()]);
    }

    #[test]
    fn bulk_completion_publishes_events_only_after_commit() {
        let repository = InMemoryTaskRepository::new();
        let mut task_service = new_service(repository.clone());
        task_service.create_task("task-1", "Buy milk").unwrap();
        task_service.create_task("task-2", "Buy eggs").unwrap();
        task_service.complete_task("task-2").unwrap();

        let publisher = InMemoryDomainEventPublisher::new();
        let runner = InMemoryTransactionRunner::new(&repository);
        let mut bulk_service =
            BulkTaskCompletionService::new(repository.clone(), runner, publisher.clone());

        // task-2 が完了済みなので全体が取り消される。取り消された task-1 の完了イベントも出ない
        assert!(bulk_service.complete_tasks(["task-1", "task-2"]).is_err());
        assert!(publisher.events().is_empty());

        bulk_service.complete_tasks(["task-1"]).unwrap();
        assert_eq!(
            publisher.events(),
            vec![TaskEvent::TaskCompleted {
                task_id: TaskId::new("task-1").unwrap()
            }]
        );
    }
}
