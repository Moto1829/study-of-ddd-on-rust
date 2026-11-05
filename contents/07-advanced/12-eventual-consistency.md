# 第7章-12 Aggregate間の参照と結果整合性

本書のタスク管理アプリには、Aggregate が `Task` の1つしかありません。しかし実務では、複数の Aggregate が互いに関係しながら動きます。第4章-3の `Aggregateと整合性境界` では「別オブジェクトとの参照はIDだけで十分ではないか」という問いを示しました。この章では、その考え方を具体的に掘り下げます。

## 例: プロジェクトとタスク

タスク管理アプリに「プロジェクト」を追加するとします。業務ルールは次の通りです。

- タスクは1つのプロジェクトに属する
- プロジェクトには、未完了タスクの数を表示する
- プロジェクトをクローズすると、属するタスクはすべてアーカイブされる

素直に考えると、`Project` が `Task` のリストを持つ設計が思い浮かびます。

```rust
pub struct Project {
    id: ProjectId,
    name: ProjectName,
    tasks: Vec<Task>, // タスクを直接持つ
}
```

この設計にはいくつかの問題があります。

- プロジェクトを読み込むたびに、すべてのタスクを読み込む必要がある
- タスクの名前を変えるだけで、プロジェクト全体を保存し直す必要がある
- 2人が同じプロジェクトの別々のタスクを同時に編集すると、競合する
- タスクが1000件あるプロジェクトでは、性能が破綻する

## Aggregate は ID で参照する

DDDの原則は、**別の Aggregate は ID で参照する**ことです。

```rust
pub struct Project {
    id: ProjectId,
    name: ProjectName,
    status: ProjectStatus,
}

pub struct Task {
    id: TaskId,
    project_id: ProjectId, // オブジェクトではなくIDで参照する
    title: TaskTitle,
    status: TaskStatus,
}
```

こうすると、`Project` と `Task` はそれぞれ独立して読み込み・保存できます。タスクの名前を変えても、プロジェクトには影響しません。

Rustでは、この原則が所有権と自然に一致します。`Project` が `Vec<Task>` を所有すると、タスクを変更するには `Project` への `&mut` 参照が必要になります。ID で参照すれば、それぞれの Aggregate が自分の状態だけを所有し、借用の問題も起きません。

## 1つのトランザクションで1つの Aggregate を更新する

ID で参照するようにすると、次の疑問が出てきます。「プロジェクトをクローズしたら、属するタスクをすべてアーカイブする」というルールは、どう守ればよいのでしょうか。

1つの方法は、1つのトランザクションでプロジェクトとすべてのタスクを更新することです。

```rust
pub fn close_project(&mut self, project_id: ProjectId) -> Result<(), ProjectApplicationError> {
    self.transaction.run(|| {
        let mut project = self.projects.find_by_id(&project_id)?;
        project.close()?;
        self.projects.save(&project)?;

        for mut task in self.tasks.find_by_project(&project_id)? {
            task.archive()?;
            self.tasks.save(&task)?;
        }
        Ok(())
    })
}
```

一見正しそうですが、DDDではこれを避けるのが原則です。**1つのトランザクションでは、1つの Aggregate だけを更新する**というルールがあるからです。

複数の Aggregate を同じトランザクションで更新すると、次の問題が起きます。

- タスクが多いと、トランザクションが長くなり、ロックの競合が増える
- 誰かがタスクを1件編集しているだけで、プロジェクトのクローズが失敗する
- Aggregate が整合性の境界であるという設計の意味が薄れる

## 結果整合性

そこで使うのが**結果整合性(Eventual Consistency)** です。すべてを即座に整合させるのではなく、「最終的には整合した状態になる」ことを保証する考え方です。

```text
1. プロジェクトをクローズする(トランザクション1)
   → ProjectClosed イベントを発行する

2. ProjectClosed イベントを受け取る
   → 属するタスクを1件ずつアーカイブする(トランザクション2, 3, 4, ...)
```

```rust
// Project Aggregate
impl Project {
    pub fn close(&mut self) -> Result<ProjectEvent, ProjectError> {
        if self.status == ProjectStatus::Closed {
            return Err(ProjectError::AlreadyClosed);
        }
        self.status = ProjectStatus::Closed;
        Ok(ProjectEvent::ProjectClosed {
            project_id: self.id.clone(),
        })
    }
}

// イベントハンドラ(アプリケーション層)
pub fn on_project_closed(&mut self, project_id: &ProjectId) -> Result<(), TaskApplicationError> {
    for task_id in self.tasks.find_open_ids_by_project(project_id)? {
        let mut task = self.tasks.find_by_id(&task_id)?;
        task.archive()?;
        self.tasks.save(&task)?;
    }
    Ok(())
}
```

この方式では、プロジェクトがクローズされてから、すべてのタスクがアーカイブされるまでの間に、わずかな時間差が生まれます。その間は「クローズ済みのプロジェクトに、未アーカイブのタスクがある」状態になります。

## 時間差を許容できるかは業務で決まる

結果整合性を採用するかどうかは、技術ではなく業務の判断です。業務担当者に次のように聞いてみます。

「プロジェクトをクローズしたあと、数秒間だけタスクがアーカイブされていない状態があっても問題ありませんか」

多くの場合、答えは「問題ない」です。人間の業務では、もともと即時の整合性が保たれていないことが多いからです。紙の業務なら、プロジェクトの終了を決めてから各担当者に連絡が届くまで、数時間かかることもあります。

逆に、「残高がマイナスになってはいけない」のような、一瞬でも破られると問題になるルールは、1つの Aggregate の中で守る必要があります。

| ルール | 整合性の種類 | 守る場所 |
|---|---|---|
| タスクの名前は100文字以内 | 即時 | `TaskTitle` |
| 完了済みタスクは名前を変更できない | 即時 | `Task` |
| クローズしたプロジェクトのタスクはアーカイブされる | 結果整合 | イベントハンドラ |
| プロジェクトの未完了タスク数を表示する | 結果整合 | 読み取りモデル |

## どちらの Aggregate がルールを守るか

結果整合性の途中状態で、問題のある操作が行われる可能性もあります。たとえば、プロジェクトのクローズ直後、タスクがアーカイブされる前に、誰かがそのタスクを完了しようとするかもしれません。

この場合、次のどちらかで対応します。

- 途中状態での操作を許容する(完了されたタスクも、後からアーカイブされる)
- 操作の前に、関連する Aggregate の状態を確認する

後者の場合、Application Service でプロジェクトの状態を確認してから、タスクを操作します。

```rust
pub fn complete_task(&mut self, task_id: TaskId) -> Result<TaskOutput, TaskApplicationError> {
    let mut task = self.tasks.find_by_id(&task_id)?;
    let project = self.projects.find_by_id(task.project_id())?;

    if project.is_closed() {
        return Err(TaskApplicationError::ProjectClosed);
    }

    task.complete()?;
    // ...
}
```

ただし、これも完全な保証ではありません。確認してから保存するまでの間に、プロジェクトがクローズされる可能性があるからです。完全な保証が必要なら、そのルールは1つの Aggregate の中に収めるべきであり、Aggregate の境界を見直すサインです。

## 境界を見直すべきとき

結果整合性で無理が生じる場合は、Aggregate の境界が業務に合っていない可能性があります。

- 2つの Aggregate を、ほぼ常に同時に更新している
- 結果整合性の途中状態が、業務上どうしても許されない
- イベントハンドラの補正処理が複雑になりすぎている

こうした場合は、2つを1つの Aggregate にまとめることを検討します。逆に、1つの Aggregate の中で、ほとんど関係なく更新される部分があるなら、分割を検討します。

## よくある誤解

- 結果整合性は、整合性を諦めることだと思いがち
- 結果整合性には、メッセージキューなどの大掛かりな仕組みが必要だと考えがち
- 関連するオブジェクトは、参照でつながっている方が便利だと思いがち

結果整合性は、整合性を諦めるのではなく、「いつ整合するか」を業務に合わせて設計することです。また、同じプロセス内でイベントを処理するだけでも結果整合性は実現できます。ただし、イベントを確実に届ける仕組みは必要になります。これについては `Outboxでイベントを確実に届ける` で扱います。

次章では、同じ Aggregate を複数の利用者が同時に更新したときの競合を、楽観ロックで検出する方法を見ます。
