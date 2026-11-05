# 第4章-6 Factoryで生成ルールを閉じ込める

Entity や Aggregate の生成は、単に struct を組み立てるだけの処理に見えます。しかし実務では、「どういう条件なら作ってよいか」「初期状態はどうあるべきか」「IDはどこで決めるか」といった生成のルールが意外に多く存在します。Factory は、この生成ルールを1箇所に閉じ込めるためのパターンです。

## 新規作成と復元は別物

まず押さえておきたいのは、「新しく作る」ことと「保存済みのものを復元する」ことは、まったく別の操作だということです。

本書の `Task` には、すでにこの2つが分かれています。

```rust
impl Task {
    /// 新規作成: 必ず Open 状態から始まる
    pub fn new(id: TaskId, title: TaskTitle) -> Self {
        Self {
            id,
            title,
            status: TaskStatus::Open,
        }
    }

    /// 復元: 永続化された状態をそのまま戻す
    pub fn restore(id: TaskId, title: TaskTitle, status: TaskStatus) -> Self {
        Self { id, title, status }
    }
}
```

`new` は業務上の「タスクを作成する」に対応し、状態は必ず `Open` です。`Completed` のタスクを新規作成することは業務上ありえないため、引数で状態を受け取りません。

一方の `restore` は、DBに保存されていた状態を再構成するためのものです。こちらは業務上の行為ではなく技術的な操作なので、任意の状態を受け取ります。

この2つを1つの関数にまとめると、「新規作成なのに `Completed` を渡せる」という抜け道ができてしまいます。

## Rustでの Factory の形

Rustにはコンストラクタという言語機能がないため、関連関数がそのまま Factory の役割を果たします。生成ルールの複雑さに応じて、次の3段階で考えると整理しやすくなります。

### 1. 関連関数(最も多い)

生成ルールがそのEntity自身で完結するなら、`Task::new` のような関連関数で十分です。ほとんどの場合はこれで足ります。

### 2. 生成のための専用メソッドを別のAggregateに置く

あるAggregateが別のAggregateを生み出すのが業務上自然な場合は、生み出す側にメソッドを置きます。

たとえば、将来「プロジェクト」という概念を追加し、タスクは必ずプロジェクトの中で作るというルールになったとします。

```rust
impl Project {
    pub fn create_task(&self, id: TaskId, title: TaskTitle) -> Result<Task, ProjectError> {
        if self.is_closed() {
            return Err(ProjectError::ProjectClosed);
        }

        Ok(Task::new_in_project(id, title, self.id().clone()))
    }
}
```

「クローズしたプロジェクトにはタスクを追加できない」というルールは `Task` ではなく `Project` が知っている情報です。このように、生成の可否を判断できるAggregateに Factory メソッドを置くと、ルールの置き場所が自然になります。

### 3. 独立した Factory 型

生成に外部の情報が必要な場合は、専用の型を作ります。その典型が ID 生成です。

## ID生成をどこが担うか

現在の `TaskApplicationService::create_task` は、IDを呼び出し側から受け取っています。

```rust
pub fn create_task(
    &mut self,
    task_id: impl Into<String>,
    title: impl Into<String>,
) -> Result<TaskOutput, TaskApplicationError>
```

学習用としては分かりやすい形ですが、実務ではIDはシステム側で採番することが一般的です。採番の方法(UUID、ULID、DBのシーケンスなど)は技術的な選択なので、ドメイン層ではトレイトとして抽象化します。

```rust
// domain 層
pub trait TaskIdGenerator {
    fn generate(&self) -> TaskId;
}
```

そして Factory がこのトレイトを使って `Task` を組み立てます。

```rust
// domain 層
pub struct TaskFactory<G: TaskIdGenerator> {
    id_generator: G,
}

impl<G: TaskIdGenerator> TaskFactory<G> {
    pub fn new(id_generator: G) -> Self {
        Self { id_generator }
    }

    pub fn create(&self, title: TaskTitle) -> Task {
        Task::new(self.id_generator.generate(), title)
    }
}
```

Infrastructure 層では、UUIDなどを使った実装を用意します。

```rust
// infrastructure 層(uuid クレートを使う例)
pub struct UuidTaskIdGenerator;

impl TaskIdGenerator for UuidTaskIdGenerator {
    fn generate(&self) -> TaskId {
        TaskId::new(uuid::Uuid::new_v4().to_string())
            .expect("UUID is never empty")
    }
}
```

テストでは、決まった値を返す実装に差し替えられます。

```rust
// テスト用
struct FixedTaskIdGenerator(&'static str);

impl TaskIdGenerator for FixedTaskIdGenerator {
    fn generate(&self) -> TaskId {
        TaskId::new(self.0).unwrap()
    }
}

#[test]
fn factory_creates_open_task() {
    let factory = TaskFactory::new(FixedTaskIdGenerator("task-1"));

    let task = factory.create(TaskTitle::new("牛乳を買う").unwrap());

    assert_eq!(task.id().value(), "task-1");
    assert_eq!(task.status(), TaskStatus::Open);
}
```

IDをシステムで採番すると、Application Service での重複チェック(`TaskAlreadyExists`)も不要にできます。UUIDのように衝突しない採番方式を選べば、重複そのものが起こりにくくなるからです。

## 生成時の不変条件

Factory を使う最大の利点は、「作られた瞬間から不変条件を満たしている」ことを保証できる点です。

Rustでは、フィールドを非公開にしておけば、モジュールの外から struct リテラルで直接組み立てることはできません。つまり、`Task` を作る経路は `new`、`restore`、そして Factory に限定されます。

```rust
// モジュールの外ではコンパイルエラーになる
let task = Task {
    id: TaskId::new("task-1").unwrap(),
    title: TaskTitle::new("牛乳を買う").unwrap(),
    status: TaskStatus::Completed,
};
```

この性質により、「不正な初期状態のTaskが存在する」という可能性をコンパイル時に排除できます。

## restore をどこまで公開するか

`restore` は任意の状態を受け取れるため、使い方を誤ると不変条件をすり抜けられます。たとえばApplication Serviceが `restore` を使って `Completed` のタスクを直接作ると、業務上の「完了する」という手順を飛ばせてしまいます。

これを防ぐ方法として、次のような選択肢があります。

- 名前で意図を明示する(`restore`、`reconstruct` など、業務操作ではないとわかる名前にする)
- ドキュメントコメントで「永続化からの復元専用」と明記する
- `pub(crate)` にしてクレート外から使えなくする
- 永続化モデルからの変換(`TryFrom<TaskRow>`)の中だけで使う

本書では学習のしやすさを優先して `pub` にしていますが、実務ではクレートの公開範囲と合わせて検討する価値があります。

## よくある誤解

- Factory は必ず `XxxFactory` という独立した型にしなければならないと思いがち
- `Default` トレイトを実装すれば生成ルールを表現できると考えがち
- 新規作成と復元を1つの関数で兼ねても問題ないと思いがち

特に `Default` には注意が必要です。`#[derive(Default)]` で空のタイトルを持つ `Task` が作れてしまうなら、それは不変条件の抜け道です。ドメインオブジェクトには、業務上の「既定値」が本当に存在する場合を除いて `Default` を実装しない方が安全です。

次章では、条件判断そのものをオブジェクトとして表現する Specification パターンを見ます。
