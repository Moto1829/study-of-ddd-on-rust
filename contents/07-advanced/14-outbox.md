# 第7章-14 Outboxでイベントを確実に届ける

第7章-6の `ドメインイベントによる副作用の分離` では、イベントを使って副作用を切り離す考え方を扱いました。しかし、イベントを実際に外部へ送る段階になると、新しい問題が生まれます。「状態は保存できたのに、イベントの送信に失敗した」という問題です。この章では、その解決策である Outbox パターンを扱います。

## 二重書き込みの問題

タスクを完了したら、`TaskCompleted` イベントをメッセージブローカー(Kafka、RabbitMQ、SQS など)へ送るとします。素直に書くと、次のようになります。

```rust
pub fn complete_task(&mut self, task_id: TaskId) -> Result<(), TaskApplicationError> {
    let mut task = self.find_task(&task_id)?;
    task.complete()?;

    self.repository.save(&task)?;                                  // 1. DBに保存
    self.publisher.publish(&TaskEvent::TaskCompleted { task_id })?; // 2. ブローカーへ送信

    Ok(())
}
```

DBとブローカーという2つの異なるシステムに書き込んでいるため、次のような失敗が起こりえます。

| ケース | 結果 |
|---|---|
| 1が成功し、2が失敗 | タスクは完了したが、通知が送られない |
| 順番を入れ替え、2が成功し、1が失敗 | 通知は送られたが、タスクは完了していない |
| 1が成功し、2の直前にプロセスが落ちる | タスクは完了したが、イベントは永久に失われる |

DBとブローカーをまたぐトランザクションは、一般的には使えません。これを**二重書き込みの問題(Dual Write Problem)** と呼びます。

## Outbox パターン

Outbox パターンの考え方はシンプルです。**イベントを、状態と同じDBに、同じトランザクションで保存する**。そして、別のプロセスがそのイベントを読み出してブローカーへ送ります。

```text
┌─────────────── 1つのトランザクション ───────────────┐
│  tasks テーブルを更新                                │
│  outbox テーブルにイベントを追加                      │
└──────────────────────────────────────────────────────┘
                        ↓
          [リレー] outbox を読み出してブローカーへ送信
                        ↓
          送信に成功したら outbox の行に送信済みの印を付ける
```

状態とイベントが同じトランザクションで保存されるため、「状態は保存されたがイベントがない」「イベントはあるが状態がない」ということが起こりません。

## Outbox テーブル

Outbox テーブルは、たとえば次のような形にします。

```sql
CREATE TABLE outbox (
    id           UUID PRIMARY KEY,
    aggregate_id TEXT        NOT NULL,
    event_type   TEXT        NOT NULL,
    payload      JSONB       NOT NULL,
    occurred_at  TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ
);
```

## Aggregate にイベントを貯める

Outbox を使うには、状態の変更とイベントを一緒に Repository へ渡す必要があります。そのための方法の1つが、Aggregate 自身に発生したイベントを貯めておくことです。

```rust
pub struct Task {
    id: TaskId,
    title: TaskTitle,
    status: TaskStatus,
    pending_events: Vec<TaskEvent>,
}

impl Task {
    pub fn complete(&mut self) -> Result<(), TaskError> {
        match self.status {
            TaskStatus::Open => {
                self.status = TaskStatus::Completed;
                self.pending_events.push(TaskEvent::TaskCompleted {
                    task_id: self.id.clone(),
                });
                Ok(())
            }
            TaskStatus::Completed => Err(TaskError::TaskAlreadyCompleted),
            TaskStatus::Archived => Err(TaskError::TaskNotOpen {
                current: self.status,
            }),
        }
    }

    /// 貯まったイベントを取り出す。取り出したあとは空になる
    pub fn take_events(&mut self) -> Vec<TaskEvent> {
        std::mem::take(&mut self.pending_events)
    }
}
```

`std::mem::take` を使うと、`Vec` の中身を取り出して空の `Vec` に置き換えられます。同じイベントを2回取り出すことがなくなります。

この形にすると、「完了したらイベントを発行する」というルールが `Task` の中に閉じ込められます。Application Service がイベントの発行を忘れることもありません。

## Repository で状態とイベントを一緒に保存する

Repository の実装では、同じトランザクションで状態とイベントを書き込みます。

```rust
impl TaskRepository for PostgresTaskRepository {
    async fn save(&self, task: &mut Task) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(to_repository_error)?;

        sqlx::query("UPDATE tasks SET title = $1, status = $2 WHERE id = $3")
            .bind(task.title().value())
            .bind(task.status().as_str())
            .bind(task.id().value())
            .execute(&mut *tx)
            .await
            .map_err(to_repository_error)?;

        for event in task.take_events() {
            let record = OutboxRecord::from(&event);
            sqlx::query(
                "INSERT INTO outbox (id, aggregate_id, event_type, payload, occurred_at)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(record.id)
            .bind(record.aggregate_id)
            .bind(record.event_type)
            .bind(record.payload)
            .bind(record.occurred_at)
            .execute(&mut *tx)
            .await
            .map_err(to_repository_error)?;
        }

        tx.commit().await.map_err(to_repository_error)?;
        Ok(())
    }
}
```

## リレー: Outbox からブローカーへ送る

リレーは、Outbox の未送信イベントを定期的に読み出して、ブローカーへ送る処理です。アプリケーションとは別のタスクやプロセスとして動かします。

```rust
pub async fn relay_outbox<P: MessagePublisher>(
    pool: &PgPool,
    publisher: &P,
) -> Result<(), RelayError> {
    let records = sqlx::query_as::<_, OutboxRecord>(
        "SELECT * FROM outbox WHERE published_at IS NULL ORDER BY occurred_at LIMIT 100",
    )
    .fetch_all(pool)
    .await?;

    for record in records {
        publisher.publish(&record).await?;

        sqlx::query("UPDATE outbox SET published_at = now() WHERE id = $1")
            .bind(record.id)
            .execute(pool)
            .await?;
    }

    Ok(())
}
```

## 少なくとも1回の配信と冪等性

リレーにも、失敗のタイミングがあります。ブローカーへの送信には成功したが、`published_at` を更新する前にプロセスが落ちた場合、次回のリレーで同じイベントがもう一度送られます。

つまり、Outbox パターンが保証するのは**少なくとも1回(at-least-once)** の配信です。同じイベントが2回以上届く可能性があります。

そのため、イベントを受け取る側は**冪等(idempotent)** に作る必要があります。同じイベントを何度処理しても、結果が変わらないようにするということです。

冪等にする方法は、主に2つあります。

### 1. 処理済みのイベントIDを記録する

```rust
pub async fn handle(&self, event: &IntegrationEvent) -> Result<(), HandlerError> {
    if self.processed_events.contains(&event.id).await? {
        return Ok(()); // すでに処理済みなら何もしない
    }

    self.send_notification(event).await?;
    self.processed_events.insert(&event.id).await?;
    Ok(())
}
```

### 2. 操作自体を冪等にする

前章のリトライの例のように、「すでに目的の状態なら何もしない」形にすれば、同じイベントを何度受け取っても問題ありません。

```rust
if task.status() == TaskStatus::Archived {
    return Ok(());
}
```

ドメインの操作が冪等になるように設計できれば、処理済みイベントの記録は不要になります。

## ドメインイベントと統合イベント

Outbox からブローカーへ送るイベントは、ドメインイベントそのものとは分けて考えた方が安全です。

| 種類 | 範囲 | 目的 | 変更のしやすさ |
|---|---|---|---|
| ドメインイベント | 1つの Bounded Context の内部 | 内部の副作用を分離する | 自由に変更できる |
| 統合イベント(Integration Event) | 複数の Bounded Context の間 | 他のコンテキストへ事実を伝える | 受け取る側との合意が必要 |

ドメインイベント `TaskEvent` は、内部の型(`TaskId` など)をそのまま持っています。これを外部へそのまま公開すると、内部のモデルを変えるたびに、受け取る側も変更が必要になります。

そこで、外部へ送るときは、安定した形式の統合イベントに変換します。

```rust
// 外部へ公開する統合イベント(Published Language)
#[derive(serde::Serialize)]
pub struct TaskCompletedV1 {
    pub event_id: String,
    pub task_id: String,
    pub completed_at: String,
}
```

型名に `V1` を付けているのは、将来形式を変えるときに、古い形式と新しい形式を並行して送れるようにするためです。これは第7章-10で扱った Published Language の考え方そのものです。

## Outbox を導入するタイミング

Outbox は確実性を高めますが、テーブル、リレー、冪等性の仕組みなど、追加するものが多くあります。次のような場合に導入を検討します。

- イベントが失われると、業務上の問題が起きる(決済、在庫、契約など)
- 他の Bounded Context やシステムが、イベントをもとに状態を変える
- メッセージブローカーを使っている

逆に、イベントの受け取り手が同じプロセス内にあり、失われてもログ出力が欠けるだけなら、第7章-6の最小構成で十分です。

## よくある誤解

- メッセージブローカーを使えば、イベントは確実に届くと思いがち
- Outbox を使えば、イベントはちょうど1回だけ届くと考えがち
- ドメインイベントをそのまま外部に公開してもよいと思いがち

ブローカーはイベントを受け取った後の配信は保証してくれますが、アプリケーションからブローカーに届くまでは保証してくれません。そこを埋めるのが Outbox です。そして、Outbox を使っても重複配信は起こりうるので、受け取る側の冪等性とセットで考えます。

次章では、複数の Aggregate やサービスにまたがる長い処理を、イベントでつないで進める Saga を見ます。
