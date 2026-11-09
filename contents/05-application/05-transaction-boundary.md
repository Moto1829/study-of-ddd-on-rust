# 第5章-5 トランザクション境界とApplication Service

実務でApplication Serviceを考えるとき、避けて通れないのがトランザクションです。ユースケースの途中でDB更新が中途半端に終わると、ドメインルール以前にシステム整合性が壊れます。

この章では、トランザクションを「ユースケースのまとまり」として捉えることに集中します。DBアクセスの詳細や永続化モデル変換そのものは、後続のDB連携章で扱います。

## なぜApplication Serviceで考えるのか

トランザクションは、通常はユースケースの単位で張ります。これは業務ルールの本体ではなく、「どこまでを1回の処理として成功させるか」というアプリケーション上の責務だからです。

そのため、ドメインモデル自体にトランザクションAPIを持ち込むより、Application Service側で境界として扱う方が整理しやすくなります。

## 基本の考え方

まずは次の方針で十分です。

- 1ユースケース = 1トランザクションを基本にする
- 複数Repository更新があるなら、Application Service側でまとめて管理する
- ドメイン層はトランザクションの存在を知らないまま保つ

## どう表現するか

このタスク管理アプリでは、トランザクション境界の概念例として `TransactionRunner` という小さなインターフェースを置いています。名前は例ですが、「ある処理を1つのトランザクションで包む役割」を表しています。

```rust
pub trait TransactionRunner<E> {
    fn run<T, F>(&self, operation: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>;
}
```

これは実装そのものというより、「トランザクションを張る仕組みはApplication Serviceの外側(Infrastructure)にあるが、どこを包むかはユースケース単位でApplication Serviceが決める」という考え方を示すための最小インターフェースです。

## 使用例: 複数タスクの一括完了

サンプルコードでは、`TransactionRunner` を使うユースケースとして `BulkTaskCompletionService`(`src/application/bulk_task_completion_service.rs`)を用意しています。指定した複数のタスクをまとめて完了し、1件でも失敗したら全件を取り消す、という方針のユースケースです。

```rust
pub struct BulkTaskCompletionService<R, T, P>
where
    R: TaskRepository,
    T: TransactionRunner<TaskApplicationError>,
    P: DomainEventPublisher,
{
    repository: R,
    transaction_runner: T,
    event_publisher: P,
}

impl<R, T, P> BulkTaskCompletionService<R, T, P>
where
    R: TaskRepository,
    T: TransactionRunner<TaskApplicationError>,
    P: DomainEventPublisher,
{
    pub fn new(repository: R, transaction_runner: T, event_publisher: P) -> Self {
        Self {
            repository,
            transaction_runner,
            event_publisher,
        }
    }

    pub fn complete_tasks<I, S>(&mut self, task_ids: I) -> Result<Vec<TaskOutput>, TaskApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        // クロージャは repository だけを可変借用し、transaction_runner は不変借用する。
        // 別々のフィールドなので、同時に借用しても衝突しない。
        let repository = &mut self.repository;

        let (outputs, events) = self.transaction_runner.run(|| {
            let mut outputs = Vec::new();
            let mut events = Vec::new();

            for task_id in task_ids {
                let task_id = TaskId::new(task_id)?;
                let mut task = repository.find_by_id(&task_id).ok_or_else(|| {
                    TaskApplicationError::TaskNotFound {
                        task_id: task_id.value().to_owned(),
                    }
                })?;

                // ここで失敗すると、それまでに保存した分も TransactionRunner が取り消す
                task.complete()?;
                outputs.push(TaskOutput::from(&task));
                events.push(TaskEvent::TaskCompleted {
                    task_id: task.id().clone(),
                });
                repository.save(task);
            }

            Ok((outputs, events))
        })?;

        // トランザクションが確定してから、起きた事実を発行する
        for event in &events {
            self.event_publisher.publish(event)?;
        }

        Ok(outputs)
    }
}
```

ポイントは次の3つです。

- **境界を決めているのはApplication Service**: `run` に渡すクロージャの範囲が、そのまま1つのトランザクションになる。「全件をまとめて包む」という判断はユースケースの設計そのもの
- **ドメイン層はトランザクションを知らない**: `Task::complete` は、自分がトランザクションの中で呼ばれているかどうかを気にしない
- **失敗は `Result` で伝える**: クロージャの中で `?` によって `Err` が返ると、`TransactionRunner` はそれまでの変更を取り消す
- **イベントは確定後に発行する**: `run(...)?` の `?` で、取り消された場合はそのまま呼び出し元へ `Err` を返す。`run` が `Ok` を返したときだけ `TaskCompleted` を発行するので、取り消された完了について通知が飛ぶことはない

`?` が2か所にあることに注意してください。ループの中の `?` はクロージャから抜けてロールバックさせるためのもので、`run(...)?` の `?` はロールバックされた `Err` を `complete_tasks` の呼び出し元へ返すためのものです。

なお、この方針では、指定したタスクが1件でも完了できなければ(存在しない、すでに完了済み、アーカイブ済み)全件が取り消されます。同じIDを2回指定した場合も、2回目が `TaskAlreadyCompleted` になるため全体が失敗します。完了済みのタスクをとばす冪等な作りにするかどうかは、業務上「完了済みが混ざっていたら誤操作として知らせるべきか」で決める判断です。

なお、クロージャが `self.repository` を可変借用している間に `self.transaction_runner` を不変借用していますが、別々のフィールドなのでRustの借用規則には違反しません。

### インメモリでのロールバック

Infrastructure側には、学習用の実装として `InMemoryTransactionRunner`(`src/infrastructure/in_memory_transaction_runner.rs`)を置いています。`InMemoryTaskRepository` と同じ保存領域を共有し、処理の前にスナップショットを取り、`Err` が返ったらスナップショットへ戻します。

```rust
impl<E> TransactionRunner<E> for InMemoryTransactionRunner {
    fn run<T, F>(&self, operation: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let snapshot = self.store.snapshot();
        let result = operation();

        if result.is_err() {
            self.store.restore(snapshot);
        }

        result
    }
}
```

組み立ては次のようになります。Repository と TransactionRunner が同じ保存領域を見ていることが重要です。

```rust
let repository = InMemoryTaskRepository::new();
let runner = InMemoryTransactionRunner::new(&repository);
let publisher = TaskCompletedNotificationHandler::new(LoggingTaskCompletionNotifier);
let mut service = BulkTaskCompletionService::new(repository.clone(), runner, publisher);
```

`BulkTaskCompletionService` は、`run` が `Ok` を返したあと、つまりトランザクションが確定してから `TaskCompleted` イベントをまとめて発行します。`run` の中で発行すると、あとで取り消された完了についても通知が飛んでしまうからです。イベント発行の詳しい考え方は、第7章-6の `ドメインイベントによる副作用の分離` を参照してください。

テスト `bulk_completion_rolls_back_earlier_tasks_when_a_later_task_fails` では、1件目のタスクを完了して保存したあと、2件目が完了済みで失敗するケースを確かめています。結果はエラーになり、1件目の完了も取り消されて `Open` のまま残ります。

本番のDB実装に置き換えるなら、`run` の中で `BEGIN` を発行し、`Ok` なら `COMMIT`、`Err` なら `ROLLBACK` を発行する形になります。その場合は、Repository がトランザクション中の同じ接続を使うように組み立てる必要があります。Application Service のコードは変わりません。

## 一括更新との関係

たとえば「複数タスクをまとめて完了する」ユースケースでは、途中で1件だけ失敗したときにどうするかを決める必要があります。

- 全件成功でのみ確定する
- 成功した分だけ反映する
- 失敗を集約して返す

サンプルの `BulkTaskCompletionService` は、このうち「全件成功でのみ確定する」を選んだ例です。この判断はドメインモデルだけではなく、ユースケースとしての取り扱いに関わります。だからこそ、トランザクション境界はApplication Serviceの話として整理した方が分かりやすいです。

たとえば3件のタスクをまとめて完了する処理で、2件目だけが「すでに完了済み」で失敗したとします。このとき、1件目だけ完了を確定してよいのか、それとも3件とも取り消すべきかは、永続化技術の都合ではなくユースケースの設計判断です。こうした「どの変更を1まとまりとして扱うか」を意識すると、トランザクション境界や Unit of Work の必要性が見えやすくなります。

## 複数のRepositoryをまとめる: Unit of Work

`TransactionRunner` は「どこからどこまでを1つのトランザクションで包むか」という境界だけを表します。`BulkTaskCompletionService` のように更新する Repository が1つなら、これで十分です。

しかし、1つのユースケースで**複数の Repository(= 複数の Aggregate)を更新し、それらをまとめて確定したい**場合は、それぞれの Repository が同じトランザクションに属している必要があります。これを表すのが Unit of Work です。1つのユースケースの中で発生した変更を追跡し、最後にまとめて確定するための考え方です。

### いつ使うべきか

DDDの原則は「1つのトランザクションで更新するのは1つの Aggregate」です。複数の Aggregate の整合性を保ちたいときは、次の順で検討します(詳しくは第7章-12)。

1. 数秒のずれが許されるなら、イベントでつなぐ結果整合性にする(第7章-12、第7章-14)
2. 一瞬もずれてはいけないなら、本当に別の Aggregate であるべきかを見直す
3. それでも別の Aggregate であるべきで、同じDBにあるなら、**Unit of Work で1つのトランザクションにする**

つまり Unit of Work で複数の Aggregate を更新するのは、原則に対する「意識した例外」です。

### サンプル: プロジェクトのクローズ

サンプルコードには、タスクをまとめる `Project` Aggregate(`src/domain/project.rs`)を追加しています。`Project` は所属するタスクを `TaskId` で参照し、`Task` そのものは持ちません。

業務ルールとして、「プロジェクトをクローズしたら、所属するタスクをすべてアーカイブする。クローズ済みのプロジェクトに、アーカイブされていないタスクが一瞬でも残ってはいけない」を前提にします。このルールを守るには、`Project` と複数の `Task` を1つのトランザクションで更新する必要があります。

まず、Unit of Work をアプリケーション層のトレイトとして定義します(`src/application/unit_of_work.rs`)。

```rust
pub trait UnitOfWork {
    type Tasks: TaskRepository;
    type Projects: ProjectRepository;

    fn tasks(&mut self) -> &mut Self::Tasks;
    fn projects(&mut self) -> &mut Self::Projects;

    /// 変更をまとめて確定する。`self` を消費するので、確定後に使い続けることはできない。
    fn commit(self) -> Result<(), UnitOfWorkError>;
}

/// ユースケースごとに新しい Unit of Work(= トランザクション)を開始する。
pub trait UnitOfWorkFactory {
    type UnitOfWork: UnitOfWork;

    fn begin(&self) -> Self::UnitOfWork;
}
```

`tasks()` と `projects()` が返す Repository は、どちらも同じトランザクションに属します。`commit` は `self` を消費するので、確定したあとに Unit of Work を使い続けるコードはコンパイルエラーになります。また、`commit` せずに drop すると変更は破棄されます。`?` で早期リターンすれば、自動的にロールバックされるということです。

Application Service(`src/application/project_application_service.rs`)は、Repository を直接持たず、ユースケースごとに `UnitOfWorkFactory::begin` で Unit of Work を開始します。

```rust
pub fn close_project(
    &self,
    project_id: impl Into<String>,
) -> Result<ProjectOutput, ProjectApplicationError> {
    let project_id = ProjectId::new(project_id)?;

    let mut uow = self.unit_of_work_factory.begin();

    let mut project = find_project(&mut uow, &project_id)?;
    project.close()?;

    let mut events = Vec::new();
    for task_id in project.task_ids() {
        let mut task = uow.tasks().find_by_id(task_id).ok_or_else(|| {
            // ここで抜けると uow は commit されずに drop され、
            // それまでにアーカイブしたタスクも含めてすべて破棄される
            ProjectApplicationError::TaskNotFound {
                task_id: task_id.value().to_owned(),
            }
        })?;

        // すでにアーカイブ済みのタスクは、目的の状態なので何もしない
        if task.status() == TaskStatus::Archived {
            continue;
        }

        task.archive()?;
        events.push(TaskEvent::TaskArchived {
            task_id: task.id().clone(),
        });
        uow.tasks().save(task);
    }

    let output = ProjectOutput::from(&project);
    uow.projects().save(project);

    // Project と Task の変更を、ここでまとめて確定する
    uow.commit()?;

    // 確定してから、起きた事実を発行する
    for event in &events {
        self.event_publisher.publish(event)?;
    }

    Ok(output)
}
```

ポイントは次の通りです。

- **ドメインはトランザクションを知らない**: `Project::close` は自分の状態だけを変え、タスクのアーカイブは扱いません。2つの Aggregate を同じトランザクションで更新すると決めているのは Application Service です
- **確定は1か所**: `uow.commit()` を呼ぶまで、`Project` の変更も `Task` の変更も確定しません。途中の `?` で抜ければ、どちらも破棄されます
- **イベントは確定後に発行する**: `BulkTaskCompletionService` と同じく、`commit` が成功してから `TaskArchived` を発行します
- **すでにアーカイブ済みのタスクは何もしない**: 目的の状態になっているものは変更しないので、同じプロジェクトを何度クローズしようとしても、タスク側で失敗することはありません(プロジェクト自体の二重クローズは `ProjectAlreadyClosed` になります)

### インメモリでの実装

インフラ層の `InMemoryUnitOfWork`(`src/infrastructure/in_memory_unit_of_work.rs`)は、開始時に共有の保存領域を複製した「作業用の Repository」を作ります。ユースケースはこの作業用の Repository だけを読み書きし、`commit` で共有の保存領域へ書き戻します。

```rust
impl UnitOfWorkFactory for InMemoryUnitOfWorkFactory {
    type UnitOfWork = InMemoryUnitOfWork;

    fn begin(&self) -> Self::UnitOfWork {
        InMemoryUnitOfWork {
            shared_tasks: self.tasks.clone(),
            shared_projects: self.projects.clone(),
            tasks: InMemoryTaskRepository::from_snapshot(self.tasks.snapshot()),
            projects: InMemoryProjectRepository::from_snapshot(self.projects.snapshot()),
        }
    }
}

impl UnitOfWork for InMemoryUnitOfWork {
    // ...
    fn commit(self) -> Result<(), UnitOfWorkError> {
        self.shared_tasks.restore(self.tasks.snapshot());
        self.shared_projects.restore(self.projects.snapshot());
        Ok(())
    }
}
```

これで、DBのトランザクションの「確定するまで他から見えない」「確定しなければ残らない」をメモリ上で再現しています。`InMemoryTransactionRunner` が「先に変更して、失敗したら戻す」方式だったのに対して、こちらは「別の場所で変更して、成功したら反映する」方式です。

ただし学習用の簡易実装なので、2つの Unit of Work が同時に同じデータを変更しても競合は検出せず、後から `commit` した方が勝ちます。本番では、第7章-13の楽観ロックなどと組み合わせます。

組み立ては次のようになります。Factory に、タスク用とプロジェクト用の両方の Repository を渡します。

```rust
let tasks = InMemoryTaskRepository::new();
let projects = InMemoryProjectRepository::new();
let project_service = ProjectApplicationService::new(
    InMemoryUnitOfWorkFactory::new(&tasks, &projects),
    publisher,
);
```

テストでは、次の3点を確かめています。

- `closing_project_archives_all_its_tasks_in_one_unit_of_work`: クローズでプロジェクトと全タスクが一緒に更新される
- `closing_project_rolls_back_project_and_tasks_when_a_task_is_missing`: 途中のタスクが見つからないと、プロジェクトのクローズも、それまでのアーカイブも確定しない
- `unit_of_work_changes_are_invisible_until_commit_and_discarded_on_drop`: `commit` 前の変更は外から見えず、drop すれば破棄される

### DB実装に置き換えるなら

`sqlx` を使う場合は、`begin` でトランザクションを開始し、各 Repository がその接続を使ってクエリを発行する形になります。

```rust
pub struct PgUnitOfWork {
    tx: sqlx::Transaction<'static, sqlx::Postgres>,
}

impl PgUnitOfWork {
    pub async fn commit(self) -> Result<(), UnitOfWorkError> {
        self.tx.commit().await.map_err(|error| UnitOfWorkError::CommitFailed {
            reason: error.to_string(),
        })
    }
}
```

`sqlx::Transaction` は、`commit` せずに drop されると自動的にロールバックします。サンプルの `InMemoryUnitOfWork` と同じ性質です。Application Service のコードは、Repository のメソッドが `async` になる点(第5章-8)を除けば変わりません。

## よくある誤解

- Repositoryがトランザクションを全部隠してくれると思いがち
- ドメインモデルのメソッド内でトランザクションを開始したくなりがち
- 1件更新でも毎回複雑なUnit of Workを導入したくなりがち
- 複数の Aggregate を1つのトランザクションで更新できるなら、結果整合性は不要だと考えがち

学習段階では、まず「トランザクション境界はユースケース単位で考える」という理解があれば十分です。Unit of Work は、更新する Repository が複数になり、しかも即時の整合性が必要なときに初めて導入すれば足ります。