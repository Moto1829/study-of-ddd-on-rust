# Domain Serviceが必要になる具体例

ここまでは、`Task` や `TaskTitle` の責務だけでかなり多くのルールを表現できました。しかし実務では、1つのEntityだけでは判断できないルールも普通に出てきます。そういう場面で、はじめてDomain Serviceが自然な選択肢になります。

## 例: 1ユーザーが同時に持てる未完了タスク数の上限

たとえば次のようなルールを考えます。

- 1ユーザーが同時に持てるOpenなタスクは5件まで

このルールは、`Task` 単体では判断できません。必要なのは次の情報です。

- 誰のタスクか
- そのユーザーが今いくつ未完了タスクを持っているか

これは1つのEntityの内部状態では完結しないので、Domain Serviceとして切り出す余地があります。

## サンプル実装

このリポジトリでは、`OpenTaskLimitPolicy` をDomain Serviceの具体例として追加しています。

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

## よくある誤解

- 件数確認があるなら全部Application Serviceで良いと思いがち
- Domain Serviceはとにかく増やすものだと誤解しがち
- 単に複数Repositoryを呼ぶだけの処理もDomain Serviceだと考えがち

大事なのは、そこに業務判断があるかどうかです。Entityへ置けないが、確かにドメインルールである。そういうときにDomain Serviceは意味を持ちます。