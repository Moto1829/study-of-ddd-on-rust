# 第4章-1 Value Objectを実装する

Value Objectは、RustでDDDを始めるときにもっとも効果がわかりやすい要素です。理由は単純で、プリミティブ型のままでは表現できない「意味」と「制約」を、型として閉じ込められるからです。

## なぜ String のままでは足りないのか

タスク名を `String` で受け取る設計にすると、空文字、空白だけの文字列、異常に長い文字列がそのまま流れ込みます。後からチェックを足しても、すべての経路で漏れなく検証するのは困難です。

そこで `TaskTitle` という型を作り、生成時にのみ検証を通すようにします。

## サンプル実装

たとえば、`TaskTitle` は次のように定義できます。

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTitle(String);

impl TaskTitle {
    pub fn new(value: impl Into<String>) -> Result<Self, TaskError> {
        let value = value.into();
        let normalized = value.trim().to_owned();
        let actual = normalized.chars().count();

        if normalized.is_empty() {
            return Err(TaskError::EmptyTaskTitle);
        }

        if actual > 100 {
            return Err(TaskError::TaskTitleTooLong {
                max: 100,
                actual,
            });
        }

        Ok(Self(normalized))
    }
}
```

ここで重要なのは、`TaskTitle` の内部表現が `String` であることではなく、妥当性検証済みの値しか作れないことです。

## この設計で得られること

- 妥当性検証の場所が1箇所に集まる
- ドメイン上の意味が型名として現れる
- Entity側では「検証済みの値を受け取る」ことに集中できる

ここで重要なのは、Value Objectが単なる入力チェックの入れ物ではないことです。Value Objectは「この値が何を意味するか」を型で固定し、その意味に付随するルールを閉じ込めます。

たとえば `TaskTitle` を `String` のまま扱うと、呼び出し側は毎回「これは未検証の生文字列なのか」「すでに画面表示向けに整形済みなのか」を文脈で推測しなければなりません。Value Objectにしておけば、この曖昧さを消せます。

## どこまでを Value Object の責務にするか

Value Objectに何でも押し込むと、かえって責務が曖昧になります。実務では次のように切り分けると考えやすくなります。

- ドメイン上必須の正規化はValue Objectが持つ
- 画面表示専用の整形はValue Objectの外へ出す
- JSONやDB都合の変換はInfrastructureやDTO変換側へ寄せる

たとえば、前後の空白除去は「同じタイトルを別物として扱わない」ためのドメイン上の都合として `TaskTitle::new` に入れやすいです。一方で、一覧画面で20文字に省略する処理はドメインルールではないため、Value Objectへ持ち込まない方が自然です。

この切り分けが曖昧だと、Value Objectが便利関数の集まりになりやすくなります。そうなると不変条件を守る中心ではなく、文字列操作の置き場になってしまいます。

## 複数フィールドを持つ Value Object

Value Objectは1つの文字列や整数だけを包むとは限りません。複数の値がまとまって初めて意味を持つなら、それ全体を1つのValue Objectとして扱えます。

たとえば期限を「日付」と「時刻」に分けて受け取る外部入力があっても、ドメイン上は `TaskDeadline` という1つの概念として扱う方が自然なことがあります。

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskDeadline {
    at: chrono::DateTime<chrono::Utc>,
}

impl TaskDeadline {
    pub fn new(at: chrono::DateTime<chrono::Utc>) -> Result<Self, TaskError> {
        if at.year() < 2000 {
            return Err(TaskError::InvalidDeadline);
        }

        Ok(Self { at })
    }

    pub fn is_overdue(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        self.at < now
    }
}
```

ここで伝えたいのは `chrono` を使うこと自体ではありません。複数のプリミティブ値や外部ライブラリ型を、そのままEntityへ広げるのではなく、「業務上ひとまとまりの概念」として閉じ込められる、という点が重要です。

## Eq と Hash が効いてくる場面

RustでValue Objectを作る利点の1つは、比較可能性を明示しやすいことです。`PartialEq` や `Eq` があると、同じ意味の値であるかを自然に比較できます。さらに `Hash` を実装できれば、重複排除や集合演算にも使いやすくなります。

たとえばタグ名やメールアドレスのように「同じ値なら同一視したい」概念では、Value Objectが `HashSet` の要素になれることが設計上の意味を持ちます。

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TagName(String);
```

このときも、単にderiveできるから付けるのではなく、「同一性を値で判断したいか」というドメイン上の意図が先にあります。

## よくある誤解

- newtypeで包めば自動的にValue Objectになるわけではない
- バリデーションが外に残っているなら、まだValue Objectとして責務を取り切れていない
- DBやAPIの都合だけで作られた型は、必ずしもValue Objectではない

大事なのは「意味」と「制約」をその型が引き受けていることです。単なるラッパーを増やすこと自体が目的ではありません。

## 失敗しやすい設計

学習段階で特に起きやすいのは、次の2つです。

1つ目は、コンストラクタを通さず `pub struct TaskTitle(pub String);` のように公開してしまうことです。これでは不正値をいつでも作れてしまい、Value Objectで守りたかった不変条件が崩れます。

2つ目は、Value Objectに外部都合を背負わせすぎることです。たとえば `to_json`, `from_row`, `from_query_param` のような変換責務まで集めると、その型がドメインの中心なのか入出力の便利窓口なのかが曖昧になります。

迷ったときは、「この処理は業務上の意味や制約に関係するか」で判断すると整理しやすくなります。

## テストで不変条件を確認する

テストでは、`TaskTitle` の不変条件を次のように確認できます。

```rust
#[test]
fn task_title_rejects_empty_input() {
    let result = TaskTitle::new("   ");

    assert_eq!(result, Err(TaskError::EmptyTaskTitle));
}

#[test]
fn task_title_rejects_too_long_input() {
    let title = "a".repeat(101);
    let result = TaskTitle::new(title);

    assert_eq!(
        result,
        Err(TaskError::TaskTitleTooLong {
            max: 100,
            actual: 101,
        })
    );
}
```

Value Objectのテストは小さく、速く、仕様が読み取れる形にしておくと、テキスト本文とも対応づけやすくなります。

さらに一歩進めるなら、通るケースも明示しておくと仕様の輪郭がはっきりします。

```rust
#[test]
fn task_title_trims_surrounding_spaces() {
    let title = TaskTitle::new("  レポート作成  ").unwrap();

    assert_eq!(title.as_str(), "レポート作成");
}
```

拒否する入力だけでなく、どう正規化するかまでテストで示せると、読み手は「禁止ルール」と「許容ルール」を区別して理解できます。

## 実務での注意点

- 表示用文字列と保存用文字列で正規化方針が違う場合は、その責務をValue Objectに持たせるか検討する
- Validation error を文字列で返すだけでなく、列挙型で分類すると扱いやすい
- JSONやDBとの変換都合をValue Objectに持ち込みすぎない

設計判断に迷ったら、次の順で確認するとぶれにくくなります。

1. その値に業務上の名前があるか
2. その値に守るべき制約があるか
3. その制約を生成時に閉じ込めたいか
4. 比較や再利用の単位として独立させる意味があるか

この4つに答えられるなら、その値はValue Objectとして切り出す価値があります。

次章以降では、この `TaskTitle` を使って Entity を組み立てていきます。