# Domain Serviceの使いどころ

Domain Serviceは便利な退避先ではありません。重要な業務ルールだが、特定のEntityやValue Objectの責務として置きにくい場合にだけ導入するものです。

## まずEntityやValue Objectを疑う

たとえば、このサンプルで「完了済みタスクは名前変更できない」というルールをServiceに置く必要はありません。これは `Task` 自身の整合性に関わるので、Entityメソッドに置くのが自然です。

同様に、「タイトルは空文字不可」というルールは `TaskTitle` に置くべきで、Serviceに逃がすとValue Objectの意味が薄れます。

## Domain Serviceが必要になる場面

次のようなケースでは、Domain Serviceが候補になります。

- 複数のEntityやValue Objectをまたいで判断する
- ドメイン上重要だが、どの1つのEntityにも自然に属さない
- 単なるアプリケーションフローではなく、業務ルールそのものを表す

たとえば「同一ユーザーの未完了タスク数に応じて新規作成可否を決める」といったルールは、1つの `Task` だけでは判断できません。この種のルールはDomain Serviceとして切り出す余地があります。

## Application Serviceとの違い

Application Serviceはユースケースの進行役であり、Domain Serviceは業務ルールの担い手です。似たような見た目になることはありますが、責務は違います。

- Application Service: 入力を受けて処理を組み立てる
- Domain Service: ドメイン判断を提供する

この違いを曖昧にすると、Service層が巨大な手続き置き場になります。

## このサンプルでの判断

現時点のタスク管理サンプルでは、明確なDomain Serviceはまだ不要です。これは悪いことではなく、むしろモデルの責務が自然にまとまっている証拠です。Domain Serviceは「必要だから導入する」のであって、「DDDっぽく見せるために置く」ものではありません。

次は、ユースケースを調停するApplication Serviceを実装します。