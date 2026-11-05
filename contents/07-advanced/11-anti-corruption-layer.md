# 第7章-11 腐敗防止層で外部モデルを翻訳する

外部のシステムやAPIと連携すると、相手のモデルが自分たちのモデルに入り込んできます。相手のフィールド名、状態の表し方、エラーの形式などが、気づかないうちに自分たちのコードに広がっていきます。腐敗防止層(Anti-Corruption Layer、ACL)は、この侵入を防ぐために、外部のモデルを自分たちのモデルへ翻訳する層です。

第5章-6の `外部API連携とInfrastructureの責務` では、外部API呼び出しをインフラ層に置くことを扱いました。この章では、そこからさらに進めて「外部のモデルをどう翻訳するか」に焦点を当てます。

## 何が腐敗するのか

外部の課題管理サービスからタスクを取り込む機能を追加するとします。外部APIは、次のようなJSONを返します。

```json
{
  "issue_key": "PROJ-123",
  "summary": "牛乳を買う",
  "state": 3,
  "resolution": "Done",
  "fields": {
    "custom_field_10021": "2026-11-30"
  }
}
```

このデータをそのまま使うと、次のような問題が起きます。

- `state: 3` が何を意味するのか、コードのあちこちで判断が必要になる
- `resolution` が `"Done"` か `"Won't Do"` かによって、完了とみなすかどうかが変わる
- `custom_field_10021` という、自分たちの業務とは無関係な名前が広がる
- 外部APIの仕様が変わると、自分たちのドメインロジックまで修正が必要になる

これが「腐敗」です。自分たちのユビキタス言語が、外部システムの言葉で汚染されていきます。

## ACL の構成

ACL は、次の3つの要素で構成するのが一般的です。

```text
[外部API] ←→ [Client] → [外部DTO] → [Translator] → [ドメインモデル]
                   └────────── Adapter ──────────┘
```

| 要素 | 役割 | 層 |
|---|---|---|
| Client | 外部APIとの通信を行う。HTTPやSDKの詳細を扱う | インフラ層 |
| 外部DTO | 外部APIのデータ構造をそのまま写した型 | インフラ層 |
| Translator | 外部DTOをドメインモデルへ変換する | インフラ層 |
| Adapter | 上の3つをまとめ、ドメイン層で定義したトレイトを実装する | インフラ層 |

ドメイン層から見えるのは、Adapter が実装するトレイトだけです。

## ドメイン層: 自分たちの言葉でポートを定義する

まず、ドメイン層(またはアプリケーション層)に、自分たちの言葉でトレイトを定義します。

```rust
// domain 層
pub struct ImportedTask {
    pub source_id: ExternalTaskRef,
    pub title: TaskTitle,
    pub status: TaskStatus,
}

pub trait ExternalTaskSource {
    fn fetch(&self, reference: &ExternalTaskRef) -> Result<ImportedTask, ExternalTaskSourceError>;
}
```

ここには `issue_key` も `state: 3` も出てきません。外部サービスの名前すら出てきません。ドメイン層が知っているのは、「外部からタスクを取り込める」という事実だけです。

## インフラ層: 外部DTOを定義する

外部APIのデータ構造は、インフラ層でそのまま写し取ります。

```rust
// infrastructure 層
#[derive(serde::Deserialize)]
struct ExternalIssueDto {
    issue_key: String,
    summary: String,
    state: u8,
    resolution: Option<String>,
}
```

この型は外部APIの都合で設計されているため、ドメイン層へは一切出しません。モジュールの外から見えないように、`pub` を付けないのがポイントです。

## Translator: 翻訳のルールを集める

外部の概念を自分たちの概念へ翻訳するルールを、1箇所に集めます。Rustでは `TryFrom` が自然です。

```rust
// infrastructure 層
impl TryFrom<ExternalIssueDto> for ImportedTask {
    type Error = ExternalTaskSourceError;

    fn try_from(dto: ExternalIssueDto) -> Result<Self, Self::Error> {
        let title = TaskTitle::new(dto.summary)
            .map_err(|error| ExternalTaskSourceError::InvalidData(error.to_string()))?;

        let status = translate_status(dto.state, dto.resolution.as_deref())?;

        Ok(ImportedTask {
            source_id: ExternalTaskRef::new(dto.issue_key),
            title,
            status,
        })
    }
}

/// 外部サービスの状態を、タスク管理アプリの状態へ翻訳する
fn translate_status(
    state: u8,
    resolution: Option<&str>,
) -> Result<TaskStatus, ExternalTaskSourceError> {
    match (state, resolution) {
        (1 | 2, _) => Ok(TaskStatus::Open),
        (3, Some("Done")) => Ok(TaskStatus::Completed),
        // 「対応しない」で閉じられた課題は、完了ではなくアーカイブとして扱う
        (3, Some("Won't Do")) => Ok(TaskStatus::Archived),
        (state, resolution) => Err(ExternalTaskSourceError::UnknownStatus {
            state,
            resolution: resolution.map(str::to_owned),
        }),
    }
}
```

`translate_status` には、「外部サービスの `Won't Do` は、自分たちの業務ではアーカイブにあたる」という翻訳の判断が書かれています。これは業務上の判断なので、業務担当者と相談して決めるべき内容です。ACL は、こうした翻訳の判断を1箇所に閉じ込める場所でもあります。

また、未知の状態を受け取ったときにエラーにしている点も重要です。外部サービスが新しい状態を追加したとき、黙って `Open` 扱いにするより、エラーとして気づける方が安全です。

## Adapter: すべてをまとめる

最後に、Client と Translator をまとめて、ドメイン層のトレイトを実装します。

```rust
// infrastructure 層
pub struct ExternalIssueTrackerAdapter<C: HttpClient> {
    client: C,
    base_url: String,
}

impl<C: HttpClient> ExternalTaskSource for ExternalIssueTrackerAdapter<C> {
    fn fetch(&self, reference: &ExternalTaskRef) -> Result<ImportedTask, ExternalTaskSourceError> {
        let url = format!("{}/issues/{}", self.base_url, reference.value());

        let dto: ExternalIssueDto = self
            .client
            .get_json(&url)
            .map_err(|error| ExternalTaskSourceError::Unavailable(error.to_string()))?;

        ImportedTask::try_from(dto)
    }
}
```

Application Service は、`ExternalTaskSource` トレイトだけに依存します。

```rust
pub fn import_task(&mut self, reference: ExternalTaskRef) -> Result<TaskOutput, TaskApplicationError> {
    let imported = self.external_source.fetch(&reference)?;
    let task = Task::restore(self.id_generator.generate(), imported.title, imported.status);
    // ...
}
```

## ACL のテスト

ACL は翻訳のルールを持つため、テストの価値が高い部分です。特に `translate_status` のような関数は、外部APIを呼ばずに単体でテストできます。

```rust
#[test]
fn wont_do_resolution_is_translated_to_archived() {
    assert_eq!(translate_status(3, Some("Won't Do")), Ok(TaskStatus::Archived));
}

#[test]
fn unknown_state_is_rejected() {
    assert!(translate_status(9, None).is_err());
}
```

外部APIの実際のレスポンスをJSONファイルとして保存しておき、それを `ExternalIssueDto` にデシリアライズして変換するテストを書くと、外部APIの仕様変更にも気づきやすくなります。

## 逆方向の翻訳

ACL は、外部から取り込むときだけでなく、外部へ送るときにも使います。たとえばタスクを完了したときに外部サービスへ通知するなら、ドメインモデルから外部DTOへの変換も ACL の中に置きます。

```rust
impl From<&Task> for ExternalIssueUpdateDto {
    fn from(task: &Task) -> Self {
        let (state, resolution) = match task.status() {
            TaskStatus::Open => (1, None),
            TaskStatus::Completed => (3, Some("Done".to_owned())),
            TaskStatus::Archived => (3, Some("Won't Do".to_owned())),
        };

        Self { state, resolution }
    }
}
```

`match` が網羅的なため、`TaskStatus` に新しい状態を追加すると、ここがコンパイルエラーになります。外部への翻訳を忘れることがありません。

## ACL を作るべきか

ACL には、型と変換コードが増えるというコストがあります。次のような場合に作る価値があります。

- 外部のモデルが、自分たちのコアドメインの言葉と大きく違う
- 外部のAPIが頻繁に変わる、または将来別のサービスに乗り換える可能性がある
- 外部のデータに、自分たちの不変条件を満たさない値が含まれうる
- 複数の外部サービスを、同じドメインの概念として扱いたい

逆に、外部のモデルが自分たちの業務にそのまま合っていて、コアドメインに関係しないなら、Conformist として外部の型をそのまま使う方が単純です。

## よくある誤解

- ACL は大規模なレガシーシステムとの連携だけに使うものだと思いがち
- 外部DTOに `serde` の属性を付けてフィールド名を変えれば、翻訳したことになると考えがち
- ACL を作れば、外部APIの変更の影響を完全に防げると思いがち

フィールド名を変えるだけでは、外部の概念(`state: 3` と `resolution` の組み合わせで状態を表すなど)はそのまま残ります。ACL の本質は、名前ではなく意味の翻訳です。また、外部APIの変更の影響は、ACL の中に閉じ込められるだけでなくなりはしません。ACL の価値は、修正箇所を1箇所に限定できることにあります。

次章では、複数の Aggregate を扱うときに避けて通れない、Aggregate 間の参照と結果整合性を見ます。
