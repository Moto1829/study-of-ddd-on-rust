//! タスク管理アプリのエントリポイント。
//!
//! `main.rs` の責務は次の4つだけに絞っている。
//! 1. 設定を読み込む
//! 2. 依存を組み立てる(コンポジションルート)
//! 3. 入出力ループを回す
//! 4. 結果を終了コードへ変換する
//!
//! 業務ルールやユースケースの進行はここに書かない。

use std::env;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use study_of_ddd_on_rust::application::{TaskApplicationService, TaskCompletedNotificationHandler};
use study_of_ddd_on_rust::domain::{OpenTaskLimitPolicy, UserId};
use study_of_ddd_on_rust::infrastructure::{InMemoryTaskRepository, LoggingTaskCompletionNotifier};
use study_of_ddd_on_rust::presentation::{CliApp, Command, HELP_TEXT, error_message, parse_command};

const DEFAULT_PER_PAGE: u64 = 20;
const DEFAULT_USER: &str = "me";

/// 起動時に一度だけ読み込む設定値。
struct Config {
    per_page: u64,
    user: String,
    max_open_tasks: Option<usize>,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let per_page = match env::var("TASK_APP_PER_PAGE") {
            Ok(value) => value
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("TASK_APP_PER_PAGE は1以上の整数で指定してください: {value}"))?,
            Err(_) => DEFAULT_PER_PAGE,
        };

        let user = env::var("TASK_APP_USER").unwrap_or_else(|_| DEFAULT_USER.to_owned());

        let max_open_tasks = match env::var("TASK_APP_MAX_OPEN_TASKS") {
            Ok(value) => Some(
                value
                    .parse::<usize>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| {
                        format!("TASK_APP_MAX_OPEN_TASKS は1以上の整数で指定してください: {value}")
                    })?,
            ),
            Err(_) => None,
        };

        Ok(Self {
            per_page,
            user,
            max_open_tasks,
        })
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    // 1. 設定の読み込み
    let config = Config::from_env()?;

    // 2. 依存の組み立て(コンポジションルート)
    //    InMemoryTaskRepository は内部で状態を Arc 共有しているため、
    //    clone したものを書き込み用と読み取り用に渡すと同じデータを参照できる。
    let repository = InMemoryTaskRepository::new();
    // 完了イベントを受けて通知する購読者を、イベントの発行先として渡す
    let event_publisher = TaskCompletedNotificationHandler::new(LoggingTaskCompletionNotifier);
    let creation_policy = config
        .max_open_tasks
        .map(OpenTaskLimitPolicy::new)
        .unwrap_or_default();
    let owner = UserId::new(config.user).map_err(|error| error.to_string())?;
    let service = TaskApplicationService::new(
        repository.clone(),
        event_publisher,
        creation_policy,
        owner,
    );
    let mut app = CliApp::new(service, repository, config.per_page);

    // 3. 入出力ループ
    println!("タスク管理アプリ(help でコマンド一覧、quit で終了)");
    println!("{HELP_TEXT}");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush().map_err(|error| error.to_string())?;

        let mut line = String::new();
        let read = stdin
            .lock()
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;

        // EOF(Ctrl-D や入力のパイプ終了)なら正常終了
        if read == 0 {
            println!();
            return Ok(());
        }

        match parse_command(&line) {
            Ok(Command::Quit) => return Ok(()),
            Ok(command) => match app.execute(command) {
                Ok(output) => println!("{output}"),
                // ユースケースの失敗は利用者の入力ミスなので、プロセスは止めない
                Err(error) => eprintln!("{}", error_message(&error)),
            },
            Err(error) => eprintln!("{}", error.message()),
        }
    }
}
