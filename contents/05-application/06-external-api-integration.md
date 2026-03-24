# 外部API連携とInfrastructureの責務

実務では、DBだけでなく外部APIとの連携も頻繁に発生します。メール送信、通知、決済、認証、ファイル保存など、多くの機能が外部システムに依存しています。

この章の焦点は「外部API呼び出しをどの層へ置くべきか」です。イベント駆動で副作用をどう分離するかは、発展章の `ドメインイベントによる副作用の分離` で詳しく扱います。

## どこに置くべきか

外部API連携の実装はInfrastructure層に閉じるのが基本です。理由は単純で、HTTPクライアントやSDKの都合はドメインルールそのものではないからです。

ドメイン層やApplication Serviceは「何をしたいか」を表現し、実際にどのAPIをどう叩くかはInfrastructureで引き受ける方が責務が明確になります。

## サンプルの最小例

このタスク管理アプリでは、タスク完了通知のためのポートとして `TaskCompletionNotifier` を追加しています。

```rust
pub trait TaskCompletionNotifier {
    fn notify_task_completed(&self, task_id: &str) -> Result<(), NotificationError>;
}
```

これに対して、Infrastructure側には `LoggingTaskCompletionNotifier` という最小実装を置いています。実際のメール送信やWebhook呼び出しではありませんが、「外部連携の詳細はInfrastructureに置く」という形は示せます。

ここでは、通知を直接Application Serviceから呼ぶ最小構成を理解できれば十分です。後からドメインイベント経由へ発展させる余地がある、という順番で読むと混乱しにくくなります。

## 重要なのは依存方向

ここで大切なのは、ドメイン層がHTTPクライアントやSDKを知らないことです。

- ドメイン層: 業務ルールを持つ
- Application層: いつ通知するかを決める
- Infrastructure層: どう通知するかを実装する

この分担を守ると、外部API変更の影響を閉じ込めやすくなります。

## よくある誤解

- とりあえずApplication Serviceから直接HTTPを叩けばよいと思いがち
- 通知処理もドメインメソッドの中へ埋め込みたくなりがち
- SDKに合わせてドメインモデルの形を変えたくなりがち

外部API都合をドメイン側へ持ち込むと、業務ルールと技術詳細が密結合になります。最初は小さなポートだけでも用意しておくと、あとで拡張しやすくなります。