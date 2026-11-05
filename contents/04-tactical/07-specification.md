# 第4章-7 Specificationで条件を表現する

業務ルールの中には、「あるオブジェクトが条件を満たすかどうか」を判定するものが多くあります。「期限切れのタスク」「今週対応すべきタスク」「アーカイブ可能なタスク」などです。Specification パターンは、こうした条件判断を独立したオブジェクトとして表現する方法です。

## なぜ条件をオブジェクトにするのか

条件判断がコードのあちこちに散らばると、同じルールが少しずつ違う形で何度も書かれるようになります。

```rust
// 一覧表示の処理
if task.status() == TaskStatus::Open && task.deadline() < today { /* ... */ }

// 通知の処理
if task.status() != TaskStatus::Completed && task.deadline() <= today { /* ... */ }
```

この2つは「期限切れ」を判定しているつもりですが、条件が微妙に違います。アーカイブ済みのタスクを含むのか、期限当日を含むのかが揃っていません。業務ルールが変わったとき、両方を正しく直せる保証もありません。

Specification として切り出すと、このルールに名前がつき、1箇所に集まります。

## Rustでの基本形

Rustでは、Specification をトレイトとして定義するのが自然です。

```rust
pub trait Specification<T> {
    fn is_satisfied_by(&self, candidate: &T) -> bool;
}
```

本書の `Task` には期限がないため、ここでは現在のモデルで作れる例として「名前を変更できるタスク」を考えます。

```rust
pub struct RenamableTask;

impl Specification<Task> for RenamableTask {
    fn is_satisfied_by(&self, task: &Task) -> bool {
        task.status() == TaskStatus::Open
    }
}
```

これだけでは `task.status() == TaskStatus::Open` と書くのとほとんど変わりません。Specification が価値を持つのは、条件が業務的な意味を持ち、複数の場所で使われ、組み合わせたくなったときです。

## 期限を追加した場合の例

タスクに期限(`TaskDeadline`)を追加したとします。「期限切れ」は、日付を受け取って判定する Specification として表現できます。

```rust
pub struct OverdueTask {
    today: NaiveDate,
}

impl OverdueTask {
    pub fn new(today: NaiveDate) -> Self {
        Self { today }
    }
}

impl Specification<Task> for OverdueTask {
    fn is_satisfied_by(&self, task: &Task) -> bool {
        task.status() == TaskStatus::Open
            && task
                .deadline()
                .is_some_and(|deadline| deadline.value() < self.today)
    }
}
```

ここで重要なのは、「今日」を Specification の外から渡していることです。内部で現在時刻を取得すると、テストのたびに結果が変わってしまいます。時刻のような外部の状態は引数として受け取ると、Specification を純粋な判定として保てます。

## 組み合わせる

Specification の利点の1つは、論理演算で組み合わせられることです。Rustではジェネリクスを使って、組み合わせを型として表現できます。

```rust
pub struct And<A, B>(A, B);

impl<T, A, B> Specification<T> for And<A, B>
where
    A: Specification<T>,
    B: Specification<T>,
{
    fn is_satisfied_by(&self, candidate: &T) -> bool {
        self.0.is_satisfied_by(candidate) && self.1.is_satisfied_by(candidate)
    }
}

pub struct Not<A>(A);

impl<T, A> Specification<T> for Not<A>
where
    A: Specification<T>,
{
    fn is_satisfied_by(&self, candidate: &T) -> bool {
        !self.0.is_satisfied_by(candidate)
    }
}
```

さらに拡張トレイトを用意すると、メソッドチェーンで書けます。

```rust
pub trait SpecificationExt<T>: Specification<T> + Sized {
    fn and<B: Specification<T>>(self, other: B) -> And<Self, B> {
        And(self, other)
    }

    fn not(self) -> Not<Self> {
        Not(self)
    }
}

impl<T, S: Specification<T>> SpecificationExt<T> for S {}
```

```rust
// 期限切れで、かつ高優先度のタスク
let urgent = OverdueTask::new(today).and(HighPriorityTask);

let urgent_tasks: Vec<&Task> = tasks
    .iter()
    .filter(|task| urgent.is_satisfied_by(task))
    .collect();
```

`And<OverdueTask, HighPriorityTask>` という具体的な型になるため、動的ディスパッチのコストはかかりません。

## 3つの使いどころ

Specification は、次の3つの目的で使われます。

### 1. 検証

オブジェクトが条件を満たすかを確認します。Entityのメソッド内やDomain Serviceでのルール判定に使います。

### 2. 選択

コレクションから条件を満たすものを選び出します。上の `filter` の例がこれにあたります。

### 3. 生成の条件

「この条件を満たすオブジェクトを作る」という要求を表します。実務で使う場面は限られます。

## Repository との関係

Specification を Repository の検索条件として使いたくなることがあります。

```rust
pub trait TaskRepository {
    fn find_satisfying(&self, spec: &dyn Specification<Task>) -> Vec<Task>;
}
```

この形は一見きれいですが、注意が必要です。`is_satisfied_by` は Rust のコードとして実行される判定なので、DB実装ではすべての行を読み込んでからメモリ上でフィルタすることになります。データ量が増えると、性能面で破綻します。

そのため、一覧取得や検索のように大量データを扱う場面では、Specification ではなく専用の検索条件型(本書の `FindTasksQuery` のようなもの)を読み取りモデル側に用意する方が現実的です。この判断は第5章-4の `一覧取得と検索をどう設計するか` で扱った考え方と同じです。

Specification は、「メモリ上にある少数のオブジェクトに対する業務判定」に使うのが最も扱いやすいと覚えておくとよいでしょう。

## Rustのクロージャで十分な場合

Rustではクロージャも `Fn(&Task) -> bool` として条件を表現できます。条件に業務上の名前が不要で、1箇所でしか使わないなら、クロージャで十分です。

```rust
let open_tasks: Vec<&Task> = tasks
    .iter()
    .filter(|task| task.status() == TaskStatus::Open)
    .collect();
```

Specification を struct として定義する価値があるのは、次のような場合です。

- 条件に業務上の名前がある(「期限切れ」「要対応」)
- 同じ条件を複数の場所で使う
- 条件自体をテストしたい
- 条件の組み合わせを業務担当者と議論したい

## よくある誤解

- 条件判断はすべて Specification にすべきだと思いがち
- Specification を使えば Repository の検索も抽象化できると考えがち
- 汎用的な `And`/`Or`/`Not` を作ることが Specification の本質だと思いがち

Specification の本質は組み合わせの仕組みではなく、業務上の条件に名前をつけ、1箇所に集めることです。組み合わせが不要なら、名前付きの struct と1つのメソッドだけで十分です。

次章では、Rustの型システムを使って状態遷移そのものをコンパイル時に検査する typestate パターンを見ます。
