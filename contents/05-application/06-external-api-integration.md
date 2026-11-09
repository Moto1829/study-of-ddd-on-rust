# 第5章-6 外部API連携とInfrastructureの責務

実務では、DBだけでなく外部APIとの連携も頻繁に発生します。メール送信、通知、決済、認証、ファイル保存など、多くの機能が外部システムに依存しています。

この章の焦点は「外部API呼び出しをどの層へ置くべきか」です。イベント駆動で副作用をどう分離するかは、発展章の `ドメインイベントによる副作用の分離` で詳しく扱います。

## どこに置くべきか

外部API連携の実装はInfrastructure層に閉じるのが基本です。理由は単純で、HTTPクライアントやSDKの都合はドメインルールそのものではないからです。

ドメイン層やApplication Serviceは「何をしたいか」を表現し、実際にどのAPIをどう呼び出すかはInfrastructureで引き受ける方が責務が明確になります。

## サンプルの最小例

この本のタスク管理アプリに外部連携を足すとしたら、タスク完了通知のためのポートとして `TaskCompletionNotifier` のような境界を置くのが自然です。

```rust
pub trait TaskCompletionNotifier {
    fn notify_task_completed(&self, task_id: &str) -> Result<(), NotificationError>;
}
```

これに対して、Infrastructure側には `LoggingTaskCompletionNotifier` のような最小実装を置けます。実際のメール送信やWebhook呼び出しではありませんが、「外部連携の詳細はInfrastructureに置く」という形は示せます。

`LoggingTaskCompletionNotifier` は、実際の送信の代わりに標準エラー出力へログを書きます。

```rust
impl TaskCompletionNotifier for LoggingTaskCompletionNotifier {
    fn notify_task_completed(&self, task_id: &str) -> Result<(), NotificationError> {
        if task_id.trim().is_empty() {
            return Err(NotificationError::SendFailed {
                reason: "task id must not be empty".to_owned(),
            });
        }

        // 実際のメール送信やWebhookの代わりに、標準エラー出力へログを書く
        eprintln!("[通知] タスク {task_id} が完了しました");
        Ok(())
    }
}
```

## サンプルでの呼び出し方

最も単純なのは、Application Service の `complete_task` の中で notifier を直接呼ぶ形です。ただしこの形だと、Application Service が「完了したら通知する」という後続の処理まで知ることになり、通知以外の副作用(監査ログなど)が増えるたびに Application Service を書き換えることになります。

そこでサンプルコードでは、一歩進めてドメインイベント経由で呼び出しています。

1. `TaskApplicationService::complete_task` は、タスクを保存したあと `TaskEvent::TaskCompleted` を発行するだけ
2. `TaskCompletedNotificationHandler`(application層)がそのイベントを受け取り、`TaskCompletionNotifier` を呼ぶ
3. `LoggingTaskCompletionNotifier`(infrastructure層)が実際の通知を行う

```rust
impl<N> DomainEventPublisher for TaskCompletedNotificationHandler<N>
where
    N: TaskCompletionNotifier,
{
    fn publish(&self, event: &TaskEvent) -> Result<(), DomainEventPublishError> {
        match event {
            TaskEvent::TaskCompleted { task_id } => self
                .notifier
                .notify_task_completed(task_id.value())
                .map_err(|error| DomainEventPublishError::PublishFailed {
                    reason: format!("{error:?}"),
                }),
            TaskEvent::TaskCreated { .. } | TaskEvent::TaskArchived { .. } => Ok(()),
        }
    }
}
```

組み立ては `main.rs` で行います。

```rust
let event_publisher = TaskCompletedNotificationHandler::new(LoggingTaskCompletionNotifier);
```

CLIでタスクを完了すると、通知のログが出力されます。

```text
> complete t1
[通知] タスク t1 が完了しました
完了にしました: [t1] 牛乳を買う (completed)
```

テストでは、通知されたタスクIDを記録する Spy を notifier として渡し、「完了したときだけ通知される」ことを確かめています(`notification_handler_notifies_only_on_task_completed`、`completing_task_through_application_service_sends_notification`)。イベントの仕組みそのものは、発展章の `ドメインイベントによる副作用の分離` で詳しく扱います。

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

次章では、Application Service が扱うエラーを層ごとにどう設計するかを整理します。
