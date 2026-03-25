# 実務で使うディレクトリ構造の考え方

DDDの学習で意外と詰まりやすいのが、概念ではなくファイル配置です。どこに何を置けばよいかが見えないと、せっかく理解した責務分担も実装へ落としにくくなります。

この章では、単一クレートを前提にした整理の仕方を扱います。Cargo workspaceへいつ分割するかは、後続の `Cargo workspaceに分割するタイミング` に分けて説明します。

## まずはこの構成で十分

単一クレートの学習用プロジェクトでは、まず次のような責務分割を意識すると整理しやすくなります。

- `domain`: ドメインモデル
- `application`: ユースケースとQueryモデル
- `infrastructure`: 永続化や外部連携の実装
- `presentation`: HTTPやCLIなどの入出力

まずはこのレベルで十分です。学習段階で最初から複雑なCargo workspaceに分割する必要はありません。

## 単一クレートでの実務向け構成例

小中規模のシステムなら、単一クレートのままでもかなり整理できます。

```text
src/
  domain/
    task/
      mod.rs
      task.rs
      task_id.rs
      task_title.rs
      task_error.rs
    shared/
  application/
    task/
      create_task.rs
      complete_task.rs
      archive_task.rs
      find_tasks.rs
  infrastructure/
    persistence/
      task_repository.rs
      task_row.rs
    external/
      notifier.rs
  presentation/
    http/
    cli/
  lib.rs
  main.rs
```

この構成の利点は、責務ごとにファイルを分けながら、依存方向をまだ追いやすいことです。

## いつ workspace を考えるか

次のような状況になったら、Cargo workspaceを検討してもよいです。

- APIサーバーとバッチを明確に分けたい
- ドメイン層を複数の実行バイナリから共有したい
- プレゼンテーション層の依存が重く、分離メリットが大きい

ただし、workspaceは整理手段であって、DDDを実現するための必須条件ではありません。境界が見えない段階で分けると、かえって設計の迷いが増えます。

この節では「workspaceを検討するサイン」だけを押さえ、実際にどの順で分けるべきかは後続章へ譲ります。

## 置き場所に迷ったときの基準

新しいコードを書くときは、次の問いで判断すると整理しやすいです。

- これは業務ルールか
- これはユースケースの進行か
- これはDBやHTTPなどの技術都合か
- これは表示や入出力の都合か

この基準で見れば、かなりの確率で `domain` / `application` / `infrastructure` / `presentation` のどこへ置くべきかが見えてきます。