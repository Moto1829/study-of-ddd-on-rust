pub mod application;
pub mod domain;
pub mod infrastructure;

#[cfg(test)]
mod tests {
    use crate::application::{
        TaskApplicationError, TaskApplicationService, TaskStatusDto,
    };
    use crate::domain::{Task, TaskError, TaskId, TaskStatus, TaskTitle};
    use crate::infrastructure::InMemoryTaskRepository;

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
}