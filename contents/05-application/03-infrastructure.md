# 第5章-3 Infrastructure層の実装方針

Infrastructure層は、ドメインの外側にある技術的関心事を受け持つ層です。DB、外部API、メッセージング、時刻取得などがここに入ります。

この章の役割は、Infrastructure層を広く俯瞰して「なぜ必要か」を押さえることです。一覧取得の読み取りモデル、トランザクション境界、外部API連携の詳細は、後続の章でそれぞれ個別に掘り下げます。

## タスク管理アプリでの役割

ここでは、Infrastructure層の最小例として `InMemoryTaskRepository` というインメモリ実装を扱います。これはタスクをメモリ上へ保持するRepositoryで、本番向けの永続化ではありませんが、学習用としては非常に重要です。

- ドメインとアプリケーション層の責務を分けて検証できる
- テストだけでユースケースを動かせる
- DBやフレームワークの都合を持ち込まずに済む

拡張後のタスク管理アプリでは、同じインメモリ実装に一覧取得用の `TaskSummaryReader` も持たせています。これは一覧表示向けの要約データを読むための境界で、書き込み用のRepositoryと読み取り用の問い合わせを分けて考える入口になります。

この分離が必要になるのは、一覧表示と個別更新で求められる責務が大きく異なるからです。更新では `Task` の整合性や状態遷移が重要ですが、一覧ではIDやタイトル、状態のような要約情報だけが取れれば十分なことが多くあります。同じモデルをそのまま返し続けると、読み取りのためだけに不要な責務まで引きずり込みやすくなります。

また、外部API連携の入口として `TaskCompletionNotifier` のようなポートも追加できます。これは「タスク完了を外部へ通知する境界」を表す名前の一例です。ここでは「外部連携の詳細はInfrastructureに閉じる」という方針だけを押さえ、通知やイベント連携の実例は後続章へ譲ります。

## InMemoryTaskRepository の意味

インメモリ実装は単なる手抜きではありません。学習段階では、永続化技術よりも境界設計そのものを理解する方が重要です。

この実装では `HashMap<String, TaskRow>` という単純なメモリ上の対応表をDBのテーブルに見立てて保存しています。保存するのは `Task` そのものではなく、DBの行に近い `TaskRow` です。保存時に `Task` から `TaskRow` へ、取得時に `TaskRow` から `Task` へ変換することで、DB実装と同じ「行とドメインモデルの境界」をメモリ上で再現しています。Application Service はDBの知識を一切持たず、Repositoryトレイトだけに依存できます。

## 本番向け実装へ進むときの考え方

DB実装に進むときも、方針は変わりません。

- ドメインモデルはドメイン都合で設計する
- 永続化モデルはDB都合で設計する
- 両者の変換はInfrastructure側で引き受ける

たとえば `TaskRow` のような構造体を置き、SQLやORMの結果を一度そこへ受けた上で `Task` に変換する、という形は実務でもかなり扱いやすいです。サンプルの `InMemoryTaskRepository` もすでにこの形で書かれているので、DB実装に置き換えるときは「`HashMap` への出し入れ」を「SQLの実行」に差し替えるだけで済みます。`TaskRow` はDBレコードに近い形の中間表現だと考えると十分です。DBモデル分離の具体例は発展章の `DB連携と永続化モデルの分離` で詳しく扱います。

ここを崩してORM中心でモデルを作ると、ドメインルールよりテーブル都合が前に出やすくなります。

## コラム: コンポジションルートで依存を組み立てる

ここまでで、Application Service は `TaskRepository` トレイトだけに依存し、具体的な実装は Infrastructure 層に置く、という形になりました。では「実際にどの実装を使うか」は、どこで決めればよいのでしょうか。

その答えになるのが **コンポジションルート(Composition Root)** です。

### DDDの用語ではない

最初に注意しておくと、コンポジションルートはDDDの用語ではありません。Eric Evans の『ドメイン駆動設計』にも、Vaughn Vernon の『実践ドメイン駆動設計』にも、パターンとしては登場しません。Mark Seemann が『Dependency Injection in .NET』で広めた、DI(依存性注入)の文脈の言葉です。

それでもここで取り上げるのは、DDDをレイヤードアーキテクチャやヘキサゴナルアーキテクチャと組み合わせて実装すると、ほぼ必ずこの場所が必要になるからです。

### 意味

コンポジションルートとは、**アプリケーションのエントリポイント付近にある、オブジェクトの依存関係をまとめて組み立てる唯一の場所** のことです。

- 具象型を選び、つなぎ合わせる処理はここに集める
- ドメイン層やアプリケーション層は、具象型を一切知らない
- 依存方向(infrastructure → application → domain)を守ったまま、全層を知ってよい唯一の場所になる

### タスク管理アプリでの例

本書のサンプルコードは `lib.rs` のみのライブラリ構成で、組み立てはテストの中で行っています。実行可能なアプリケーションにするなら、`main.rs` がコンポジションルートになります。

```rust
use study_of_ddd_on_rust::application::{TaskApplicationService, TaskCompletedNotificationHandler};
use study_of_ddd_on_rust::domain::{OpenTaskLimitPolicy, UserId};
use study_of_ddd_on_rust::infrastructure::{InMemoryTaskRepository, LoggingTaskCompletionNotifier};

fn main() {
    // ここで具象型を選ぶ。将来DB実装に差し替えるときも、変更はこの行だけで済む
    let repository = InMemoryTaskRepository::new();
    let event_publisher = TaskCompletedNotificationHandler::new(LoggingTaskCompletionNotifier);

    // Application Service にはトレイトを満たす値として注入する
    let mut service = TaskApplicationService::new(
        repository,
        event_publisher,
        OpenTaskLimitPolicy::default(),
        UserId::new("me").unwrap(),
    );

    let task = service.create_task("task-1", "Write chapter draft").unwrap();
    println!("{:?}", task);
}
```

`TaskApplicationService<R: TaskRepository, P: DomainEventPublisher>` はジェネリクスで書かれているため、`R` や `P` に何を入れるかは呼び出し側が決めます。その「呼び出し側の一番外側」がコンポジションルートです。たとえば PostgreSQL 実装を追加した場合でも、`TaskApplicationService` やドメイン層のコードには手を入れず、`main.rs` で生成する Repository を差し替えるだけで済みます。

テストコードで `InMemoryTaskRepository` を生成して `TaskApplicationService::new` に渡しているのも、テスト用の小さなコンポジションルートだと見ることができます。

### やってはいけないこと

コンポジションルートの考え方を崩す典型例は、Application Service の内部で具象型を直接生成してしまうことです。

```rust
// 悪い例: Application Service が Infrastructure の具象型を知ってしまう
impl TaskApplicationService {
    pub fn new() -> Self {
        Self {
            repository: InMemoryTaskRepository::new(),
        }
    }
}
```

こうすると、application → infrastructure という逆向きの依存が生まれ、テスト時に実装を差し替えることもできなくなります。具象型の生成は外側に追い出し、内側はトレイトだけを受け取る、という線引きを守ることが大切です。

### Rustでの実践上のポイント

- RustにはSpringやASP.NET Coreのような標準的なDIコンテナはないため、`main.rs` で手書きで組み立てる **Pure DI** が基本になる
- 依存が増えてきたら、`main.rs` から `bootstrap.rs` や `app_state.rs` のようなモジュールへ組み立て処理を切り出すと読みやすい
- Webフレームワーク(axumなど)を使う場合は、`main` で組み立てた Application Service を `State` として各ハンドラへ渡す形が典型的

ディレクトリ構成全体の中でこの組み立て処理をどこに置くかは、発展章の `実務で使うディレクトリ構造の考え方` もあわせて参照してください。

## 学習用コードとしての価値

インメモリ実装をサンプルコードとして手元に持っておくと、章で説明した抽象化が本当に必要かをすぐ検証できます。学習コンテンツにおいては、動く最小構成を維持すること自体が重要な品質です。

このあと第6章のタスク管理アプリ編へ進むときは、今の `Task`、`TaskRepository`、`TaskApplicationService` を土台にユースケースを増やしていく形が自然です。