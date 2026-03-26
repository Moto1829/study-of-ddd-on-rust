# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 概要

RustにおけるDDD(ドメイン駆動設計)の学習用リポジトリ。タスク管理アプリを題材にDDDの戦術的パターンを実装したサンプルコード(`src/`)と、mdBook形式の学習テキスト(`contents/`)で構成される。

## コマンド

```bash
# ビルド
cargo build

# テスト実行（全件）
cargo test

# 特定のテストのみ実行
cargo test <テスト関数名>
# 例: cargo test task_title_rejects_empty_input

# mdBookのローカルプレビュー（要 mdbook インストール）
mdbook serve

# mdBookのビルド
mdbook build
```

## アーキテクチャ

`src/` はDDDの三層構造で構成されている:

```
src/
├── domain/           # ドメイン層（外部依存なし）
│   ├── task.rs           # Entity: Task, Value Object: TaskId/TaskTitle/TaskStatus
│   ├── repository.rs     # TaskRepository トレイト定義
│   ├── task_creation_policy.rs  # Domain Service: OpenTaskLimitPolicy
│   └── task_event.rs     # Domain Event: TaskEvent
├── application/      # アプリケーション層
│   ├── task_application_service.rs  # ユースケース実装 (TaskApplicationService<R>)
│   ├── task_query_service.rs        # 読み取りモデル (TaskSummaryReader トレイト)
│   ├── task_completion_notifier.rs  # 外部通知トレイト
│   ├── domain_event_publisher.rs    # イベント発行トレイト
│   └── transaction_runner.rs        # トランザクション境界トレイト
└── infrastructure/   # インフラ層
    ├── in_memory_task_repository.rs  # InMemoryTaskRepository (TaskRepository実装)
    ├── persistence/task_row.rs       # TaskRow (DBレコード相当の永続化モデル)
    └── external/
        ├── logging_task_completion_notifier.rs  # ログ出力通知実装
        └── in_memory_domain_event_publisher.rs  # インメモリイベント発行実装
```

### 設計の核心

- **依存方向**: infrastructure → application → domain（domainは他層に依存しない）
- **Repository トレイト**: `domain::TaskRepository` はドメイン層で定義し、インフラ層で実装。`TaskApplicationService<R: TaskRepository>` のようにジェネリクスで注入する
- **永続化モデルの分離**: `Task`（ドメインオブジェクト）と `TaskRow`（永続化レコード）を分け、`From<&Task>`/`TryFrom<TaskRow>` で変換する
- **状態遷移**: タスクは Open → Completed/Archived の一方向遷移。Open状態のタスクのみリネーム可能
- **テスト**: `src/lib.rs` の `#[cfg(test)]` にすべての統合テストをまとめている

### mdBook

- コンテンツ: `contents/` 配下のMarkdownファイル
- 目次: `contents/SUMMARY.md`
- ビルド出力: `book/`（gitignore対象）
- `main` ブランチへのpushでGitHub Actionsが自動デプロイ（`.github/workflows/deploy-pages.yml`）
