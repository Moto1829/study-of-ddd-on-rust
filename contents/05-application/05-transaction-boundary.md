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
pub struct BulkTaskCompletionService<R, T>
where
    R: TaskRepository,
    T: TransactionRunner<TaskApplicationError>,
{
    repository: R,
    transaction_runner: T,
}

impl<R, T> BulkTaskCompletionService<R, T>
where
    R: TaskRepository,
    T: TransactionRunner<TaskApplicationError>,
{
    pub fn complete_tasks<I, S>(&mut self, task_ids: I) -> Result<Vec<TaskOutput>, TaskApplicationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let repository = &mut self.repository;

        self.transaction_runner.run(|| {
            let mut outputs = Vec::new();

            for task_id in task_ids {
                let task_id = TaskId::new(task_id)?;
                let mut task = repository.find_by_id(&task_id).ok_or_else(|| {
                    TaskApplicationError::TaskNotFound {
                        task_id: task_id.value().to_owned(),
                    }
                })?;

                task.complete()?;
                outputs.push(TaskOutput::from(&task));
                repository.save(task);
            }

            Ok(outputs)
        })
    }
}
```

ポイントは次の3つです。

- **境界を決めているのはApplication Service**: `run` に渡すクロージャの範囲が、そのまま1つのトランザクションになる。「全件をまとめて包む」という判断はユースケースの設計そのもの
- **ドメイン層はトランザクションを知らない**: `Task::complete` は、自分がトランザクションの中で呼ばれているかどうかを気にしない
- **失敗は `Result` で伝える**: クロージャの中で `?` によって `Err` が返ると、`TransactionRunner` はそれまでの変更を取り消す

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
let mut service = BulkTaskCompletionService::new(repository.clone(), runner);
```

テスト `bulk_completion_rolls_back_earlier_tasks_when_a_later_task_fails` では、1件目のタスクを完了して保存したあと、2件目が完了済みで失敗するケースを確かめています。結果はエラーになり、1件目の完了も取り消されて `Open` のまま残ります。

本番のDB実装に置き換えるなら、`run` の中で `BEGIN` を発行し、`Ok` なら `COMMIT`、`Err` なら `ROLLBACK` を発行する形になります。その場合は、Repository がトランザクション中の同じ接続を使うように組み立てる必要があります。Application Service のコードは変わりません。

## 一括更新との関係

たとえば「複数タスクをまとめて完了する」ユースケースでは、途中で1件だけ失敗したときにどうするかを決める必要があります。

- 全件成功でのみ確定する
- 成功した分だけ反映する
- 失敗を集約して返す

サンプルの `BulkTaskCompletionService` は、このうち「全件成功でのみ確定する」を選んだ例です。この判断はドメインモデルだけではなく、ユースケースとしての取り扱いに関わります。だからこそ、トランザクション境界はApplication Serviceの話として整理した方が分かりやすいです。

たとえば3件のタスクをまとめて完了する処理で、2件目だけが「すでに完了済み」で失敗したとします。このとき、1件目だけ完了を確定してよいのか、それとも3件とも取り消すべきかは、永続化技術の都合ではなくユースケースの設計判断です。こうした「どの変更を1まとまりとして扱うか」を意識すると、トランザクション境界や Unit of Work の必要性が見えやすくなります。

## 関連する考え方: Unit of Work

トランザクション境界を考えるとき、関連する考え方として Unit of Work があります。これは、1つのユースケースの中で発生した変更を追跡し、最後にまとめて確定するための考え方です。たとえば複数のEntityを読み出して変更し、それらを最後にまとめて永続化するような実装では、この概念が前面に出てきます。

`TransactionRunner` が「どこからどこまでを1つのトランザクションで包むか」という境界を表すのに対して、Unit of Work は「その中でどの変更をまとめて確定するか」という見方に近いものです。実装によってはORMや永続化ライブラリが暗黙に担うこともあります。

ただし、この本の段階では Unit of Work を独立したパターンとして深掘りしなくても構いません。まず重要なのは、「どこまでを1回の処理として成功させるか」をApplication Serviceの責務として捉えることです。その意味で `TransactionRunner` を理解できれば、学習の第一段階としては十分です。

## よくある誤解

- Repositoryがトランザクションを全部隠してくれると思いがち
- ドメインモデルのメソッド内でトランザクションを開始したくなりがち
- 1件更新でも毎回複雑なUnit of Workを導入したくなりがち

学習段階では、まず「トランザクション境界はユースケース単位で考える」という理解があれば十分です。