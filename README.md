# study-of-ddd-on-rust
RustにおけるDDD(ドメイン駆動開発)の実現方法の学習用リポジトリ

このリポジトリは Cargo プロジェクトとして構成し、学習用テキストは `contents/`、本文で扱うサンプルコードとテストは `src/` に配置します。

## コンテンツ構成案

mdBookでの公開を前提にした構成案を以下にまとめています。

- [docs/content-outline.md](docs/content-outline.md)
- [docs/content-expansion-proposal.md](docs/content-expansion-proposal.md)

## 執筆の開始地点

- mdBook の目次: [contents/SUMMARY.md](contents/SUMMARY.md)
- 本文: [contents/01-introduction.md](contents/01-introduction.md)
- 検証用コード: [src/lib.rs](src/lib.rs)

本文ファイルと章フォルダには順序が追いやすいよう番号を付けています。

## 現在のカバー範囲

- 導入
- DDD基礎整理
- 戦術的DDDの前半: Value Object / Entity / Aggregate / Domain Service
- アプリケーション層とインフラ層の導入: Application Service / Repository / InMemory実装

## GitHub Pages公開

GitHub Actions で mdBook をビルドし、GitHub Pages へデプロイする workflow を追加しています。

- Workflow: [.github/workflows/deploy-pages.yml](.github/workflows/deploy-pages.yml)
- 公開先想定URL: `https://moto1829.github.io/study-of-ddd-on-rust/`

初回のみ、GitHub のリポジトリ設定で Pages の Source を `GitHub Actions` に設定してください。以後は `main` への push で自動デプロイされます。
