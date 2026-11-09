# 第4章-9 Domain Eventを設計する

Domain Event(ドメインイベント)は、**ドメインで起きた、業務上意味のある出来事**を表すオブジェクトです。Entity や Value Object と並ぶ、DDDの戦術的パターンの基本的な部品の1つです。

本書では、イベントを使って副作用を分ける方法(第7章-6)や、イベントを確実に届ける方法(第7章-14)を発展編で扱っています。この章ではその手前として、**イベントそのものをどう設計するか**に焦点を当てます。

## イベントは「起きた事実」

Domain Event は、すでに起きたことを表します。この点で、これから行いたいことを表すコマンドとは性質がまったく違います。

| | コマンド | Domain Event |
|---|---|---|
| 意味 | 〜してほしいという要求 | 〜したという事実 |
| 名前の時制 | 命令形(`CompleteTask`) | 過去形(`TaskCompleted`) |
| 失敗するか | 失敗しうる(ルール違反なら拒否される) | すでに起きたので、取り消せない |
| 受け取る側 | 1つの処理が受け付ける | 0個以上の購読者が反応する |

本書のサンプルでは、`TaskApplicationService::complete_task` がコマンドにあたり、その結果として `TaskEvent::TaskCompleted` が発行されます。

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEvent {
    TaskCreated { task_id: TaskId },
    TaskCompleted { task_id: TaskId },
    TaskArchived { task_id: TaskId },
}
```

## 名前の付け方

### 過去形で、業務の言葉を使う

イベント名は、「主語 + 過去形の動詞」で、ユビキタス言語を使って付けます(第3章-3)。

| 良い例 | 避けたい例 | 理由 |
|---|---|---|
| `TaskCompleted` | `CompleteTask` | 命令形はコマンドと区別できない |
| `TaskCompleted` | `TaskStatusUpdated` | 何が起きたのか、業務上の意味が消えている |
| `TaskArchived` | `TaskDeleted` | 業務では「削除」ではなく「アーカイブ」と呼んでいる |
| `TaskCompleted` | `TaskRowSaved` | 技術的な出来事であって、業務の出来事ではない |

### 汎用的なイベントを避ける

`TaskStatusChanged { from, to }` のような汎用的なイベントを1つ作ると、型の数は減ります。しかし、受け取る側は「`to` が `Completed` のときだけ通知する」といった判定を毎回書くことになります。

```rust
// 汎用的なイベント: 受け取る側で意味を解釈し直す必要がある
match event {
    TaskEvent::TaskStatusChanged { to: TaskStatus::Completed, task_id, .. } => notify(task_id),
    _ => {}
}

// 具体的なイベント: 名前を見れば何が起きたかがわかる
match event {
    TaskEvent::TaskCompleted { task_id } => notify(task_id),
    _ => {}
}
```

業務担当者が「タスクが完了したら」と言うなら、`TaskCompleted` という名前のイベントがあるべきです。イベントの粒度は、業務の会話で区別されている出来事の粒度に合わせます。

## どの情報を載せるか

イベントに載せる情報の量には、大きく2つの考え方があります。

### IDだけを載せる(薄いイベント)

```rust
TaskCompleted { task_id: TaskId }
```

受け取った側は、必要な情報をIDから改めて読み込みます。本書のサンプルはこの形です。

- イベントの形が安定しやすい(Task に属性が増えてもイベントは変わらない)
- 受け取った時点の最新の状態を読める
- その代わり、受け取る側が Repository を読む必要がある

### 必要な値まで載せる(厚いイベント)

```rust
TaskRenamed {
    task_id: TaskId,
    old_title: TaskTitle,
    new_title: TaskTitle,
}
```

- 受け取る側が Repository を読まずに処理できる
- 「変更前の値」のように、あとから読み直しても得られない情報を残せる
- その代わり、載せた値の形を変えるときに受け取る側への影響が大きい

### 判断の基準

迷ったら、次の基準で決めます。

- **その出来事でしか得られない情報は載せる**:名前変更の前の値、完了した時刻など。あとから読み直しても取り戻せません
- **最新の状態を読めば足りる情報は載せない**:タスクの現在のタイトルなど
- **受け取る側が別の Bounded Context なら、多めに載せる**:相手に自分の Repository を読ませることはできないからです(第7章-14の統合イベント)

ドメインイベントのフィールドには、`TaskId` や `TaskTitle` のようなドメインの型をそのまま使えます。同じ Bounded Context の中で使う限り、Value Object の不変条件がイベントの中でも守られます。外部へ送る統合イベントでは、安定した形式にするため `String` などの基本型に変換します。

## メタデータ: いつ、どのイベントが起きたか

実務では、イベントの中身とは別に、次のような情報が必要になることがよくあります。

- **イベントID**:同じイベントを二重に処理しないため(第7章-14の冪等性)
- **発生した時刻**:監査、並べ替え、時間のずれの調査のため

これらをすべてのバリアントに書くと重複するので、中身を包む「封筒」の型を用意するのが一般的です。

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEnvelope<E> {
    pub event_id: EventId,
    pub occurred_at: OccurredAt,
    pub payload: E,
}

pub type TaskEventEnvelope = EventEnvelope<TaskEvent>;
```

ジェネリクスにしておけば、`ProjectEvent` のような別の Aggregate のイベントにも同じ封筒を使えます。

時刻を封筒の中で `SystemTime::now()` のように直接取得すると、テストで値が毎回変わってしまいます。時刻は第4章-6の ID 生成と同じく、外から渡すか、`Clock` のようなトレイトで差し替えられるようにしておきます。

```rust
pub trait Clock {
    fn now(&self) -> OccurredAt;
}
```

本書のサンプルでは、学習のしやすさを優先して封筒を使わず、イベントの中身だけを発行しています。

## イベントは変更しない

イベントは過去の事実なので、作ったあとに変更されるべきではありません。Rustでは、次のようにしておくと自然に守れます。

- 変更用のメソッド(setter)を用意しない
- 発行先には `&TaskEvent`(共有参照)で渡す。`DomainEventPublisher::publish` もそうなっています
- 購読者が加工したい場合は、自分で新しい値を作る

enum のフィールドは外から読めますが、`&` で受け取っている限り書き換えることはできません。

## 誰がイベントを作るか

イベントを組み立てる場所には、3つの選択肢があります。

### 1. Application Service が作る(本書のサンプル)

```rust
task.complete()?;
let event = TaskEvent::TaskCompleted {
    task_id: task.id().clone(),
};
self.repository.save(task);
self.event_publisher.publish(&event)?;
```

- ドメインモデルがイベントを知らずに済み、最も単純
- その代わり、「完了したら `TaskCompleted` が起きる」という対応を Application Service が知っている。完了を行うユースケースが複数あると、イベントの作成を書き忘れるおそれがある

実際、本書のサンプルでは `TaskApplicationService::complete_task` と `BulkTaskCompletionService::complete_tasks` の両方で `TaskCompleted` を作っています。

### 2. Entity のメソッドがイベントを返す

```rust
#[must_use = "発生したイベントを発行してください"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEvent {
    // ...
}

impl Task {
    pub fn complete(&mut self) -> Result<TaskEvent, TaskError> {
        match self.status {
            TaskStatus::Open => {
                self.status = TaskStatus::Completed;
                Ok(TaskEvent::TaskCompleted {
                    task_id: self.id.clone(),
                })
            }
            TaskStatus::Completed => Err(TaskError::TaskAlreadyCompleted),
            TaskStatus::Archived => Err(TaskError::TaskNotOpen {
                current: self.status,
            }),
        }
    }
}
```

```rust
// Application Service
let event = task.complete()?;
self.repository.save(task);
self.event_publisher.publish(&event)?;
```

- 「完了したら `TaskCompleted` が起きる」というルールが `Task` の中に閉じる
- 戻り値として受け取るので、どのイベントが起きたかがシグネチャから読める
- `TaskEvent` 型に `#[must_use]` を付けておくと、イベントを受け取らずに捨てたときにコンパイラが警告する

`#[must_use]` は、関数ではなく型に付けるのがポイントです。関数に付けた場合、警告の対象は戻り値の `Result` になります。`task.complete()?;` と書くと `Result` は `?` で使われたことになるため、取り出された `TaskEvent` を捨てても警告が出ません。型に付けておけば、`task.complete()?;` の時点で「使われていない `TaskEvent`」として警告されます。

1回の操作で1つのイベントが起きる場合は、この形が Rust では最も素直です。

### 3. Aggregate がイベントを貯める

```rust
pub struct Task {
    // ...
    pending_events: Vec<TaskEvent>,
}

impl Task {
    pub fn take_events(&mut self) -> Vec<TaskEvent> {
        std::mem::take(&mut self.pending_events)
    }
}
```

- 1回の操作で複数のイベントが起きる場合や、複数の操作をしてから最後にまとめて発行する場合に向いている
- Repository が保存と同時にイベントを取り出す形にすると、Outbox と組み合わせやすい(第7章-14)
- その代わり、`Task` がイベントの一覧という状態を持つことになり、`PartialEq` で比較するときなどに注意が必要になる

### 比較

| 方式 | ルールの置き場所 | 書き忘れの防ぎやすさ | 向いている場面 |
|---|---|---|---|
| Application Service が作る | Application Service | 低い | イベントを使い始めたばかり、ユースケースが少ない |
| Entity が返す | Entity | 高い(型に `#[must_use]`) | 1回の操作で1つのイベント |
| Aggregate が貯める | Entity | 高い(Repository が取り出す) | 1回の操作で複数のイベント、Outbox と組み合わせる |

本書のサンプルは、ドメインモデルの変更を最小限にするため1の方式を採っています。完了を行うユースケースが2つに増えた今は、2の方式に移る時期だと言えます。

## 失敗はイベントにしない

ドメインルールに違反して操作が拒否された場合、それは「起きた事実」ではないので、通常はイベントにしません。`Err(TaskError::TaskAlreadyCompleted)` を返すだけです。

サンプルでも、`task.complete()?` が失敗すると `?` で抜けるため、イベントは作られません。テスト `application_service_does_not_publish_event_when_domain_rule_fails` でこれを確かめています。

ただし、拒否されたこと自体に業務上の意味がある場合は例外です。たとえば「不正なログインが試みられた」は、セキュリティ監視のためのイベントになりえます。「拒否されたことに、誰かが反応する必要があるか」で判断します。

## enum でまとめるか、型を分けるか

Rustでは、イベントの表し方にも選択肢があります。

### Aggregate ごとに enum でまとめる(本書のサンプル)

```rust
pub enum TaskEvent {
    TaskCreated { task_id: TaskId },
    TaskCompleted { task_id: TaskId },
    TaskArchived { task_id: TaskId },
}
```

- `match` が網羅性をチェックするので、新しいイベントを追加したときに、扱いを決めていない購読者がコンパイルエラーになる
- 1つの発行先(`DomainEventPublisher`)ですべてのイベントを扱える

### イベントごとに struct を分ける

```rust
pub struct TaskCompleted {
    pub task_id: TaskId,
}

pub struct TaskArchived {
    pub task_id: TaskId,
}
```

- 購読者が `fn handle(&self, event: &TaskCompleted)` のように、関心のあるイベントだけを型で受け取れる
- イベントの種類が多く、購読者がそれぞれ一部のイベントにしか関心がない場合に向いている

最初は enum でまとめ、イベントの種類と購読者が増えてきたら分割を検討するのが扱いやすい順番です。enum のバリアントに struct を持たせる `TaskEvent::Completed(TaskCompleted)` という形にすれば、両方の利点を得ることもできます。

## イベントでテストを書く

イベントは「何が起きたか」を表すので、テストの検証にも使えます。状態を1つずつ確かめる代わりに、「この操作をしたら、このイベントが起きた」と書けます。

```rust
#[test]
fn completing_open_task_emits_task_completed() {
    let mut task = Task::new(
        TaskId::new("task-1").unwrap(),
        TaskTitle::new("牛乳を買う").unwrap(),
    );

    let event = task.complete().unwrap();

    assert_eq!(
        event,
        TaskEvent::TaskCompleted {
            task_id: TaskId::new("task-1").unwrap()
        }
    );
}
```

これは上の2の方式(Entity が返す)を使った場合のテストです。サンプルのテスト `application_service_publishes_events_for_state_changes` では、1の方式のまま、記録用の `InMemoryDomainEventPublisher` を使って同じことを確かめています。

## よくある誤解

- 状態が変わるたびに、必ずイベントを発行すべきだと思いがち
- イベントには、受け取る側が必要としそうな情報をすべて載せるべきだと考えがち
- Domain Event を使うには、メッセージキューやイベントバスが必要だと思いがち

イベントは、業務上「その出来事に反応したい誰か」がいるときに意味を持ちます。誰も反応しない出来事にイベントを用意しても、型が増えるだけです。また、Domain Event は設計上の部品であって、配信の仕組みではありません。同じプロセスの中で関数を呼ぶだけでも、イベントとして設計する価値は十分にあります。

次章では、これまで見てきたドメインモデルをユースケースとして組み立てる Application Service を扱います。
