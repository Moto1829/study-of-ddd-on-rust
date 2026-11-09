# 第4章-5 Domain Serviceが必要になる具体例

ここまでは、`Task` や `TaskTitle` の責務だけでかなり多くのルールを表現できました。しかし実務では、1つのEntityだけでは判断できないルールも普通に出てきます。そういう場面で、はじめてDomain Serviceが自然な選択肢になります。

## 例: 1ユーザーが同時に持てる未完了タスク数の上限

たとえば次のようなルールを考えます。

- 1ユーザーが同時に持てるOpenなタスクは5件まで

このルールは、`Task` 単体では判断できません。必要なのは次の情報です。

- 誰のタスクか
- そのユーザーが今いくつ未完了タスクを持っているか

これは1つのEntityの内部状態では完結しないので、Domain Serviceとして切り出す余地があります。

## サンプル実装

この本のタスク管理アプリでは、`OpenTaskLimitPolicy` という名前のポリシー型をDomain Serviceの具体例として置いています。名前は一例ですが、「未完了タスク数の上限を判定する業務ルール」を表しています。

```rust
pub struct OpenTaskLimitPolicy {
    max_open_tasks: usize,
}

impl OpenTaskLimitPolicy {
    pub fn ensure_can_create(
        &self,
        user_id: &UserId,
        open_task_count: usize,
    ) -> Result<(), TaskCreationPolicyError> {
        if open_task_count >= self.max_open_tasks {
            return Err(TaskCreationPolicyError::OpenTaskLimitExceeded {
                user_id: user_id.value().to_owned(),
                max: self.max_open_tasks,
                actual: open_task_count,
            });
        }

        Ok(())
    }
}
```

ここで重要なのは、このロジックが単なるアプリケーションフローではなく、業務上の判断そのものであることです。

## Application Serviceとの分担

この種のルールを使うとき、Application Serviceは次のような流れになります。

1. ユーザーIDを受け取る
2. そのユーザーのOpenタスク数を取得する
3. `OpenTaskLimitPolicy` で作成可能か判定する
4. 問題なければTaskを生成して保存する

このとき、件数を数えるのはApplication ServiceやRepositoryの役目であっても、「この件数なら作成してよいか」という判断はDomain Serviceの責務です。

## サンプルコードでの組み込み

サンプルコードでは、`TaskApplicationService::create_task` がこの流れをそのまま実装しています。

```rust
pub fn create_task(
    &mut self,
    task_id: impl Into<String>,
    title: impl Into<String>,
) -> Result<TaskOutput, TaskApplicationError> {
    let task_id = TaskId::new(task_id)?;
    let title = TaskTitle::new(title)?;

    if self.repository.find_by_id(&task_id).is_some() {
        return Err(TaskApplicationError::TaskAlreadyExists {
            task_id: task_id.value().to_owned(),
        });
    }

    // 件数を数えるのは Repository、作成してよいかを判断するのは Domain Service
    let open_task_count = self.repository.count_open_tasks();
    self.creation_policy
        .ensure_can_create(&self.owner, open_task_count)?;

    let task = Task::new(task_id, title);
    // ...保存とイベント発行
}
```

`OpenTaskLimitPolicy` と対象ユーザーの `UserId` は、`TaskApplicationService::new` で外から渡します。上限値は `main.rs`(コンポジションルート)で決め、CLIでは環境変数 `TASK_APP_MAX_OPEN_TASKS` で変えられます。

```bash
$ printf 'create t1 牛乳を買う\ncreate t2 卵を買う\n' | TASK_APP_MAX_OPEN_TASKS=1 cargo run -q
> 作成しました: [t1] 牛乳を買う (open)
> 未完了のタスクは1件までです。完了かアーカイブをしてから作成してください
```

ポリシーの拒否は `TaskCreationPolicyError` として返り、Application Service で `TaskApplicationError::CreationPolicy` に包まれます。ドメインルール違反(`TaskError`)とは別のバリアントにしているので、どのルールで拒否されたかを呼び出し側が区別できます。

### なぜタスクに所有者がないのか

この本のタスク管理アプリは「個人向けタスク管理」という1つの Bounded Context なので、1つの Repository に入っているタスクはすべて1人の利用者のものとして扱っています。そのため `Task` 自体は所有者を持たず、`count_open_tasks` は Repository 全体の Open タスクを数えます。

複数の利用者が同じ Repository を共有するなら、`Task` に `owner_id: UserId` を持たせ、Repository も `count_open_tasks(&self, owner: &UserId)` のように所有者で絞り込む形に変わります。それでも `OpenTaskLimitPolicy` のコードは変わりません。判断に必要な値(誰の、何件か)を受け取るだけで、その値の集め方を知らないからです。所有者を持たせた場合の認可の考え方は、第5章-9の `認可をどの層で扱うか` で扱っています。

## よくある誤解

- 件数確認があるなら全部Application Serviceで良いと思いがち
- Domain Serviceはとにかく増やすものだと誤解しがち
- 単に複数Repositoryを呼ぶだけの処理もDomain Serviceだと考えがち

大事なのは、そこに業務判断があるかどうかです。Entityへ置けないが、確かにドメインルールである。そういうときにDomain Serviceは意味を持ちます。

次章では、Entityの生成ルールをどこに置くかを扱う Factory パターンを見ます。
