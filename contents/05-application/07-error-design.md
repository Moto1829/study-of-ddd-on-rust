# 第5章-7 層ごとのエラー設計

Rustでは失敗を `Result<T, E>` で表現するため、エラー型の設計がそのまま層の設計に直結します。エラー型を1つにまとめるのか、層ごとに分けるのか、どこで変換するのか。この章では、DDDの層構造に沿ったエラー設計を整理します。

## エラーにも種類がある

まず、アプリケーションで起きる失敗を性質で分けてみます。

| 種類 | 例 | 誰が知っているか |
|---|---|---|
| ドメインエラー | タイトルが空、完了済みタスクの再完了 | ドメイン層 |
| アプリケーションエラー | タスクが見つからない、IDが重複している | アプリケーション層 |
| インフラエラー | DB接続の失敗、外部APIのタイムアウト | インフラ層 |

この3つは、意味も対処方法もまったく違います。

- ドメインエラーは、ユーザーの入力や操作が業務ルールに反していることを示す。ユーザーに理由を伝えて、操作を直してもらう
- アプリケーションエラーは、ユースケースが成立しないことを示す。「見つかりません」のように状況を伝える
- インフラエラーは、システム側の問題を示す。ユーザーには詳細を見せず、ログに記録してリトライや運用対応を考える

これらを1つの `String` や汎用的なエラー型にまとめると、呼び出し側が「この失敗はユーザーに見せてよいのか」を判断できなくなります。

## ドメインエラー: 業務ルールの言葉で書く

本書の `TaskError` は、ドメインエラーの例です。

```rust
pub enum TaskError {
    EmptyTaskId,
    EmptyTaskTitle,
    TaskTitleTooLong { max: usize, actual: usize },
    TaskAlreadyCompleted,
    TaskAlreadyArchived,
    TaskNotOpen { current: TaskStatus },
    InvalidTaskStatus { value: String },
}
```

ドメインエラーを設計するときのポイントは次の通りです。

- **バリアント名をユビキタス言語で書く**: `TaskAlreadyCompleted` は業務担当者にも意味が通じる
- **判断に必要な情報を持たせる**: `TaskTitleTooLong { max, actual }` なら、画面で「100文字以内で入力してください(現在120文字)」と表示できる
- **技術的な詳細を含めない**: SQLエラーやHTTPステータスはドメインエラーに入れない
- **外部クレートに依存しない**: ドメイン層を純粋に保つ

## アプリケーションエラー: ドメインエラーを包む

Application Service は、ドメインエラーに加えて、ユースケース固有の失敗を扱います。本書の `TaskApplicationError` がこれにあたります。

```rust
pub enum TaskApplicationError {
    TaskAlreadyExists { task_id: String },
    TaskNotFound { task_id: String },
    Domain(TaskError),
}

impl From<TaskError> for TaskApplicationError {
    fn from(value: TaskError) -> Self {
        Self::Domain(value)
    }
}
```

`From<TaskError>` を実装しているため、Application Service の中では `?` 演算子でドメインエラーを自然に伝播できます。

```rust
let task_id = TaskId::new(task_id)?; // TaskError → TaskApplicationError に自動変換
task.rename(TaskTitle::new(new_title)?)?;
```

`TaskNotFound` がドメインエラーではなくアプリケーションエラーなのは、「タスクが見つからない」が業務ルール違反ではなく、ユースケースの前提が成り立たないことを表すからです。

## インフラエラーを上の層へどう伝えるか

本書の `TaskRepository` は `Result` を返していません。インメモリ実装では失敗しないからです。

```rust
pub trait TaskRepository {
    fn save(&mut self, task: Task);
    fn find_by_id(&self, task_id: &TaskId) -> Option<Task>;
}
```

しかし、DB実装に置き換えると、接続失敗などのインフラエラーが起こりえます。そこで Repository トレイトは次のように変わります。

```rust
pub trait TaskRepository {
    fn save(&mut self, task: &Task) -> Result<(), RepositoryError>;
    fn find_by_id(&self, task_id: &TaskId) -> Result<Option<Task>, RepositoryError>;
}
```

ここで問題になるのが、`RepositoryError` の中身です。

### やってはいけない例

```rust
// domain 層なのに sqlx に依存してしまっている
pub enum RepositoryError {
    Database(sqlx::Error),
}
```

ドメイン層で定義するトレイトが `sqlx::Error` を知ってしまうと、ドメイン層が特定のDBライブラリに依存します。DBを差し替えるとドメイン層まで変更が必要になり、依存方向の原則が崩れます。

### 推奨する形

ドメイン層では、技術に依存しない抽象的なエラーを定義します。

```rust
// domain 層
#[derive(Debug)]
pub enum RepositoryError {
    /// 一時的な障害(リトライで回復する可能性がある)
    Unavailable { reason: String },
    /// 楽観ロックの競合
    Conflict,
    /// 保存されたデータがドメインの制約を満たさない
    Corrupted { reason: String },
}
```

インフラ層では、ライブラリのエラーをこの型へ変換します。

```rust
// infrastructure 層
fn to_repository_error(error: sqlx::Error) -> RepositoryError {
    match error {
        sqlx::Error::Decode(_) | sqlx::Error::ColumnDecode { .. } => {
            RepositoryError::Corrupted {
                reason: error.to_string(),
            }
        }
        other => RepositoryError::Unavailable {
            reason: other.to_string(),
        },
    }
}
```

元のエラーの詳細を失いたくない場合は、`Box<dyn std::error::Error + Send + Sync>` で保持する方法もあります。

```rust
pub enum RepositoryError {
    Unavailable {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    // ...
}
```

こうすれば、ドメイン層は具体的なライブラリを知らないまま、ログ出力用に原因を辿れます。

## 変換の流れ

全体を通すと、エラーは次のように変換されていきます。

```text
sqlx::Error                    (インフラ層: ライブラリのエラー)
  ↓ map_err
RepositoryError                (ドメイン層で定義した抽象エラー)
  ↓ From
TaskApplicationError           (アプリケーション層: ユースケースの失敗)
  ↓ From / match
HTTPレスポンス、CLIのメッセージ  (プレゼンテーション層: 利用者への表現)
```

各層の境界で変換することで、それぞれの層は自分が理解できるエラーだけを扱えます。

## プレゼンテーション層での最終変換

最後に、アプリケーションエラーを利用者向けの表現に変換します。Web API なら HTTP ステータスコードへの対応付けがこれにあたります。

ここでは、`TaskApplicationError` に `Repository(RepositoryError)` バリアントを追加した前提で書いています。

```rust
// presentation 層
fn to_status_code(error: &TaskApplicationError) -> u16 {
    match error {
        TaskApplicationError::TaskNotFound { .. } => 404,
        TaskApplicationError::TaskAlreadyExists { .. } => 409,
        TaskApplicationError::Domain(_) => 422,
        TaskApplicationError::Repository(_) => 503,
    }
}
```

`match` が網羅性をチェックするため、新しいエラーのバリアントを追加したときに、対応付けを忘れるとコンパイルエラーになります。これはRustでエラーを enum で設計する大きな利点です。

## thiserror と anyhow の使い分け

Rustのエラー処理では、`thiserror` と `anyhow` という2つのクレートがよく使われます。DDDの層構造に当てはめると、使い分けの指針は次の通りです。

| クレート | 特徴 | 向いている層 |
|---|---|---|
| `thiserror` | enum のエラー型に `Display` や `Error` を簡潔に実装する | ドメイン層、アプリケーション層 |
| `anyhow` | エラーの種類を区別せず、文脈を付けて伝播する | プレゼンテーション層の最上位、`main` 関数 |

`thiserror` を使うと、本書の `TaskError` の `Display` 実装は次のように書けます。

```rust
#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("task title must not be empty")]
    EmptyTaskTitle,
    #[error("task title is too long: max={max}, actual={actual}")]
    TaskTitleTooLong { max: usize, actual: usize },
    // ...
}
```

`thiserror` は手書きの実装を生成するだけで、利用者側には依存が露出しません。そのため、ドメイン層の純粋さを重視する場合でも許容しやすいクレートです。本書のサンプルでは依存クレートをゼロにするため、手書きで実装しています。

一方、`anyhow::Error` はエラーの種類を消してしまうため、ドメイン層やアプリケーション層の戻り値に使うと、呼び出し側が `match` で分岐できなくなります。「どの失敗かを区別して扱う必要がない場所」に限定して使うのが安全です。

## よくある誤解

- エラー型は1つにまとめた方がシンプルだと思いがち
- インフラのエラーをそのままドメイン層まで伝えてもよいと考えがち
- `panic!` や `unwrap` でもテストが通れば問題ないと思いがち

ドメインルール違反は「起こりうる業務上の出来事」なので、`panic!` ではなく `Result` で表現します。`panic!` を使ってよいのは、プログラムのバグ(本来起こりえない状態)に限られます。

次章では、非同期処理を前提とした Repository トレイトの設計を見ます。
