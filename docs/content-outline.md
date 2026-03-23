# RustにおけるDDD(ドメイン駆動開発)の実現方法

このドキュメントは、mdBookで公開することを前提にしたコンテンツ構成案です。Rustの言語特性を踏まえつつ、DDDの考え方をどのように実装へ落とし込むかを段階的に学べる構成を想定しています。

## コンテンツの狙い

- DDDの用語や原則を、Rustの型システムと所有権モデルの上でどう表現するかを整理する
- Entity、Value Object、Repository、Domain Serviceなどの典型的な要素をRustでどう設計するかを示す
- サンプルアプリケーションを通じて、戦術的DDDからアプリケーション層、永続化までのつながりを説明する
- 学習用途だけで終わらず、小規模な実装のひな形としても参照できる内容にする

## 想定読者

- Rustの基本構文、所有権、トレイト、モジュール分割をひと通り理解している人
- DDDの概要は聞いたことがあるが、実装レベルに落とし込めていない人
- 他言語のDDD実装経験をRustに持ち込みたい人

## 到達目標

- RustでDDDを実践する際のレイヤ分割と責務を説明できる
- 不変条件を型やコンストラクタで保護する設計ができる
- ドメインモデルと永続化モデルを分離する理由を説明できる
- 小規模な業務アプリケーションをDDDベースで組み立て始められる

## 全体構成案

### Part 1. 導入

#### 1. はじめに

- 本書の目的
- 対象読者
- RustとDDDの相性
- サンプルアプリケーションの概要

#### 2. なぜRustでDDDを学ぶのか

- DDDが解決したい問題
- Rustの型安全性とドメイン表現
- OOP前提のDDDをそのまま持ち込まないための視点

### Part 2. 基礎整理

#### 3. DDDの基本用語をRust目線で整理する

- Entity
- Value Object
- Aggregate
- Repository
- Domain Service
- Application Service
- Bounded Context

#### 4. Rustでドメインを表現するための前提知識

- structとenumの使い分け
- newtype pattern
- トレイトによる振る舞いの抽象化
- Resultによるドメインエラー表現
- 所有権とライフタイムが設計に与える影響

### Part 3. 戦術的DDDをRustで実装する

#### 5. Value Objectを実装する

- 妥当性検証をコンストラクタに閉じ込める
- 不変オブジェクトとして扱う
- 比較可能性と表示の設計

#### 6. Entityを実装する

- 識別子の扱い
- 可変性をどこまで許すか
- 振る舞い中心でメソッドを定義する

#### 7. Aggregateと整合性境界

- Aggregate Rootの役割
- 不変条件をどこで守るか
- 複数Entityをまたぐ更新の考え方

#### 8. Domain Serviceの使いどころ

- EntityやValue Objectに置けない振る舞い
- 過剰なService化を避ける基準

### Part 4. アプリケーション層とインフラ層

#### 9. Application Serviceでユースケースを表現する

- 入出力DTOの扱い
- トランザクション境界の考え方
- ドメイン層との依存関係

#### 10. Repositoryをトレイトで定義する

- 永続化の抽象化
- インメモリ実装とDB実装の切り替え
- 非同期化する場合の注意点

#### 11. Infrastructure層の実装方針

- Repository実装の配置
- ORMやSQLクレートとの付き合い方
- ドメインモデルと永続化モデルの分離

### Part 5. サンプルアプリケーション

#### 12. サンプル題材のドメインを定義する

- 例: 注文管理、在庫管理、タスク管理のいずれか
- ユビキタス言語の整理
- 主要なユースケースの洗い出し

#### 13. ドメインモデルを実装する

- Value Object
- Entity
- Aggregate
- ドメインエラー

#### 14. ユースケースを実装する

- 登録
- 更新
- 参照
- 状態遷移

#### 15. 永続化とテストを実装する

- インメモリRepositoryでのテスト
- DB接続を含むInfrastructure実装
- 結合テストの考え方

### Part 6. 発展事項

#### 16. RustでDDDを実践する際の悩みどころ

- 所有権とAggregate設計の折り合い
- トレイトの抽象化が過剰になる問題
- 非同期処理とドメイン層の境界

#### 17. CQRSやイベント駆動へ発展させるには

- 読み取りモデルの分離
- Domain Eventの扱い
- 将来的な拡張ポイント

#### 18. まとめ

- 本書で扱ったことの整理
- 実務に適用する際の第一歩
- 次に読むとよい資料

## mdBook向けのファイル構成案

```text
contents/
  SUMMARY.md
  01-introduction.md
  02-why-rust-and-ddd.md
  03-fundamentals/
    01-ddd-terms-in-rust.md
    02-rust-prerequisites.md
  04-tactical/
    01-value-object.md
    02-entity.md
    03-aggregate.md
    04-domain-service.md
  05-application/
    01-application-service.md
    02-repository.md
    03-infrastructure.md
  06-sample-app/
    01-domain-definition.md
    02-domain-model.md
    03-use-cases.md
    04-persistence-and-testing.md
  07-advanced/
    01-practical-tradeoffs.md
    02-common-stumbling-points.md
    03-cqrs-and-events.md
  08-conclusion.md

src/
  lib.rs
  domain/
    mod.rs
    task.rs
```

## SUMMARY.md のたたき台

```md
# Summary

- [はじめに](./01-introduction.md)
- [なぜRustでDDDを学ぶのか](./02-why-rust-and-ddd.md)

- [基礎整理](./03-fundamentals/01-ddd-terms-in-rust.md)
  - [DDDの基本用語をRust目線で整理する](./03-fundamentals/01-ddd-terms-in-rust.md)
  - [Rustでドメインを表現するための前提知識](./03-fundamentals/02-rust-prerequisites.md)

- [戦術的DDDをRustで実装する](./04-tactical/01-value-object.md)
  - [Value Objectを実装する](./04-tactical/01-value-object.md)
  - [Entityを実装する](./04-tactical/02-entity.md)
  - [Aggregateと整合性境界](./04-tactical/03-aggregate.md)
  - [Domain Serviceの使いどころ](./04-tactical/04-domain-service.md)

- [アプリケーション層とインフラ層](./05-application/01-application-service.md)
  - [Application Serviceでユースケースを表現する](./05-application/01-application-service.md)
  - [Repositoryをトレイトで定義する](./05-application/02-repository.md)
  - [Infrastructure層の実装方針](./05-application/03-infrastructure.md)

- [サンプルアプリケーション](./06-sample-app/01-domain-definition.md)
  - [サンプル題材のドメインを定義する](./06-sample-app/01-domain-definition.md)
  - [ドメインモデルを実装する](./06-sample-app/02-domain-model.md)
  - [ユースケースを実装する](./06-sample-app/03-use-cases.md)
  - [永続化とテストを実装する](./06-sample-app/04-persistence-and-testing.md)

- [発展事項](./07-advanced/01-practical-tradeoffs.md)
  - [RustでDDDを実践する際の悩みどころ](./07-advanced/01-practical-tradeoffs.md)
  - [DDD学習者がよくつまずく点](./07-advanced/02-common-stumbling-points.md)
  - [CQRSやイベント駆動へ発展させるには](./07-advanced/03-cqrs-and-events.md)

- [まとめ](./08-conclusion.md)
```

## 執筆の進め方の提案

- まずは Part 1 から Part 4 までを先に書き、概念整理と実装方針を固める
- 次に Part 5 のサンプルアプリケーションで、前半の説明内容を具体化する
- 最後に Part 6 で実務上のトレードオフや拡張案を補足する
- 各章は「概念の説明 → Rustでの設計指針 → コード例 → 注意点」の順で統一すると読みやすい

## 補足

- GitHub Pages公開を前提にするなら、1章ごとの分量は短めに保ち、コード例は小さく完結させると読みやすい
- 章が長くなりそうな箇所は、1ファイル1トピックに分割した方がmdBookのナビゲーションと相性がよい
- サンプルコードは最初から完成形を見せるのではなく、章を追って育てる構成にすると学習効果が高い

## このリポジトリでの配置方針

- 学習用の本文は `contents/` に配置する
- 本文で扱うサンプルコードとテストは `src/` に配置し、`cargo test` で検証できるようにする
- mdBook は `book.toml` で `contents/` を参照するように設定する