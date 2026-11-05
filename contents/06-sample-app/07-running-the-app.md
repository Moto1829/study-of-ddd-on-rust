# 第6章-7 アプリケーションを起動する: main関数の設計

ここまでの章では、ドメイン層・アプリケーション層・インフラ層を作り、すべてテストから動かしてきました。しかし実際のアプリケーションは、どこかで `main()` から起動され、利用者の入力を受け取り、結果を返し、終了します。

この章では「DDDで作ったコードを、実行可能なプログラムとしてどう起動するか」に着目します。題材として、タスク管理アプリにCLI(対話型コマンドライン)の入口を追加しました。

```bash
cargo run
```

```text
タスク管理アプリ(help でコマンド一覧、quit で終了)
> create task-1 Write chapter draft
作成しました: [task-1] Write chapter draft (open)
> complete task-1
完了にしました: [task-1] Write chapter draft (completed)
> rename task-1 New title
未完了のタスクだけが変更できます(現在の状態: completed)
> list
[task-1] Write chapter draft (completed)
> quit
```

## 起動まわりで追加したファイル

```text
src/
├── lib.rs                 # ライブラリクレートのルート(各層を公開)
├── main.rs                # バイナリクレートのルート(エントリポイント)
├── domain/
├── application/
├── infrastructure/
└── presentation/          # 今回追加
    └── cli.rs             # 入力の解析、ユースケース呼び出し、出力の整形
```

ポイントは、`main.rs` と `presentation/` を足しただけで、**ドメイン層とアプリケーション層には一切手を入れていない** ことです。入口を足しても内側が変わらないのは、ここまで依存方向を守ってきた成果です。

## lib.rs と main.rs を分ける理由

Cargoは、`src/lib.rs` があればライブラリクレート、`src/main.rs` があればバイナリクレートとして扱います。両方あると、同じパッケージの中に2つのクレートができます。

```rust
// main.rs から見ると、lib.rs 側は外部クレートと同じ扱いになる
use study_of_ddd_on_rust::application::TaskApplicationService;
use study_of_ddd_on_rust::infrastructure::InMemoryTaskRepository;
use study_of_ddd_on_rust::presentation::{CliApp, Command, parse_command};
```

この分け方には次の利点があります。

- **ロジックをテストしやすい**: `main.rs` の中に書いたコードは単体テストから呼び出しにくいですが、`lib.rs` 側に置いたものは `cargo test` で普通にテストできる
- **入口を増やしやすい**: 将来 `src/bin/batch.rs` や Webサーバー用のバイナリを追加しても、同じライブラリを共有できる
- **公開範囲が明確になる**: `main.rs` は `pub` なものしか使えないため、各層が外へ何を見せているかが自然と整理される

DDDの観点では、`lib.rs` 側が「アプリケーションの本体」、`main.rs` 側が「それを起動して外界とつなぐ薄い殻」という役割分担になります。

## main.rs の責務は4つだけ

`main.rs` に置くのは、次の4つだけにしています。

1. 設定を読み込む
2. 依存を組み立てる(コンポジションルート)
3. 入出力ループを回す
4. 結果を終了コードへ変換する

逆に言えば、**業務ルールとユースケースの進行は `main.rs` に書きません**。「タスク名は100文字まで」「完了済みタスクはリネームできない」といった判断が `main.rs` に現れたら、それはドメイン層かアプリケーション層へ移すべきサインです。

全体の構造は次のとおりです。

```rust
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
    let repository = InMemoryTaskRepository::new();
    let service = TaskApplicationService::new(repository.clone());
    let mut app = CliApp::new(service, repository, config.per_page);

    // 3. 入出力ループ
    loop {
        // 1行読み取り → parse_command → app.execute → 表示
    }
}
```

`main()` 自体は `run()` を呼んで結果を終了コードへ変換するだけです。`run()` が `Result` を返すようにしておくと、内部では `?` 演算子で素直にエラーを伝播でき、最後に1か所でまとめて処理できます。

### 1. 設定の読み込み

設定値(ページサイズ、DB接続文字列、APIキーなど)は、起動時に一度だけ読み込み、型付きの構造体にまとめます。

```rust
struct Config {
    per_page: u64,
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

        Ok(Self { per_page })
    }
}
```

ここで大切なのは次の2点です。

- **ドメイン層やアプリケーション層に環境変数を読ませない**: `env::var` が内側の層に散らばると、テストのたびに環境変数を準備する必要が出てきます。設定は外側で読み、値として内側へ渡します
- **不正な設定は起動時に失敗させる**: 設定ミスに処理の途中で気づくより、起動直後に分かりやすいメッセージで終了する方がずっと扱いやすくなります(fail fast)

```bash
$ TASK_APP_PER_PAGE=0 cargo run
error: TASK_APP_PER_PAGE は1以上の整数で指定してください: 0
$ echo $?
1
```

### 2. 依存の組み立て

依存の組み立ては、第5章-3のコラムで扱った **コンポジションルート** そのものです。具体的な実装を選ぶのはここだけで、`CliApp` も `TaskApplicationService` もトレイト境界にしか依存していません。

今回の構成で1つ注目してほしいのは、`repository.clone()` です。

```rust
let repository = InMemoryTaskRepository::new();
let service = TaskApplicationService::new(repository.clone()); // 書き込み側
let mut app = CliApp::new(service, repository, config.per_page); // 読み取り側
```

書き込みは `TaskApplicationService` 経由で `TaskRepository` を使い、一覧表示は `TaskSummaryReader` を直接使います。`InMemoryTaskRepository` は内部の `HashMap` を `Arc<RwLock<...>>` で包んでいるため、clone しても同じデータを指します。そのため、作成したタスクが一覧にもちゃんと現れます。

DB実装に置き換えた場合も考え方は同じで、コネクションプールを共有した Repository と Reader をそれぞれ作って渡すことになります。「同じデータソースを、書き込み用と読み取り用の別の窓口から使う」という組み立てを1か所で表現できるのが、コンポジションルートの利点です。

### 3. 入出力ループ

入出力ループの中身は、1行読んで、解析して、実行して、表示する、の繰り返しです。

```rust
match parse_command(&line) {
    Ok(Command::Quit) => return Ok(()),
    Ok(command) => match app.execute(command) {
        Ok(output) => println!("{output}"),
        Err(error) => eprintln!("{}", error_message(&error)),
    },
    Err(error) => eprintln!("{}", error.message()),
}
```

`main.rs` は `stdin` と `stdout` を扱うだけで、コマンドの意味は知りません。解析・実行・表示用の変換は、次に説明するプレゼンテーション層に任せています。

### 4. 終了コード

Rustの `main` は `ExitCode` を返せます。CLIアプリケーションでは、終了コードはシェルスクリプトやCIから結果を判定するための大切な情報です。

ここで区別しておきたいのは、**エラーには2種類ある** ということです。

| 種類 | 例 | 扱い |
| --- | --- | --- |
| 起動・実行基盤のエラー | 設定値が不正、標準入力が読めない | `run()` から `Err` を返し、終了コード1で終了する |
| ユースケースのエラー | タスクが見つからない、タイトルが長すぎる | メッセージを表示して、ループを続ける |

ユースケースのエラーは、利用者の入力に対する正常な応答の一部です。「完了済みタスクはリネームできない」はドメインルールが正しく機能した結果であって、プログラムの異常ではありません。これをプロセス終了につなげてしまうと、ドメインのエラーと基盤の障害の区別がつかなくなります。

## プレゼンテーション層の役割

`presentation/cli.rs` は、外界の入出力とアプリケーション層の間を取り持つ層です。ここでは3つの仕事をしています。

### 入力を型付きのコマンドへ変換する

```rust
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

pub fn parse_command(line: &str) -> Result<Command, CommandParseError>
```

`parse_command` が検証するのは、「`create` には ID とタイトルの2つの引数が必要」といった **入力の形式** だけです。タイトルが空でないか、100文字以内かといった **業務ルール** はチェックしません。それは `TaskTitle::new` の仕事です。

もしプレゼンテーション層でもタイトルの長さをチェックすると、同じルールが2か所に書かれることになり、片方だけ変更されて食い違う原因になります。プレゼンテーション層の検証は「解釈できる形になっているか」までに留め、意味のある値かどうかはドメインに判断させます。

### ユースケースを呼び出す

```rust
pub struct CliApp<R, Q>
where
    R: TaskRepository,
    Q: TaskSummaryReader,
{
    service: TaskApplicationService<R>,
    reader: Q,
    per_page: u64,
}
```

`CliApp::execute` は `Command` に応じて Application Service のメソッドを呼ぶだけです。ここでも具体的な Repository の型は登場せず、ジェネリクスのまま扱っています。

### エラーを利用者向けの言葉へ変換する

```rust
pub fn error_message(error: &TaskApplicationError) -> String {
    match error {
        TaskApplicationError::TaskNotFound { task_id } => {
            format!("タスク {task_id} が見つかりません")
        }
        TaskApplicationError::Domain(TaskError::TaskTitleTooLong { max, actual }) => {
            format!("タスク名は{max}文字以内にしてください(現在{actual}文字)")
        }
        // ...
    }
}
```

エラーの **意味** (何が起きたか)はドメイン層とアプリケーション層が型で表現し、**見せ方** (どんな言葉で、どの言語で伝えるか)はプレゼンテーション層が決めます。この分担にしておくと、Webの入口を追加したときには「`TaskNotFound` は HTTP 404」「`Domain(...)` は HTTP 422」というように、同じエラー型から別の表現を作れます。

## テストは main の手前で書く

`main.rs` は標準入出力に依存しているため、そのままではテストしにくい部分です。だからこそ、`main.rs` を薄く保ち、ロジックをテストできる場所へ寄せています。

```rust
#[test]
fn cli_app_shares_state_between_service_and_reader() {
    let repository = InMemoryTaskRepository::new();
    let service = TaskApplicationService::new(repository.clone());
    let mut app = CliApp::new(service, repository, 20);

    app.execute(Command::Create {
        id: "task-1".to_owned(),
        title: "Write chapter draft".to_owned(),
    })
    .unwrap();
    let listed = app.execute(Command::List { status: None }).unwrap();

    assert_eq!(listed, "[task-1] Write chapter draft (open)");
}
```

このテストは `main.rs` と同じ組み立てを再現しているので、「組み立て方を間違えて一覧に何も出ない」といった配線ミスも検出できます。`main.rs` に残るのは、標準入出力と終了コードの扱いという、ほとんど分岐のないコードだけです。

なお、パイプで入力を流し込めば、CLI全体の動作も手軽に確認できます。

```bash
printf 'create task-1 Write chapter draft\ncomplete task-1\nlist\n' | cargo run -q
```

## 発展: Webサーバーとして起動する場合

本書のサンプルは外部クレートを使わずに動かすためCLIにしていますが、実務ではWebサーバーとして起動することが多いでしょう。その場合も `main.rs` の責務は変わりません。以下は axum と tokio を使った場合のイメージです(リポジトリには含まれていません)。

```rust
use std::sync::{Arc, Mutex};

use axum::{Router, routing::post};

type AppService = TaskApplicationService<PostgresTaskRepository>;

#[derive(Clone)]
struct AppState {
    service: Arc<Mutex<AppService>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 設定の読み込み
    let config = Config::from_env()?;

    // 2. 依存の組み立て(コンポジションルート)
    let pool = PgPool::connect(&config.database_url).await?;
    let repository = PostgresTaskRepository::new(pool);
    let service = TaskApplicationService::new(repository);
    let state = AppState {
        service: Arc::new(Mutex::new(service)),
    };

    // 3. 入出力ループ(ここではHTTPサーバー)
    let app = Router::new()
        .route("/tasks", post(create_task))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // 4. 終了(エラーは anyhow 経由で終了コード1になる)
    Ok(())
}
```

CLI版と見比べると、構造がほぼ同じであることが分かります。違いは次の点です。

- **非同期ランタイム**: `#[tokio::main]` はランタイムを起動するためのマクロで、これも「起動の都合」なので `main.rs` に置きます。ドメイン層を `async` にするかどうかは別の設計判断です
- **共有状態**: HTTPリクエストは並行に処理されるため、Application Service を `Arc` で共有し、`&mut self` を要求するメソッドを呼ぶなら `Mutex` などで保護する必要があります
- **ハンドラ**: `create_task` のようなハンドラがCLI版の `CliApp::execute` に相当し、リクエストをコマンドへ変換し、エラーをHTTPステータスへ変換します
- **グレースフルシャットダウン**: `with_graceful_shutdown` で終了シグナルを受け取り、処理中のリクエストを終えてから停止します。これもドメインとは無関係な、起動・停止の関心事です

## 複数のエントリポイントを持つ場合

同じドメインを、APIサーバーとバッチ処理など複数の入口から使いたくなることがあります。Cargoでは `src/bin/` にファイルを置くと、それぞれが独立したバイナリになります。

```text
src/
├── lib.rs
├── main.rs            # cargo run(APIサーバーなど)
└── bin/
    └── archive_old_tasks.rs   # cargo run --bin archive_old_tasks
```

どのバイナリも `lib.rs` 側の同じドメインとユースケースを使い、それぞれの `main` で自分に必要な依存だけを組み立てます。たとえばバッチでは通知機能を無効にした実装を注入する、といった差し替えも、コンポジションルートが入口ごとに分かれているからこそ簡単にできます。

入口の数が増えて依存クレートが重くなってきたら、`Cargo workspaceに分割するタイミング` で扱う workspace 化を検討するとよいでしょう。

## まとめ

- `lib.rs` にアプリケーションの本体を置き、`main.rs` はそれを起動する薄い殻にする
- `main.rs` の責務は「設定の読み込み」「依存の組み立て」「入出力ループ」「終了コードへの変換」の4つに絞る
- 設定は外側で一度だけ読み、不正なら起動時に失敗させる
- 起動・基盤のエラーとユースケースのエラーを区別し、後者ではプロセスを止めない
- プレゼンテーション層は入力の形式チェック、ユースケースの呼び出し、エラーの表現変換を担い、業務ルールの判断はドメインに任せる
- CLIでもWebサーバーでも、起動の構造は同じ。入口を変えても内側の層は変わらない
