# 第7章-13 楽観ロックで同時更新を検出する

Aggregate は整合性の境界であり、1つのトランザクションで1つの Aggregate を更新するのが原則です。しかし、同じ Aggregate を複数の利用者が同時に更新しようとしたらどうなるでしょうか。この章では、同時更新の問題と、その代表的な解決策である楽観ロックを扱います。

## 失われた更新

本書の Application Service は、次の流れでタスクを更新します。

1. Repository からタスクを読み込む
2. ドメインメソッドで状態を変える
3. Repository に保存する

この流れを2人が同時に実行すると、問題が起きます。

```text
時刻  利用者A                         利用者B
 1    タスクを読み込む(Open, "牛乳")
 2                                    タスクを読み込む(Open, "牛乳")
 3    rename("牛乳を2本買う")
 4    保存する(Open, "牛乳を2本買う")
 5                                    complete()
 6                                    保存する(Completed, "牛乳")
```

最終的に保存されるのは、利用者Bの `(Completed, "牛乳")` です。利用者Aの名前変更は、エラーもなく消えてしまいました。これを**失われた更新(Lost Update)** と呼びます。

さらに深刻なのは、ドメインのルールがすり抜けられる可能性があることです。利用者Bは「Open のタスクを完了する」という正しい操作をしたつもりですが、実際には利用者Aが変更した後の状態を見ていません。

## 悲観ロックと楽観ロック

同時更新を防ぐ方法は、大きく2つあります。

| 方式 | 考え方 | 仕組み | 向いている場面 |
|---|---|---|---|
| 悲観ロック | 競合は頻繁に起きると考え、先にロックする | `SELECT ... FOR UPDATE` で読み込み時にロックする | 競合が多い、処理が短い |
| 楽観ロック | 競合はめったに起きないと考え、保存時に検出する | バージョン番号を比較し、変わっていたら失敗させる | 競合が少ない、利用者の操作をはさむ |

タスク管理アプリのように、同じタスクを複数人が同時に編集することがまれなら、楽観ロックが向いています。

## バージョン番号を Aggregate に持たせる

楽観ロックでは、Aggregate にバージョン番号を持たせます。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version(u64);

impl Version {
    pub fn initial() -> Self {
        Self(0)
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

pub struct Task {
    id: TaskId,
    title: TaskTitle,
    status: TaskStatus,
    version: Version,
}
```

バージョン番号は業務上の概念ではなく、技術的な関心事です。それでも Aggregate に持たせるのは、「どの時点の状態を読み込んだか」を保存時まで運ぶ必要があるからです。業務ロジックからは参照しないため、ドメインメソッドの中でバージョンを触る必要はありません。

## Repository で競合を検出する

保存時には、「読み込んだときのバージョン」と「DBに保存されている現在のバージョン」が一致する場合だけ更新します。

```sql
UPDATE tasks
SET title = $1, status = $2, version = version + 1
WHERE id = $3 AND version = $4
```

`WHERE` 句の `version = $4` が楽観ロックの核心です。他の誰かが先に更新していれば、バージョンが進んでいるので、この `UPDATE` は1行も更新しません。更新件数が0件なら、競合が起きたと判断します。

```rust
impl TaskRepository for PostgresTaskRepository {
    async fn save(&self, task: &Task) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE tasks SET title = $1, status = $2, version = version + 1
             WHERE id = $3 AND version = $4",
        )
        .bind(task.title().value())
        .bind(task.status().as_str())
        .bind(task.id().value())
        .bind(task.version().value() as i64)
        .execute(&self.pool)
        .await
        .map_err(to_repository_error)?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::Conflict);
        }

        Ok(())
    }
}
```

新規作成のときは `INSERT` を使い、主キーの重複で競合を検出します。

## インメモリ実装でも同じ契約を守る

テストで使うインメモリ実装も、同じ振る舞いをするように作ります。そうしないと、Application Service のテストで競合時の振る舞いを確かめられません。

```rust
impl TaskRepository for InMemoryTaskRepository {
    fn save(&mut self, task: &Task) -> Result<(), RepositoryError> {
        let mut tasks = self.tasks.write().unwrap();

        if let Some(stored) = tasks.get(task.id()) {
            if stored.version() != task.version() {
                return Err(RepositoryError::Conflict);
            }
        }

        let mut next = task.clone();
        next.increment_version();
        tasks.insert(task.id().clone(), next);
        Ok(())
    }
}
```

「保存時にバージョンが一致しなければ `Conflict` を返す」というのは、`TaskRepository` トレイトの契約です。トレイトのドキュメントコメントに、この契約を明記しておくとよいでしょう。

## 競合したらどうするか

競合を検出したあとの対応は、ユースケースによって異なります。

### 利用者に知らせる

画面から利用者が操作した場合は、「他の人が先に更新しました。最新の内容を確認してください」と伝えるのが一般的です。

```rust
impl From<RepositoryError> for TaskApplicationError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::Conflict => Self::ConcurrentModification,
            other => Self::Repository(other),
        }
    }
}
```

Web API なら、HTTP 409 Conflict を返します。

### 自動でリトライする

システムが自動で行う処理(イベントハンドラなど)では、最新の状態を読み込み直してリトライできます。

```rust
pub fn archive_task_with_retry(&mut self, task_id: &TaskId) -> Result<(), TaskApplicationError> {
    const MAX_ATTEMPTS: usize = 3;

    for _ in 0..MAX_ATTEMPTS {
        let mut task = self.find_task(task_id)?;

        if task.status() == TaskStatus::Archived {
            return Ok(()); // すでに目的の状態なら何もしない
        }

        task.archive()?;

        match self.repository.save(&task) {
            Ok(()) => return Ok(()),
            Err(RepositoryError::Conflict) => continue, // 読み込み直してやり直す
            Err(error) => return Err(error.into()),
        }
    }

    Err(TaskApplicationError::ConcurrentModification)
}
```

リトライのたびに読み込み直すので、ドメインのルールは最新の状態に対して評価されます。他の誰かがすでにアーカイブしていれば、何もせずに成功とします。

## 画面をまたぐ楽観ロック

利用者が編集画面を開いてから保存ボタンを押すまでに、数分かかることもあります。その間に他の人が更新していたら、利用者は古い内容を見て編集していたことになります。

この競合を検出するには、バージョン番号を画面に渡し、保存時に送り返してもらいます。

```rust
pub struct RenameTaskInput {
    pub task_id: String,
    pub new_title: String,
    pub expected_version: u64, // 画面を開いたときのバージョン
}

pub fn rename_task(&mut self, input: RenameTaskInput) -> Result<TaskOutput, TaskApplicationError> {
    let mut task = self.find_task(&TaskId::new(input.task_id)?)?;

    if task.version().value() != input.expected_version {
        return Err(TaskApplicationError::ConcurrentModification);
    }

    task.rename(TaskTitle::new(input.new_title)?)?;
    self.repository.save(&task)?;
    // ...
}
```

Application Service での比較は、画面を開いてからの競合を検出します。Repository での比較は、読み込んでから保存するまでの競合を検出します。両方を組み合わせることで、利用者の操作全体を保護できます。

## Aggregate を小さく保つ理由

楽観ロックは、Aggregate 単位でバージョンを管理します。そのため、Aggregate が大きいほど競合が起きやすくなります。

前章の例で、`Project` が `Vec<Task>` を持つ設計にすると、どのタスクを編集しても `Project` のバージョンが上がります。プロジェクトに参加している全員が、互いに競合し続けることになります。

Aggregate を小さく保つことは、楽観ロックの競合を減らすことにもつながります。

## よくある誤解

- トランザクションを使えば、同時更新の問題は自動的に防げると思いがち
- バージョン番号はインフラの関心事なので、ドメインモデルに持たせるべきではないと考えがち
- 楽観ロックの競合はめったに起きないので、対応しなくてよいと思いがち

一般的なDBの既定の分離レベルでは、トランザクションを使っても失われた更新は防げません。また、競合がまれだからこそ、起きたときに気づけないと調査が困難になります。競合を検出してエラーにする仕組みは、最初から用意しておく価値があります。

次章では、ドメインイベントを外部へ確実に届けるための Outbox パターンを見ます。
