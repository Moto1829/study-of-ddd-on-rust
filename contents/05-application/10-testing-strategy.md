# 第5章-10 層ごとのテスト戦略

DDDで層を分ける大きな利点の1つは、テストしやすくなることです。ドメイン層は外部に依存しないので高速にテストでき、アプリケーション層は Repository を差し替えて検証できます。この章では、層ごとに何をどうテストするかを整理します。

第6章-4の `永続化とテストを実装する` ではサンプルアプリのテスト観点を扱いました。ここでは、より一般的なテストの設計方針を扱います。

## 層ごとのテストの役割

| 層 | テストの対象 | 外部依存 | 速度 | 数 |
|---|---|---|---|---|
| ドメイン層 | Value Object の検証、Entity の状態遷移、Domain Service のルール | なし | 非常に速い | 多い |
| アプリケーション層 | ユースケースの流れ、エラーの変換、副作用の呼び出し | テストダブル | 速い | 中程度 |
| インフラ層 | DBとの変換、SQLの正しさ、外部APIとの通信 | 実際のDBなど | 遅い | 少ない |
| 全体 | 主要なシナリオが端から端まで動くこと | すべて | 最も遅い | ごく少ない |

業務ルールの大半をドメイン層に置いておけば、最も速く安定したテストで、最も重要なルールを検証できます。これは、ドメインモデルを厚くすることの実利的な効果です。

## ドメイン層: 業務ルールを直接テストする

ドメイン層のテストは、外部依存なしで書けます。

```rust
#[test]
fn completed_task_cannot_be_renamed() {
    let mut task = Task::new(
        TaskId::new("task-1").unwrap(),
        TaskTitle::new("牛乳を買う").unwrap(),
    );
    task.complete().unwrap();

    let result = task.rename(TaskTitle::new("卵を買う").unwrap());

    assert_eq!(
        result,
        Err(TaskError::TaskNotOpen {
            current: TaskStatus::Completed
        })
    );
}
```

テストを書くときのポイントは次の通りです。

- **テスト名を業務の言葉で書く**: `completed_task_cannot_be_renamed` は業務ルールそのもの
- **エラーの種類まで検証する**: `is_err()` だけでなく、どのエラーかを確かめる
- **1つのテストで1つのルールを検証する**

### 状態遷移を網羅する

状態遷移のテストでは、「すべての状態 × すべての操作」の組み合わせを表にすると、抜けがないか確認しやすくなります。

| 現在の状態 | rename | complete | archive |
|---|---|---|---|
| Open | 成功 | 成功 → Completed | 成功 → Archived |
| Completed | `TaskNotOpen` | `TaskAlreadyCompleted` | 成功 → Archived |
| Archived | `TaskNotOpen` | `TaskNotOpen` | `TaskAlreadyArchived` |

この9マスがすべてテストされていれば、状態遷移のルールは網羅できています。

## プロパティベーステストで Value Object を検証する

Value Object のテストでは、具体的な値をいくつか試すだけでは境界の抜けを見落としがちです。`proptest` クレートを使うと、ランダムな入力を大量に生成して性質を検証できます。

```rust
use proptest::prelude::*;

proptest! {
    // 1〜100文字の空白を含まない文字列は、必ず TaskTitle として受け入れられる
    #[test]
    fn valid_titles_are_accepted(title in "[^\\s]{1,100}") {
        prop_assert!(TaskTitle::new(title).is_ok());
    }

    // 101文字以上の文字列は、必ず拒否される
    #[test]
    fn too_long_titles_are_rejected(title in "[^\\s]{101,200}") {
        prop_assert!(TaskTitle::new(title).is_err());
    }

    // 受け入れられたタイトルは、前後に空白を含まない
    #[test]
    fn accepted_titles_are_trimmed(title in ".{0,120}") {
        if let Ok(task_title) = TaskTitle::new(title) {
            let value = task_title.value();
            prop_assert_eq!(value, value.trim());
        }
    }
}
```

プロパティベーステストは、「どんな入力でも成り立つべき性質」を書くテストです。Value Object の不変条件はまさにそうした性質なので、相性が良い組み合わせです。

特に、マルチバイト文字や空白の扱いのように、手で書いたテストでは見落としやすいケースを見つけてくれます。たとえば `TaskTitle` は `chars().count()` で文字数を数えていますが、もし `len()`(バイト数)で数えていたら、日本語のタイトルで上限の判定がずれるバグをプロパティベーステストが検出できます。

## アプリケーション層: テストダブルで差し替える

Application Service のテストでは、Repository などの依存をテスト用の実装に差し替えます。テスト用の実装にはいくつかの種類があります。

| 種類 | 振る舞い | 用途 |
|---|---|---|
| Fake | 本物と同じように動く簡易実装 | Repository のインメモリ実装 |
| Stub | 決まった値を返すだけ | ID生成、現在時刻 |
| Spy | 呼び出されたことを記録する | 通知やイベント発行の確認 |
| Mock | 期待する呼び出しを事前に設定し、違反したら失敗する | 呼び出し順序が重要な場合 |

### Fake: InMemoryTaskRepository

本書の `InMemoryTaskRepository` は Fake です。本物のDBと同じように保存と取得ができるため、ユースケースの流れを自然に検証できます。

`TaskApplicationService` は Repository のほかにイベントの発行先や作成上限ポリシーも受け取るため、テストでは組み立てをヘルパー関数にまとめています。これもテスト用の小さなコンポジションルートです。

```rust
fn new_service_with_limit(
    repository: InMemoryTaskRepository,
    max_open_tasks: usize,
) -> (TestService, InMemoryDomainEventPublisher) {
    let publisher = InMemoryDomainEventPublisher::new();
    let service = TaskApplicationService::new(
        repository,
        publisher.clone(),
        OpenTaskLimitPolicy::new(max_open_tasks),
        UserId::new("user-1").unwrap(),
    );
    (service, publisher)
}

fn new_service(repository: InMemoryTaskRepository) -> TestService {
    new_service_with_limit(repository, 100).0
}
```

```rust
#[test]
fn application_service_completes_existing_task() {
    let mut service = new_service(InMemoryTaskRepository::new());
    service.create_task("task-1", "牛乳を買う").unwrap();

    let output = service.complete_task("task-1").unwrap();

    assert_eq!(output.status, TaskStatusDto::Completed);
}
```

### Spy: 副作用が起きたことを確認する

イベント発行や通知のように、戻り値ではなく「呼ばれたこと」を確認したい場合は Spy を使います。本書の `InMemoryDomainEventPublisher` は、発行されたイベントを記録する Spy として使えます。

```rust
#[derive(Default)]
struct SpyNotifier {
    notified: RefCell<Vec<String>>,
}

impl TaskCompletionNotifier for SpyNotifier {
    fn notify_task_completed(&self, task_id: &str) -> Result<(), NotificationError> {
        self.notified.borrow_mut().push(task_id.to_owned());
        Ok(())
    }
}
```

### モックライブラリは必要か

Rustには `mockall` のようなモックライブラリもあります。しかし、DDDの層構造に沿って設計していれば、多くの場合は手書きの Fake や Spy で十分です。

- トレイトのメソッドが少なければ、手書きの方が読みやすい
- Fake は本物に近い振る舞いをするため、実装の詳細に依存しないテストになる
- Mock は「どのメソッドが何回呼ばれたか」を検証するため、内部実装の変更でテストが壊れやすい

Repository のトレイトが大きくなりすぎて手書きの Fake が辛くなったら、それは Repository の責務が大きすぎるサインかもしれません。

## インフラ層: 実際の技術を使ってテストする

インフラ層のテストは、実際のDBや外部サービスを使わないと意味がありません。SQLの誤りや型の不一致は、本物のDBでしか見つからないからです。

最低限テストしておきたいのは、次の2点です。

### 1. 往復変換

ドメインモデルを保存して読み戻したとき、同じものが返ってくることを確かめます。

```rust
#[test]
fn task_row_round_trip_preserves_status() {
    let mut task = Task::new(
        TaskId::new("task-1").unwrap(),
        TaskTitle::new("牛乳を買う").unwrap(),
    );
    task.complete().unwrap();

    let row = TaskRow::from(&task);
    let restored = Task::try_from(row).unwrap();

    assert_eq!(restored, task);
}
```

本書のサンプルでは `TaskRow` との変換でこれを確認しています。DB実装では、実際に `INSERT` してから `SELECT` で読み戻すテストになります。

### 2. 不正なデータの拒否

DBに不正なデータが入っていた場合に、ドメインモデルへの変換が失敗することを確かめます。

```rust
#[test]
fn task_row_rejects_invalid_status() {
    let row = TaskRow {
        id: "task-1".to_owned(),
        title: "牛乳を買う".to_owned(),
        status: "unknown".to_owned(),
    };

    assert!(Task::try_from(row).is_err());
}
```

DBを使うテストでは、Docker で起動したDBに接続する、`sqlx` のテスト用マクロでテストごとに独立したDBを用意する、などの方法があります。これらは遅いので、`#[ignore]` を付けて通常の `cargo test` から外し、CIでだけ実行する運用もよく使われます。

## テストが書きにくいときは設計を疑う

テストの書きにくさは、設計の問題を教えてくれるサインです。

| 書きにくさ | 疑うべき設計の問題 |
|---|---|
| ドメインのテストにDBが必要 | ドメイン層がインフラに依存している |
| テストの準備コードが異様に長い | Aggregate が大きすぎる、または依存が多すぎる |
| 現在時刻によってテスト結果が変わる | 時刻をドメインの内部で取得している |
| 1つのテストで多くのモックが必要 | Application Service の責務が大きすぎる |
| 内部実装を変えるとテストが壊れる | テストが振る舞いではなく実装を検証している |

## よくある誤解

- テストはカバレッジの数値を上げることが目的だと思いがち
- すべての層を同じ密度でテストすべきだと考えがち
- モックライブラリを使わないと Application Service はテストできないと思いがち

最も重要なのは、業務ルールが壊れたときにテストが失敗することです。業務ルールがドメイン層に集まっていれば、そのテストに最も力を入れることになり、結果として速くて安定したテストが厚くなります。

次章からは、ここまでの内容をタスク管理アプリとしてまとめて実装していきます。
