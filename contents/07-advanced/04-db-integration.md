# DB連携と永続化モデルの分離

DDDを実務へ持ち込むとき、多くの人が最初にぶつかるのがDB連携です。ここでは「ドメインモデルをそのままDBの形にしない」という基本方針を、Rustのコードでどう扱うかを整理します。

この章は、永続化モデルとドメインモデルの境界に集中する章です。トランザクションの切り方そのものは `トランザクション境界とApplication Service` で扱っているため、ここでは境界の置き場所だけを主題にします。

## なぜ分けるのか

DB上の表現とドメイン上の表現は、たいてい一致しません。

- DBでは文字列だが、ドメインではValue Objectにしたい
- DBではNULL許容だが、ドメインでは必須にしたい
- DBでは複数カラムだが、ドメインでは1つの概念として扱いたい

この差を無視すると、ドメインモデルが永続化都合に引っ張られやすくなります。

## TaskRow を使った変換例

このタスク管理アプリでは、`TaskRow` という永続化モデルの例を追加しています。

```rust
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub status: String,
}
```

そして、`TaskRow` から `Task` への変換は `TryFrom` で表現しています。

```rust
impl TryFrom<TaskRow> for Task {
    type Error = TaskError;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        let id = TaskId::new(row.id)?;
        let title = TaskTitle::new(row.title)?;
        let status = TaskStatus::from_str(&row.status)?;

        Ok(Task::restore(id, title, status))
    }
}
```

この形にしておくと、DBから不正値が来たときにも、ドメインモデルへ侵入する前に検知できます。

## Rustで現実的に使う選択肢

Rustでは、次のような選択肢が現実的です。

- `sqlx`: SQLを自分で管理したいとき
- `SeaORM`: ORMベースで進めたいとき
- `tokio-postgres`: 低レベル寄りに制御したいとき

どのライブラリを使うにしても、重要なのはライブラリ依存をInfrastructure層へ閉じ込めることです。ドメイン層はテーブル定義や接続プールを知るべきではありません。

## まとめ

DB連携で大切なのは、DBを避けることではなく、DB都合の責務をドメイン層へ侵入させないことです。`TaskRow` のような中間表現を置くだけでも、境界はかなり明確になります。