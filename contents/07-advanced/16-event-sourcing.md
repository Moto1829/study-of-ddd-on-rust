# 第7章-16 Event Sourcingを検討する

本書のタスク管理アプリは、タスクの現在の状態(`id`、`title`、`status`)を保存しています。タスクを完了すると、`status` が `open` から `completed` に上書きされ、以前の状態は残りません。

Event Sourcing は、これとは異なる保存方法です。現在の状態ではなく、**状態を変えたイベントの履歴**を保存します。現在の状態は、イベントを最初から順番に再生して求めます。

## 状態の保存とイベントの保存

同じタスクの操作を、2つの方式で保存した場合を比べてみます。

### 状態を保存する方式(本書のサンプル)

```text
tasks テーブル
| id     | title      | status    |
|--------|------------|-----------|
| task-1 | 牛乳を2本買う | completed |
```

### イベントを保存する方式(Event Sourcing)

```text
events テーブル
| aggregate_id | version | event_type   | payload                 |
|--------------|---------|--------------|-------------------------|
| task-1       | 1       | TaskCreated  | {"title": "牛乳を買う"}    |
| task-1       | 2       | TaskRenamed  | {"title": "牛乳を2本買う"} |
| task-1       | 3       | TaskCompleted| {}                      |
```

Event Sourcing では、「最初は『牛乳を買う』という名前だったが、途中で変更され、その後完了した」という経緯がすべて残ります。

## Rustでの実装の骨格

Event Sourcing では、Aggregate は2種類のメソッドを持ちます。

1. **コマンドを処理するメソッド**: ルールを確認し、発生すべきイベントを返す。状態は変えない
2. **イベントを適用するメソッド**: イベントを受け取って状態を変える。ルールの確認はしない

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEvent {
    TaskCreated { task_id: TaskId, title: TaskTitle },
    TaskRenamed { title: TaskTitle },
    TaskCompleted,
    TaskArchived,
}

#[derive(Debug, Clone)]
pub struct Task {
    id: TaskId,
    title: TaskTitle,
    status: TaskStatus,
    version: u64,
}

impl Task {
    // --- コマンドの処理: ルールを確認し、イベントを返す ---

    pub fn create(task_id: TaskId, title: TaskTitle) -> TaskEvent {
        TaskEvent::TaskCreated { task_id, title }
    }

    pub fn rename(&self, title: TaskTitle) -> Result<TaskEvent, TaskError> {
        if self.status != TaskStatus::Open {
            return Err(TaskError::TaskNotOpen {
                current: self.status,
            });
        }
        Ok(TaskEvent::TaskRenamed { title })
    }

    pub fn complete(&self) -> Result<TaskEvent, TaskError> {
        match self.status {
            TaskStatus::Open => Ok(TaskEvent::TaskCompleted),
            TaskStatus::Completed => Err(TaskError::TaskAlreadyCompleted),
            TaskStatus::Archived => Err(TaskError::TaskNotOpen {
                current: self.status,
            }),
        }
    }

    // --- イベントの適用: 状態を変える ---

    pub fn apply(&mut self, event: &TaskEvent) {
        match event {
            TaskEvent::TaskCreated { .. } => {} // 生成は from_events で扱う
            TaskEvent::TaskRenamed { title } => self.title = title.clone(),
            TaskEvent::TaskCompleted => self.status = TaskStatus::Completed,
            TaskEvent::TaskArchived => self.status = TaskStatus::Archived,
        }
        self.version += 1;
    }

    // --- イベント列からの復元 ---

    pub fn from_events(events: &[TaskEvent]) -> Option<Self> {
        let (first, rest) = events.split_first()?;

        let TaskEvent::TaskCreated { task_id, title } = first else {
            return None;
        };

        let mut task = Self {
            id: task_id.clone(),
            title: title.clone(),
            status: TaskStatus::Open,
            version: 1,
        };

        for event in rest {
            task.apply(event);
        }

        Some(task)
    }
}
```

`rename` や `complete` が `&self` を受け取っている点に注目してください。コマンドの処理では状態を変えず、イベントを返すだけです。状態の変更は `apply` に一本化されます。

`apply` の `match` は網羅的なので、新しいイベントを追加すると、適用の処理を書き忘れたときにコンパイルエラーになります。

## Event Store

イベントを保存する Repository は、Event Store と呼ばれます。

```rust
pub trait TaskEventStore {
    fn load(&self, task_id: &TaskId) -> Result<Vec<TaskEvent>, EventStoreError>;

    fn append(
        &mut self,
        task_id: &TaskId,
        expected_version: u64,
        events: &[TaskEvent],
    ) -> Result<(), EventStoreError>;
}
```

`append` の `expected_version` は、第7章-13で扱った楽観ロックと同じ役割を果たします。読み込んだときのバージョンと、保存されている最新のバージョンが一致しなければ、競合として失敗させます。

Event Store は追記のみで、更新や削除は行いません。

## Application Service の流れ

```rust
pub fn complete_task(&mut self, task_id: TaskId) -> Result<(), TaskApplicationError> {
    let events = self.event_store.load(&task_id)?;
    let task = Task::from_events(&events).ok_or(TaskApplicationError::TaskNotFound)?;

    let new_event = task.complete()?;

    self.event_store
        .append(&task_id, task.version(), &[new_event])?;
    Ok(())
}
```

イベントを保存したら、それをそのまま他の処理に配信できます。状態とイベントを別々に保存する必要がないため、第7章-14で扱った二重書き込みの問題が、構造的に起きにくくなります。

## 読み取りモデル(投影)

Event Sourcing では、「未完了のタスクの一覧」のような検索を、イベントの履歴に対して直接行うのは非効率です。そこで、イベントを受け取って検索用のテーブルを更新する**投影(Projection)** を作ります。

```rust
pub fn project(&mut self, task_id: &TaskId, event: &TaskEvent) {
    match event {
        TaskEvent::TaskCreated { title, .. } => {
            self.summaries.insert(task_id.clone(), TaskSummary::open(task_id, title));
        }
        TaskEvent::TaskRenamed { title } => {
            if let Some(summary) = self.summaries.get_mut(task_id) {
                summary.title = title.value().to_owned();
            }
        }
        TaskEvent::TaskCompleted => { /* status を completed に更新 */ }
        TaskEvent::TaskArchived => { /* status を archived に更新 */ }
    }
}
```

これは CQRS そのものです。Event Sourcing を採用すると、ほぼ必然的に CQRS も採用することになります。第7章-7の `CQRSへ進む判断基準` も合わせて読んでください。

## Event Sourcing の利点

- **完全な履歴が残る**: いつ、何が、どう変わったかをすべて追跡できる。監査の要件がある業務では大きな価値がある
- **過去の任意の時点の状態を再現できる**: イベントを途中まで再生すれば、その時点の状態が得られる
- **後から新しい読み取りモデルを作れる**: 過去のイベントを再生すれば、新しい投影を最初から構築できる
- **業務の出来事がそのままデータになる**: ユビキタス言語のイベント名がそのまま保存される

## Event Sourcing のコスト

一方で、導入のコストは小さくありません。

### イベントの形式を変えにくい

イベントは過去の事実なので、一度保存したら書き換えません。イベントの形式を変えたい場合、過去の形式のイベントも読めるようにしておく必要があります。

```rust
pub enum StoredTaskEvent {
    TaskRenamedV1 { title: String },
    TaskRenamedV2 { title: String, renamed_by: String },
}
```

古い形式を新しい形式へ変換する処理(アップキャスト)を、ずっと保守し続けることになります。

### 再生のコスト

イベントが何千件もある Aggregate では、毎回すべてを再生すると遅くなります。一定の間隔で状態のスナップショットを保存し、そこから先のイベントだけを再生する工夫が必要です。

### 結果整合性

投影の更新はイベントの保存より遅れるため、更新直後に一覧を表示すると、変更が反映されていないことがあります。

### 学習コスト

チーム全員が、状態ではなくイベントで考える方式に慣れる必要があります。

## 導入を検討すべき場面

Event Sourcing は、次のような場面で検討する価値があります。

- 変更の履歴そのものが業務上重要である(会計、医療記録、法的な監査など)
- 「なぜこの状態になったのか」を説明する必要がある
- 過去の時点の状態を再現する必要がある
- 同じデータから、多様な読み取りモデルを作りたい

逆に、次のような場合は、状態を保存する方式の方が適しています。

- 現在の状態だけがわかれば十分である
- 履歴は、監査ログテーブルを別に用意すれば足りる
- チームに Event Sourcing の経験がない

本書のタスク管理アプリは、現在の状態がわかれば十分なので、状態を保存する方式を採用しています。

## よくある誤解

- ドメインイベントを使っているなら、Event Sourcing も採用すべきだと思いがち
- Event Sourcing はシステム全体に適用するものだと考えがち
- イベントの履歴は監査ログの代わりになるので、常に便利だと思いがち

ドメインイベントと Event Sourcing は別のものです。ドメインイベントは副作用の分離に使え、状態を保存する方式とも組み合わせられます。また、Event Sourcing は Bounded Context や Aggregate ごとに選べます。履歴が重要な一部の Aggregate だけに適用するのが現実的です。

次章では、本書で採用している層構造と、ヘキサゴナルアーキテクチャやクリーンアーキテクチャとの関係を整理します。
