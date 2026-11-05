use crate::application::{
    FindTasksQuery, TaskApplicationError, TaskApplicationService, TaskOutput, TaskStatusDto,
    TaskSummary, TaskSummaryReader,
};
use crate::domain::{TaskError, TaskRepository};

pub const HELP_TEXT: &str = "\
commands:
  create <id> <title>   タスクを作成する
  rename <id> <title>   タスク名を変更する
  complete <id>         タスクを完了にする
  archive <id>          タスクをアーカイブする
  show <id>             タスクを1件表示する
  list [open|completed|archived]  タスク一覧を表示する
  help                  このヘルプを表示する
  quit                  終了する";

/// CLIから受け取る1行の入力を、型付きのコマンドとして表したもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Create { id: String, title: String },
    Rename { id: String, title: String },
    Complete { id: String },
    Archive { id: String },
    Show { id: String },
    List { status: Option<TaskStatusDto> },
    Help,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandParseError {
    Empty,
    UnknownCommand(String),
    MissingArgument { usage: &'static str },
    UnknownStatus(String),
}

impl CommandParseError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "コマンドを入力してください".to_owned(),
            Self::UnknownCommand(name) => {
                format!("不明なコマンドです: {name}(help で一覧を表示)")
            }
            Self::MissingArgument { usage } => format!("引数が足りません。使い方: {usage}"),
            Self::UnknownStatus(value) => format!(
                "不明な状態です: {value}(open / completed / archived のいずれか)"
            ),
        }
    }
}

/// 文字列の入力を `Command` へ変換する。
///
/// ここで行うのは「入力の形式」のチェックだけで、タスク名の長さのような
/// 業務ルールの検証はドメイン層(`TaskTitle::new` など)に任せる。
pub fn parse_command(line: &str) -> Result<Command, CommandParseError> {
    let (name, rest) = split_first(line.trim());

    match name {
        "" => Err(CommandParseError::Empty),
        "create" => {
            let (id, title) = require_id_and_title(rest, "create <id> <title>")?;
            Ok(Command::Create { id, title })
        }
        "rename" => {
            let (id, title) = require_id_and_title(rest, "rename <id> <title>")?;
            Ok(Command::Rename { id, title })
        }
        "complete" => Ok(Command::Complete {
            id: require_id(rest, "complete <id>")?,
        }),
        "archive" => Ok(Command::Archive {
            id: require_id(rest, "archive <id>")?,
        }),
        "show" => Ok(Command::Show {
            id: require_id(rest, "show <id>")?,
        }),
        "list" => {
            let status = match rest {
                "" => None,
                "open" => Some(TaskStatusDto::Open),
                "completed" => Some(TaskStatusDto::Completed),
                "archived" => Some(TaskStatusDto::Archived),
                other => return Err(CommandParseError::UnknownStatus(other.to_owned())),
            };
            Ok(Command::List { status })
        }
        "help" => Ok(Command::Help),
        "quit" | "exit" => Ok(Command::Quit),
        other => Err(CommandParseError::UnknownCommand(other.to_owned())),
    }
}

fn split_first(input: &str) -> (&str, &str) {
    match input.split_once(char::is_whitespace) {
        Some((head, tail)) => (head, tail.trim()),
        None => (input, ""),
    }
}

fn require_id(rest: &str, usage: &'static str) -> Result<String, CommandParseError> {
    let (id, _) = split_first(rest);
    if id.is_empty() {
        return Err(CommandParseError::MissingArgument { usage });
    }
    Ok(id.to_owned())
}

fn require_id_and_title(
    rest: &str,
    usage: &'static str,
) -> Result<(String, String), CommandParseError> {
    let (id, title) = split_first(rest);
    if id.is_empty() || title.is_empty() {
        return Err(CommandParseError::MissingArgument { usage });
    }
    Ok((id.to_owned(), title.to_owned()))
}

/// CLIの入出力とユースケースをつなぐプレゼンテーション層の窓口。
///
/// 具体的なRepositoryやReaderの型は知らず、トレイト境界だけに依存する。
/// どの実装を渡すかは `main.rs`(コンポジションルート)が決める。
pub struct CliApp<R, Q>
where
    R: TaskRepository,
    Q: TaskSummaryReader,
{
    service: TaskApplicationService<R>,
    reader: Q,
    per_page: u64,
}

impl<R, Q> CliApp<R, Q>
where
    R: TaskRepository,
    Q: TaskSummaryReader,
{
    pub fn new(service: TaskApplicationService<R>, reader: Q, per_page: u64) -> Self {
        Self {
            service,
            reader,
            per_page,
        }
    }

    /// コマンドを実行し、画面に表示する文字列を返す。
    pub fn execute(&mut self, command: Command) -> Result<String, TaskApplicationError> {
        match command {
            Command::Create { id, title } => {
                let output = self.service.create_task(id, title)?;
                Ok(format!("作成しました: {}", format_output(&output)))
            }
            Command::Rename { id, title } => {
                let output = self.service.rename_task(id, title)?;
                Ok(format!("変更しました: {}", format_output(&output)))
            }
            Command::Complete { id } => {
                let output = self.service.complete_task(id)?;
                Ok(format!("完了にしました: {}", format_output(&output)))
            }
            Command::Archive { id } => {
                let output = self.service.archive_task(id)?;
                Ok(format!("アーカイブしました: {}", format_output(&output)))
            }
            Command::Show { id } => {
                let output = self.service.get_task(id)?;
                Ok(format_output(&output))
            }
            Command::List { status } => {
                let query = FindTasksQuery {
                    status,
                    page: 1,
                    per_page: self.per_page,
                };
                let summaries = self.reader.find_tasks(&query);
                Ok(format_summaries(&summaries))
            }
            Command::Help => Ok(HELP_TEXT.to_owned()),
            Command::Quit => Ok(String::new()),
        }
    }
}

/// アプリケーション層のエラーを、利用者向けのメッセージへ変換する。
///
/// エラーの「意味」はドメイン層・アプリケーション層が決め、
/// 「どう見せるか」はプレゼンテーション層が決める。
pub fn error_message(error: &TaskApplicationError) -> String {
    match error {
        TaskApplicationError::TaskAlreadyExists { task_id } => {
            format!("タスク {task_id} はすでに存在します")
        }
        TaskApplicationError::TaskNotFound { task_id } => {
            format!("タスク {task_id} が見つかりません")
        }
        TaskApplicationError::Domain(error) => match error {
            TaskError::EmptyTaskId => "タスクIDを入力してください".to_owned(),
            TaskError::EmptyTaskTitle => "タスク名を入力してください".to_owned(),
            TaskError::TaskTitleTooLong { max, actual } => {
                format!("タスク名は{max}文字以内にしてください(現在{actual}文字)")
            }
            TaskError::TaskAlreadyCompleted => "このタスクはすでに完了しています".to_owned(),
            TaskError::TaskAlreadyArchived => {
                "このタスクはすでにアーカイブされています".to_owned()
            }
            TaskError::TaskNotOpen { current } => {
                format!("未完了のタスクだけが変更できます(現在の状態: {current})")
            }
            TaskError::InvalidTaskStatus { value } => format!("不正な状態です: {value}"),
        },
    }
}

fn format_output(output: &TaskOutput) -> String {
    format!(
        "[{}] {} ({})",
        output.id,
        output.title,
        status_label(output.status)
    )
}

fn format_summaries(summaries: &[TaskSummary]) -> String {
    if summaries.is_empty() {
        return "タスクはありません".to_owned();
    }

    summaries
        .iter()
        .map(|summary| {
            format!(
                "[{}] {} ({})",
                summary.id,
                summary.title,
                status_label(summary.status)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn status_label(status: TaskStatusDto) -> &'static str {
    match status {
        TaskStatusDto::Open => "open",
        TaskStatusDto::Completed => "completed",
        TaskStatusDto::Archived => "archived",
    }
}
