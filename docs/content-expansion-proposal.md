# コンテンツ拡張提案

このドキュメントは、現在の mdBook コンテンツをさらに実務寄りかつ学習効果の高いものへ発展させるための提案です。単に章を増やすのではなく、学習者が「概念は分かったが、実際のシステム開発ではどう使うのか」が見える構成を目指します。

## 方針

現在の内容は、DDDの導入から基本要素の説明、小さなタスク管理ドメインの実装までを一通り押さえています。次の段階では、以下の4方向で厚みを出すのが有効です。

- 現在の内容をさらに深掘りする
- システム開発でよくある処理をDDDではどう扱うかを、サンプルコード付きで解説する
- DBや外部システム連携をどう設計するかを整理する
- 実務を意識したディレクトリ構造を示す

この4つを入れると、学習者が「理論だけで終わらず、プロジェクトへ持ち込める」状態に近づきます。

## 1. 現在の内容をさらに深掘りする提案

### 1-1. Value Object をもう一段深くする

現在は `TaskTitle` を中心に説明していますが、次の観点を足すと理解が深まります。

- 複数フィールドを持つValue Objectの例
- 正規化と表示形式の責務をどこまで持たせるか
- `Eq` と `Hash` を使う理由
- シリアライズ都合とValue Objectの純粋性の折り合い

追加候補の章:

- `複合Value Objectをどう設計するか`
- `Value Objectに変換責務を持たせすぎない`

### 1-2. Entity と Aggregate の境界を具体例で深掘りする

今は `Task` 単体をAggregate Rootとして扱っています。ここに「チェックリスト項目」や「担当者」を追加し、どこまでを同一Aggregateに入れるべきかを比較すると、Aggregate設計の勘所が伝わりやすくなります。

深掘り例:

- `Task` と `ChecklistItem` を同一Aggregateにする場合
- `Task` と `Assignee` をID参照に留める場合
- Aggregateを大きくしすぎたときの問題

### 1-3. Domain Service を必要になる瞬間まで持っていく

現状では「まだ不要」という説明が中心です。これは正しいですが、学習者によっては必要になるケースも見たいはずです。そこで、あえて `Task` 単体では判断できないルールを1つ入れるとよいです。

例:

- 1ユーザーが同時に持てる未完了タスク数の上限
- 締切超過タスクの自動エスカレーション条件

これにより、EntityとDomain Serviceの責務境界を実例で示せます。

### 1-4. エラー設計を拡張する

今は `TaskError` と `TaskApplicationError` が入門として十分ですが、もう一歩進めると次の観点が扱えます。

- ドメインエラーとインフラエラーの分離
- ユーザー向けメッセージと内部エラーの分離
- HTTP APIやCLIへ変換するときのマッピング方針

これは実務への接続が非常に良いテーマです。

## 2. システム開発でよくある処理をDDDではどうやるか

次の種別の処理は、学習者が実務でほぼ確実に遭遇します。現在のタスク管理サンプルに追加しやすい題材でもあります。

### 2-1. 一覧取得と検索

学習者はまず「登録・更新は分かったが、一覧画面はどうするのか」で止まりやすいです。ここでは書き込みモデルと読み取りモデルの違いも見せやすいです。

入れるとよい内容:

- 一覧取得は常にRepositoryでAggregateを返すべきか
- 画面表示用DTOとドメインモデルの違い
- ページング・並び替え・検索条件をどこで扱うか

サンプルコード候補:

```rust
pub struct FindTasksQuery {
    pub status: Option<TaskStatusDto>,
    pub page: u64,
    pub per_page: u64,
}

pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub status: TaskStatusDto,
}
```

このテーマでは「一覧取得はQuery側に寄せ、書き込み用Repositoryとは分けることがある」という説明もできます。

### 2-2. 削除と論理削除

システム開発では削除処理は頻出です。ここはDDDで特に誤解されやすいポイントです。

扱うとよい論点:

- 本当に物理削除してよいのか
- 削除ではなく `archive` や `deactivate` として表現すべきか
- ドメイン上意味のある状態遷移として扱うべきか

サンプルコード候補:

```rust
pub enum TaskStatus {
    Open,
    Completed,
    Archived,
}

impl Task {
    pub fn archive(&mut self) -> Result<(), TaskError> {
        if self.status == TaskStatus::Archived {
            return Err(TaskError::TaskAlreadyArchived);
        }

        self.status = TaskStatus::Archived;
        Ok(())
    }
}
```

### 2-3. 一括更新

実務では「複数件まとめて完了」「期限切れをまとめて更新」のような処理が多いです。ここはトランザクションやAggregate境界の説明につなげやすいです。

扱うとよい論点:

- 一括更新を1つのユースケースとしてどう表すか
- 複数Aggregateをまとめて更新してよいのか
- 失敗時のロールバックをどこで扱うか

### 2-4. 認証済みユーザー文脈の扱い

Webアプリや業務システムでは、現在ユーザーの情報を使う処理が多いです。DDDではこれをドメインルールにどう渡すかが重要です。

扱うとよい論点:

- `CurrentUser` のような概念をApplication Serviceへ渡す
- 認可判断をどこで行うか
- インフラ依存な認証情報をドメイン層へ持ち込まない

### 2-5. ドメインイベント

「完了したら通知する」「作成したら監査ログを残す」はよくある処理です。これを直接EntityやApplication Serviceへ埋め込むと責務が濁ります。

サンプルコード候補:

```rust
pub enum TaskEvent {
    TaskCreated { task_id: TaskId },
    TaskCompleted { task_id: TaskId },
}
```

これを足すことで、通知・監査・検索インデックス更新などへ自然に話を広げられます。

## 3. DBや外部システムとの連携をどう扱うか

このテーマは、学習者にとって「DDDが実務で使えるのか」を左右する重要な部分です。今後はここをもっと厚くした方がよいです。

### 3-1. Rustで現実的に使う技術スタック例を示す

抽象論だけではなく、次のような現実的な候補を示すと読者が動きやすくなります。

- `sqlx`: SQLを明示的に書きたい場合
- `SeaORM`: ORMベースで進めたい場合
- `tokio-postgres`: 低レベル寄りに制御したい場合
- `serde`: DTO変換や入出力で使用

重要なのは「どのライブラリを使うか」以上に、「それらをどの層に閉じ込めるか」です。

### 3-2. DBモデルとドメインモデルの変換例を入れる

これは今の本でまだ弱い部分です。具体例を入れると理解が一気に進みます。

サンプルコード候補:

```rust
pub struct TaskRow {
    pub id: String,
    pub title: String,
    pub status: String,
}

impl TryFrom<TaskRow> for Task {
    type Error = TaskError;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        let id = TaskId::new(row.id)?;
        let title = TaskTitle::new(row.title)?;
        let status = match row.status.as_str() {
            "open" => TaskStatus::Open,
            "completed" => TaskStatus::Completed,
            _ => return Err(TaskError::InvalidTaskStatus),
        };

        Ok(Task::restore(id, title, status))
    }
}
```

この例があると、永続化モデルの文字列やNULLと、ドメイン側の厳密な型の違いが説明しやすくなります。

### 3-3. トランザクション境界の説明を足す

アプリケーション層の章を深めるなら、トランザクションを避けて通れません。

扱うとよい論点:

- トランザクションはApplication Service側で張ることが多い
- 1ユースケース = 1トランザクションを基本にする
- 複数Repositoryをまたぐときの扱い

### 3-4. Query と Command でDBアクセスを分ける話を追加する

一覧画面や管理画面では、ドメインモデルを経由しない読み取り最適化が必要になることがあります。ここでCQRSを前面に出しすぎなくても、「読み取りは別モデルでもよい」という話を足すだけで実務感が出ます。

### 3-5. 外部API・メッセージングとの連携も軽く触れる

DBだけでなく、メール送信、通知、決済APIなどとの連携も現実には多いです。

扱うとよい論点:

- 外部API呼び出しはInfrastructureに閉じる
- ドメイン層はHTTPクライアントを知らない
- 必要ならPort/Adapterの形で抽象化する

## 4. プロジェクトのディレクトリ構造はどうするべきか

これは実務で非常に重要です。DDDを説明しても、読者は結局「ファイルをどこに置くのか」で詰まりやすいからです。

### 4-1. 今の学習用構成の良さを明示する

現在の構成は、学習用としてかなり良いです。

- `contents/`: 本文
- `src/domain`: ドメインモデル
- `src/application`: ユースケース
- `src/infrastructure`: 永続化などの技術詳細

まずはこの形で十分だと明示すると、学習者が過度に複雑な構成へ走りにくくなります。

### 4-2. 実務向けの単一クレート構成例を示す

小中規模の業務アプリでは、まずは単一クレートで十分なことが多いです。

```text
src/
  domain/
    task/
      mod.rs
      task.rs
      task_id.rs
      task_title.rs
      task_error.rs
    shared/
  application/
    task/
      create_task.rs
      complete_task.rs
      find_tasks.rs
  infrastructure/
    persistence/
      task_repository.rs
      task_row.rs
    external/
  presentation/
    http/
    cli/
  lib.rs
  main.rs
```

この例があると、章ごとの概念が実ファイル配置へ落ちやすくなります。

### 4-3. ワークスペース構成に進む条件も示す

大きめのプロジェクトでは、Cargo workspaceも視野に入ります。ただし、最初から分けすぎると辛いです。

ワークスペースを検討する条件:

- APIサーバーとバッチを分けたい
- ドメインを複数バイナリで共有したい
- プレゼンテーション層の依存が重い

例:

```text
crates/
  domain/
  application/
  infrastructure/
  api-server/
  batch/
```

ただし、この構成は「境界が見えてから」導入する方がよく、学習初期に無理に分ける必要はないと明記した方がよいです。

## 5. 追加すると価値が高い具体的な章案

優先度順で追加するなら、次の順がよいです。

### 優先度A

- `一覧取得と検索をどう設計するか`
- `削除ではなく状態遷移として表現する`
- `DBモデルとドメインモデルの変換`
- `実務で使うディレクトリ構造の例`

### 優先度B

- `Domain Serviceが必要になる具体例`
- `トランザクション境界とApplication Service`
- `外部API連携とInfrastructureの責務`

### 優先度C

- `ドメインイベントによる副作用の分離`
- `CQRSへ進む判断基準`
- `Cargo workspaceに分割するタイミング`

## 6. 実装面でのおすすめ拡張順

本文だけでなく、`src/` 側も一緒に育てた方が学習効果が高いです。おすすめの順番は次の通りです。

1. `TaskStatus` に `Archived` を追加する
2. `archive` ユースケースを追加する
3. 一覧取得用の `TaskSummary` と Query を追加する
4. `TaskRow` のような永続化モデルを追加する
5. InMemory に加えて DB を想定した Repository 実装例を追加する
6. 必要になったら Domain Event を導入する

この順番なら、今のサンプルを壊さず、章の追加とコード追加を並行しやすいです。

## 7. 提案の結論

今後コンテンツを濃くするなら、単にDDD用語を増やすより、次の3点を強くすると全体の価値が上がります。

- 実務でよくある処理をDDDでどう扱うかを示す
- DBや外部システム連携との境界をコード付きで説明する
- ディレクトリ構造まで含めて、プロジェクトへ持ち込める形にする

現在の内容は入門としては十分に成立しています。次の段階では、学習者が「理解した」から「自分で組める」へ進めるように、実務的な題材とファイル配置の話を増やすのが最も効果的です。