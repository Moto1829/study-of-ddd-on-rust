# 第5章-8 非同期処理とRepositoryトレイト

本書のサンプルは同期処理だけで書かれています。しかし実務のRustでWebアプリケーションを作るなら、`tokio` などの非同期ランタイムと、`sqlx` のような非同期DBクライアントを使うことがほとんどです。この章では、非同期処理をDDDの層構造にどう組み込むかを扱います。

第7章-1の `RustでDDDを実践する際の悩みどころ` では、「非同期性はI/O境界に閉じ込める」という方針を示しました。ここではその方針を、具体的なトレイト設計に落とし込みます。

## どこが非同期になるのか

非同期になるのは、I/Oを伴う処理です。

| 層 | 非同期か | 理由 |
|---|---|---|
| ドメイン層の Entity / Value Object | 同期 | メモリ上の計算だけで完結する |
| ドメイン層の Repository トレイト | 非同期 | 実装がDBアクセスを行う |
| Application Service | 非同期 | Repository を呼ぶため |
| Infrastructure 層の実装 | 非同期 | DBやHTTPのI/Oを行う |

ここで大事なのは、**Entity と Value Object は同期のまま保つ**ことです。`Task::complete` や `TaskTitle::new` がI/Oを行うことはないので、`async` にする理由がありません。

```rust
// Application Service: 非同期
pub async fn complete_task(&self, task_id: TaskId) -> Result<TaskOutput, TaskApplicationError> {
    let mut task = self
        .repository
        .find_by_id(&task_id)
        .await? // I/O: 非同期
        .ok_or(TaskApplicationError::TaskNotFound)?;

    task.complete()?; // ドメインロジック: 同期

    self.repository.save(&task).await?; // I/O: 非同期
    Ok(TaskOutput::from(&task))
}
```

この形なら、ドメインロジックのテストは非同期ランタイムなしで書けます。

## トレイトで async fn を書く

Rust 1.75 以降では、トレイトに直接 `async fn` を書けます。

```rust
pub trait TaskRepository {
    async fn find_by_id(&self, task_id: &TaskId) -> Result<Option<Task>, RepositoryError>;
    async fn save(&self, task: &Task) -> Result<(), RepositoryError>;
}
```

ただし、この書き方には2つの制約があります。

### 制約1: Send 境界を指定できない

`tokio` のマルチスレッドランタイムで `tokio::spawn` を使うと、Future がスレッド間を移動するため `Send` である必要があります。ところが、トレイトの `async fn` が返す Future が `Send` かどうかは、呼び出し側から指定できません。

その結果、Webフレームワークのハンドラなどで次のようなエラーに遭遇します。

```text
error: future cannot be sent between threads safely
```

これを避けるには、`async fn` の代わりに、戻り値の型を明示的に書きます。

```rust
use std::future::Future;

pub trait TaskRepository: Send + Sync {
    fn find_by_id(
        &self,
        task_id: &TaskId,
    ) -> impl Future<Output = Result<Option<Task>, RepositoryError>> + Send;

    fn save(&self, task: &Task) -> impl Future<Output = Result<(), RepositoryError>> + Send;
}
```

実装側は、通常の `async fn` として書けます。

```rust
impl TaskRepository for PostgresTaskRepository {
    async fn find_by_id(&self, task_id: &TaskId) -> Result<Option<Task>, RepositoryError> {
        // sqlx を使った実装
    }

    async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
        // sqlx を使った実装
    }
}
```

### 制約2: dyn Trait として使えない

`async fn` や `impl Future` を返すメソッドを持つトレイトは、`dyn TaskRepository` として使えません。そのため、`Box<dyn TaskRepository>` や `Arc<dyn TaskRepository>` で実装を差し替える設計にはできません。

## ジェネリクスで注入する

本書のサンプルは、もともとジェネリクスで Repository を注入しています。

```rust
pub struct TaskApplicationService<R: TaskRepository> {
    repository: R,
}
```

この設計なら、`dyn` が使えないという制約は問題になりません。非同期化しても、そのままの構造で進められます。

```rust
pub struct TaskApplicationService<R: TaskRepository> {
    repository: R,
}

impl<R: TaskRepository> TaskApplicationService<R> {
    pub async fn get_task(&self, task_id: TaskId) -> Result<TaskOutput, TaskApplicationError> {
        let task = self
            .repository
            .find_by_id(&task_id)
            .await?
            .ok_or(TaskApplicationError::TaskNotFound)?;

        Ok(TaskOutput::from(&task))
    }
}
```

ジェネリクスはコンパイル時に具体型へ展開されるため、動的ディスパッチのコストもかかりません。

## どうしても dyn が必要な場合

設定ファイルによって実行時に実装を切り替えたい場合など、`dyn` が必要になることもあります。その場合は、`async-trait` クレートを使う方法があります。

```rust
#[async_trait::async_trait]
pub trait TaskRepository: Send + Sync {
    async fn find_by_id(&self, task_id: &TaskId) -> Result<Option<Task>, RepositoryError>;
    async fn save(&self, task: &Task) -> Result<(), RepositoryError>;
}
```

`async-trait` は、Future を `Pin<Box<dyn Future + Send>>` に変換するマクロです。`dyn TaskRepository` として使えるようになる代わりに、呼び出しごとにヒープ確保が発生します。DBアクセスのコストに比べれば無視できることがほとんどですが、ドメイン層が外部クレートに依存することになる点は意識しておきましょう。

| 方式 | dyn 対応 | Send の指定 | 外部クレート | コスト |
|---|---|---|---|---|
| `async fn` | 不可 | 不可 | 不要 | なし |
| `impl Future + Send` | 不可 | 可能 | 不要 | なし |
| `async-trait` | 可能 | 可能(既定で Send) | 必要 | ヒープ確保 |

迷ったら、ジェネリクス + `impl Future + Send` から始めるのがおすすめです。

## &mut self ではなく &self にする

同期版の `TaskRepository::save` は `&mut self` を受け取っていました。

```rust
fn save(&mut self, task: Task);
```

非同期版では `&self` に変えています。理由は、実際のDB実装では内部に接続プール(`sqlx::PgPool` など)を持ち、プール自体がスレッド安全な共有を前提に作られているからです。`&mut self` を要求すると、複数のリクエストから同時に Repository を使えなくなります。

インメモリ実装で `&self` のまま状態を変更したい場合は、内部可変性を使います。本書の `InMemoryTaskRepository` がすでに `Arc<RwLock<HashMap<...>>>` を持っているのは、この移行を見越した形です。非同期版では、`tokio::sync::RwLock` か、ロックを `.await` をまたいで保持しない前提で `std::sync::RwLock` を使います。

## 非同期をドメイン層へ持ち込まないための工夫

ドメインロジックがI/Oの結果を必要とする場合でも、ドメイン層は同期のまま保てます。必要なデータを先に Application Service で取得してから、ドメインに渡せばよいのです。

第4章-5の `OpenTaskLimitPolicy` は、まさにこの形になっています。

```rust
// Application Service
let open_count = self.repository.count_open_tasks(&user_id).await?; // 非同期でデータを取得
self.policy.ensure_can_create(&user_id, open_count)?;               // 同期で判断
```

Domain Service に Repository を渡して内部で `await` させる設計も可能ですが、そうするとドメインのルール判定が非同期になり、テストにもランタイムが必要になります。「取得は Application Service、判断はドメイン」と分けると、ドメイン層を同期のまま保てます。

## よくある誤解

- 非同期アプリケーションなら、すべての関数を `async` にすべきだと思いがち
- `async-trait` を使わないと非同期トレイトは書けないと考えがち
- `dyn` で注入するのが依存性注入の標準的な方法だと思いがち

Rust 1.75 以降では、多くの場合 `async-trait` は不要です。また、Rustではジェネリクスによる注入が一般的で、コンパイル時の型検査と実行時の性能の両方で有利です。

次章では、「誰がその操作を実行してよいか」という認可の判断を、どの層に置くべきかを見ます。
