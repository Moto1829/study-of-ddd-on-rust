# 第4章-8 typestateで状態遷移を型にする

本書の `Task` は、状態を `TaskStatus` という enum で持ち、不正な操作を実行時に `Result` で拒否しています。たとえば完了済みタスクの名前変更は `TaskError::TaskNotOpen` になります。

Rustでは、これをさらに一歩進めて「不正な操作をそもそも書けない」ようにできます。それが typestate パターンです。状態ごとに別の型を用意し、その状態で許される操作だけをメソッドとして提供します。

## 実行時チェックの限界

現在の実装を振り返ります。

```rust
pub fn rename(&mut self, new_title: TaskTitle) -> Result<(), TaskError> {
    if self.status != TaskStatus::Open {
        return Err(TaskError::TaskNotOpen {
            current: self.status,
        });
    }

    self.title = new_title;
    Ok(())
}
```

このコードは正しく動きますが、次のような弱点があります。

- 完了済みタスクに対して `rename` を呼ぶコードが、コンパイルを通ってしまう
- 呼び出し側は、成功するとわかっている場面でも `Result` を処理しなければならない
- ルール違反に気づくのは、テストか本番環境で実行したとき

## 状態ごとに型を分ける

typestate パターンでは、状態を型パラメータとして表現します。

```rust
use std::marker::PhantomData;

// 状態を表すマーカー型(値を持たない)
pub struct Open;
pub struct Completed;
pub struct Archived;

pub struct Task<S> {
    id: TaskId,
    title: TaskTitle,
    _state: PhantomData<S>,
}
```

`PhantomData<S>` は、実行時のメモリを消費せずに型パラメータ `S` を保持するための仕組みです。`Task<Open>` と `Task<Completed>` は、コンパイラにとってまったく別の型になります。

## 状態ごとに許される操作を定義する

それぞれの状態に対して、許される操作だけを `impl` ブロックで定義します。

```rust
// どの状態でも使える操作
impl<S> Task<S> {
    pub fn id(&self) -> &TaskId {
        &self.id
    }

    pub fn title(&self) -> &TaskTitle {
        &self.title
    }
}

// Open のときだけ使える操作
impl Task<Open> {
    pub fn new(id: TaskId, title: TaskTitle) -> Self {
        Self {
            id,
            title,
            _state: PhantomData,
        }
    }

    pub fn rename(&mut self, new_title: TaskTitle) {
        self.title = new_title;
    }

    pub fn complete(self) -> Task<Completed> {
        Task {
            id: self.id,
            title: self.title,
            _state: PhantomData,
        }
    }

    pub fn archive(self) -> Task<Archived> {
        Task {
            id: self.id,
            title: self.title,
            _state: PhantomData,
        }
    }
}

// Completed のときだけ使える操作
impl Task<Completed> {
    pub fn archive(self) -> Task<Archived> {
        Task {
            id: self.id,
            title: self.title,
            _state: PhantomData,
        }
    }
}

// Archived には状態変更の操作がない
```

ここで注目すべき点が3つあります。

1. `rename` は `Task<Open>` にしか定義されていない
2. `rename` は `Result` を返さない。Open であることが型で保証されているので、失敗しない
3. `complete` と `archive` は `self` を値で受け取り、新しい型を返す

3つ目が重要です。`complete(self)` は元の `Task<Open>` を消費します。そのため、完了後に古い `Task<Open>` を使い続けることは所有権のルールで禁止されます。

## コンパイラがルールを守らせる

この設計では、ルール違反がコンパイルエラーになります。

```rust
let task = Task::new(
    TaskId::new("task-1").unwrap(),
    TaskTitle::new("牛乳を買う").unwrap(),
);
let completed = task.complete();

// コンパイルエラー: Task<Completed> に rename メソッドはない
completed.rename(TaskTitle::new("卵を買う").unwrap());

// コンパイルエラー: Task<Completed> に complete メソッドはない(二重完了の防止)
completed.complete();

// コンパイルエラー: task は complete() で消費済み
task.rename(TaskTitle::new("卵を買う").unwrap());
```

`TaskAlreadyCompleted` や `TaskNotOpen` といったエラーは、そもそも発生しえなくなります。業務ルールが型の構造そのものに埋め込まれた状態です。

## 永続化との境界で enum に戻す

typestate には弱点もあります。Repository から取得したタスクが、どの状態かはコンパイル時にはわかりません。DBの `status` 列の値は実行時に決まるからです。

そのため、境界では状態を enum で包む必要があります。

```rust
pub enum AnyTask {
    Open(Task<Open>),
    Completed(Task<Completed>),
    Archived(Task<Archived>),
}
```

Repository はこの `AnyTask` を返し、Application Service で状態を確かめてから操作します。

```rust
pub fn rename_task(
    &mut self,
    task_id: TaskId,
    new_title: TaskTitle,
) -> Result<(), TaskApplicationError> {
    let task = self
        .repository
        .find_by_id(&task_id)
        .ok_or(TaskApplicationError::TaskNotFound)?;

    match task {
        AnyTask::Open(mut task) => {
            task.rename(new_title);
            self.repository.save(AnyTask::Open(task));
            Ok(())
        }
        AnyTask::Completed(_) | AnyTask::Archived(_) => {
            Err(TaskApplicationError::TaskNotOpen)
        }
    }
}
```

結局 `match` で実行時の分岐が必要になるので、「typestateを使っても実行時チェックはなくならない」と感じるかもしれません。ただし、チェックの場所が変わっています。

- enum 方式: ドメインメソッドの内部でチェックする。呼び出し側はチェックを意識しない
- typestate 方式: 境界(Application Service)でチェックする。一度 `Task<Open>` を取り出せば、以降のドメイン操作はすべて型で保証される

複数の操作を組み合わせる複雑な処理ほど、typestate の恩恵が大きくなります。

## enum 方式と typestate 方式の比較

| 観点 | enum 方式(本書のサンプル) | typestate 方式 |
|---|---|---|
| 不正な操作の検出 | 実行時(`Result`) | コンパイル時 |
| 状態ごとの操作の発見しやすさ | メソッドを読む必要がある | IDEの補完で許される操作だけが出る |
| 永続化との相性 | そのまま保存できる | `AnyTask` のような enum で包む必要がある |
| コレクションでの扱い | `Vec<Task>` で混在できる | `Vec<AnyTask>` が必要 |
| 状態の数が増えたとき | バリアントを追加する | 型と `impl` ブロックが増える |
| コードの量 | 少ない | 多い |

## いつ typestate を選ぶか

typestate は強力ですが、すべての状態遷移に使うべきではありません。次のような場合に検討価値があります。

- 状態によって許される操作が大きく異なる
- 不正な遷移が深刻な業務上の問題につながる(決済の二重実行、承認前の出荷など)
- 1つの処理の中で、複数の状態遷移を順番に行う
- 状態の数が少なく、遷移が単純である

逆に、次のような場合は enum 方式の方が扱いやすいです。

- 状態が多く、遷移が複雑に入り組んでいる
- ほとんどの処理が「取得 → 1回操作 → 保存」で完結する
- 状態に関係なく一覧で扱う場面が多い

本書のタスク管理アプリは、ほとんどのユースケースが「取得 → 1回操作 → 保存」で完結するため、enum 方式を採用しています。typestate は、Rustでドメインのルールをどこまで型に寄せられるかを示す選択肢として知っておくと、設計の幅が広がります。

## よくある誤解

- typestate を使えば実行時チェックが完全になくなると思いがち
- 状態遷移があるなら必ず typestate にすべきだと考えがち
- `PhantomData` は実行時のオーバーヘッドがあると思いがち

`PhantomData` はサイズ0の型で、実行時のコストはありません。ただし、型とコードの量は確実に増えます。typestate を選ぶかどうかは、コンパイル時の保証とコードの複雑さを比べて判断します。

次章では、これまで見てきたドメインモデルをユースケースとして組み立てる Application Service を扱います。
