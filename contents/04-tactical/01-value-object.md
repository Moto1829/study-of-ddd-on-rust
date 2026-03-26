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

## よくある誤解

- newtypeで包めば自動的にValue Objectになるわけではない
- バリデーションが外に残っているなら、まだValue Objectとして責務を取り切れていない
- DBやAPIの都合だけで作られた型は、必ずしもValue Objectではない

大事なのは「意味」と「制約」をその型が引き受けていることです。単なるラッパーを増やすこと自体が目的ではありません。

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

## 実務での注意点

- 表示用文字列と保存用文字列で正規化方針が違う場合は、その責務をValue Objectに持たせるか検討する
- Validation error を文字列で返すだけでなく、列挙型で分類すると扱いやすい
- JSONやDBとの変換都合をValue Objectに持ち込みすぎない

次章以降では、この `TaskTitle` を使って Entity を組み立てていきます。