# 第5章-9 認可をどの層で扱うか

実務のアプリケーションでは、「誰がその操作を実行してよいか」を判断する認可が必ず必要になります。認可はセキュリティの話であると同時に、業務ルールの話でもあります。そのため、DDDの層構造のどこに置くべきか迷いやすいテーマです。

## 認証と認可を分ける

まず、よく混同される2つの概念を分けておきます。

- **認証(Authentication)**: その利用者が誰なのかを確かめる。ログイン、トークンの検証など
- **認可(Authorization)**: その利用者がその操作をしてよいかを判断する

認証は、ほぼ間違いなく汎用サブドメインです。パスワードのハッシュ化、トークンの署名検証、セッション管理などは、業務に関係なく同じ仕組みで解決できます。外部の認証サービスやライブラリを使い、プレゼンテーション層かインフラ層で扱います。

難しいのは認可です。認可のルールには、技術的なものと業務的なものが混在しているからです。

## 認可ルールを2種類に分ける

認可のルールは、次の2種類に分けると置き場所が決めやすくなります。

### 1. 役割ベースの認可(アプリケーション層)

「管理者だけが全ユーザーのタスクを閲覧できる」のように、利用者の役割(ロール)だけで決まるルールです。対象のドメインオブジェクトの状態を見る必要がありません。

これはユースケースの実行可否を決めるものなので、Application Service の入り口で確認します。

```rust
pub fn list_all_users_tasks(
    &self,
    actor: &Actor,
) -> Result<Vec<TaskSummary>, TaskApplicationError> {
    if !actor.has_role(Role::Admin) {
        return Err(TaskApplicationError::Forbidden);
    }

    // ...
}
```

### 2. ドメインの状態に依存する認可(ドメイン層)

「タスクの所有者だけがタスクを完了できる」のように、対象オブジェクトの状態によって可否が決まるルールです。これは業務ルールそのものなので、ドメイン層に置きます。

```rust
impl Task {
    pub fn complete(&mut self, by: &UserId) -> Result<(), TaskError> {
        if self.owner_id != *by {
            return Err(TaskError::NotOwner);
        }

        match self.status {
            TaskStatus::Open => {
                self.status = TaskStatus::Completed;
                Ok(())
            }
            TaskStatus::Completed => Err(TaskError::TaskAlreadyCompleted),
            TaskStatus::Archived => Err(TaskError::TaskNotOpen {
                current: self.status,
            }),
        }
    }
}
```

ここで `complete` が実行者を引数に取っていることに注目してください。「誰が完了したか」は業務上の意味を持つ情報です。後から「担当者も完了できる」というルールが追加されても、`Task` の中で完結して変更できます。

## 判断の基準

迷ったときは、次の問いで判断します。

| 問い | Yes なら |
|---|---|
| 対象オブジェクトの状態を見ないと判断できないか | ドメイン層 |
| 業務担当者がそのルールを業務の言葉で説明できるか | ドメイン層 |
| 利用者の役割だけで判断できるか | アプリケーション層 |
| APIの種類や画面によって変わるか | アプリケーション層またはプレゼンテーション層 |
| トークンの有効性や署名の検証か | プレゼンテーション層またはインフラ層(認証) |

## 実行者を表す型

認可を扱うときは、「誰が操作しているか」をApplication Serviceへ明示的に渡すのが基本です。

```rust
pub struct Actor {
    user_id: UserId,
    roles: Vec<Role>,
}

impl Actor {
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    pub fn has_role(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }
}
```

プレゼンテーション層で認証を終えたら、その結果を `Actor` として組み立て、Application Service の引数に渡します。

```rust
pub fn complete_task(
    &mut self,
    actor: &Actor,
    task_id: impl Into<String>,
) -> Result<TaskOutput, TaskApplicationError> {
    let task_id = TaskId::new(task_id)?;
    let mut task = self
        .repository
        .find_by_id(&task_id)
        .ok_or_else(|| TaskApplicationError::TaskNotFound {
            task_id: task_id.value().to_owned(),
        })?;

    task.complete(actor.user_id())?;
    // ...
}
```

グローバル変数やスレッドローカルから「現在のユーザー」を取り出す設計は避けた方が安全です。依存関係が見えなくなり、テストで実行者を差し替えるのも難しくなります。

## 存在を隠すか、拒否を伝えるか

認可に失敗したとき、利用者に何を返すかも設計判断です。

- 「権限がありません」と返すと、そのタスクが存在することは相手に伝わる
- 「見つかりません」と返すと、存在自体を隠せる

他人のタスクのIDを推測して存在を確認されることが問題なら、Repository の検索段階で所有者を条件に含め、見つからないものとして扱う方法があります。

```rust
fn find_by_id_and_owner(&self, task_id: &TaskId, owner_id: &UserId) -> Option<Task>;
```

これはセキュリティ要件による判断なので、ドメインのルールとは分けて考えます。ドメイン層の `NotOwner` エラーは「所有者以外は完了できない」という業務ルールを表し、それを利用者にどう見せるかはプレゼンテーション層が決めます。

## 一覧取得での認可

一覧取得では、1件ずつドメインオブジェクトを読み込んで認可を確認すると性能が出ません。「自分のタスクだけを表示する」のような条件は、読み取りモデルのクエリ条件に組み込みます。

```rust
pub struct FindTasksQuery {
    pub owner_id: String,
    pub status: Option<TaskStatusDto>,
    pub page: u64,
    pub per_page: u64,
}
```

このとき、`owner_id` を利用者からの入力ではなく、認証済みの `Actor` から設定することが重要です。利用者が任意の `owner_id` を指定できると、他人のタスクを一覧できてしまいます。

## よくある誤解

- 認可はすべてミドルウェアやフレームワークで処理すべきだと思いがち
- 認可はセキュリティの話なので、ドメインモデルとは無関係だと考えがち
- 認可チェックをドメイン層に置くと、ドメインが認証の仕組みに依存すると思いがち

ドメイン層に置くのは「所有者だけが完了できる」という業務ルールであって、トークンやセッションの仕組みではありません。ドメイン層は `UserId` という値を受け取るだけで、それがどうやって確かめられたかは知りません。

次章では、ここまでの層構造に沿って、どの層で何をテストするかを整理します。
