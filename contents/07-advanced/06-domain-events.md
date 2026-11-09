# 第7章-6 ドメインイベントによる副作用の分離

実務では、あるユースケースの完了後に別の処理を起こしたい場面が頻繁にあります。たとえば、タスク完了後に通知する、監査ログを残す、検索インデックスを更新する、といった処理です。

この章の役割は、イベントを使って副作用をどう切り離すかに絞ることです。イベント駆動全般の全体像は前章、CQRS導入の判断は次章に分けて扱います。

こうした副作用をそのまま `Task` や `TaskApplicationService` の中へ埋め込むと、責務が混ざります。そこで有効なのがドメインイベントです。

## なぜイベントにするのか

イベントを導入する理由は、「状態変更そのもの」と「状態変更のあとに起こしたいこと」を分けるためです。

- `Task` は完了したという事実を表現する
- 通知や監査は、その事実を受けて動く

この分離がないと、たとえばタスク完了と同時に通知APIを同期的に呼ぶ設計になりやすく、通知先が遅い、あるいは失敗しただけで本来独立しているはずのタスク完了処理まで引きずられます。イベントとして切り出しておけば、まず「完了した」という事実を確定させ、その後に通知や監査を進める形に整理しやすくなります。

この分離があると、後から副作用が増えてもドメインモデルを汚しにくくなります。

## タスク管理アプリの最小例

このタスク管理アプリでは、`TaskEvent` というイベント型を追加しています。これは「何が起きたか」を表す列挙型で、たとえば作成や完了の事実を表現するために使います。

```rust
pub enum TaskEvent {
    TaskCreated { task_id: TaskId },
    TaskCompleted { task_id: TaskId },
    TaskArchived { task_id: TaskId },
}
```

この段階では、Entityから自動的にイベントを貯めるところまでは進めていません。まずは「ドメイン上の事実を型で表せる」という最小構成に留めています。Aggregate にイベントを貯める形は、第7章-14の `Outboxでイベントを確実に届ける` で扱います。

## 発行先はInfrastructureへ閉じる

イベント発行の抽象として、`DomainEventPublisher` を用意しています。これは「イベントをどこかへ送る役割」だけを切り出した境界です。

```rust
pub trait DomainEventPublisher {
    fn publish(&self, event: &TaskEvent) -> Result<(), DomainEventPublishError>;
}
```

これにより、Application Serviceは「イベントを発行したい」という意図だけを持ち、実際にどこへ送るかは外側で決められます。

## ユースケースからの発行

サンプルコードの `TaskApplicationService` は、`DomainEventPublisher` をジェネリクスで受け取り、状態を変えるユースケースの最後でイベントを発行しています。

```rust
pub fn complete_task(
    &mut self,
    task_id: impl Into<String>,
) -> Result<TaskOutput, TaskApplicationError> {
    let task_id = TaskId::new(task_id)?;
    let mut task = self.repository.find_by_id(&task_id).ok_or_else(/* ... */)?;

    task.complete()?;
    let output = TaskOutput::from(&task);
    let event = TaskEvent::TaskCompleted {
        task_id: task.id().clone(),
    };
    self.repository.save(task);
    // 状態を確定させてから、起きた事実を発行する
    self.event_publisher.publish(&event)?;

    Ok(output)
}
```

押さえておきたい点は3つあります。

- **保存してから発行する**: イベントは「起きた事実」なので、状態が確定する前に発行してはいけません。`task.complete()` がドメインルールで失敗した場合は、`?` で抜けるためイベントは発行されません
- **発行の失敗は別のエラーとして返す**: 保存後に発行だけが失敗した場合は `TaskApplicationError::EventPublishFailed` を返します。このとき状態はすでに保存されています。「状態は保存できたが、イベントは届かなかった」というこの問題を確実に解決するには、第7章-14の Outbox が必要になります
- **一括処理ではトランザクション確定後に発行する**: `BulkTaskCompletionService` は、全件の完了がトランザクションとして確定してから `TaskCompleted` をまとめて発行します。途中で取り消された完了について通知が飛ぶことを防ぐためです

## 購読者と発行先の実装

`DomainEventPublisher` の実装は2つ用意しています。

| 実装 | 層 | 役割 |
|---|---|---|
| `TaskCompletedNotificationHandler<N>` | application | `TaskCompleted` を受け取り、`TaskCompletionNotifier` で通知する購読者。`main.rs` で使う |
| `InMemoryDomainEventPublisher` | infrastructure | 発行されたイベントを記録するだけ。テストで「どのイベントが発行されたか」を確かめる Spy として使う |

同じプロセスの中で同期的にイベントを届ける最小構成では、購読者そのものを発行先として Application Service に渡せます。購読者が複数になったら、受け取ったイベントを複数の購読者へ配る小さなディスパッチャーを `DomainEventPublisher` として実装すれば、Application Service を変えずに副作用を増やせます。

テストでは `InMemoryDomainEventPublisher` を渡して、ユースケースが正しいイベントを正しい順番で発行していることを確かめています。

```rust
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
            TaskEvent::TaskCreated { task_id: task_id.clone() },
            TaskEvent::TaskCompleted { task_id: task_id.clone() },
            TaskEvent::TaskArchived { task_id },
        ]
    );
}
```

## どういう順番で導入するべきか

学習段階では、次の順で進めると理解しやすいです。

1. まずはイベント型を定義する
2. 次に publisher ポートを定義する
3. そのあとで通知や監査ログの実装を追加する
4. 最後に、ユースケースからイベント発行を組み込む

最初から非同期配送やイベントバスまで入れる必要はありません。重要なのは、「副作用を後付けできる形にする」という考え方です。

## よくある誤解

- イベントを導入したら必ずメッセージキューが必要だと思いがち
- イベントを増やせばDDDらしくなると感じやすい
- 通知やログ出力も全部イベント化すべきだと考えがち

イベントは問題に対する手段であって、目的ではありません。副作用の分離に困っていない段階では、無理に入れなくても構いません。