# 第7章-15 Sagaで複数Aggregateにまたがる処理を進める

1つのトランザクションでは1つの Aggregate だけを更新する、という原則を守ると、複数の Aggregate やサービスにまたがる業務処理をどう実現するかという問題が出てきます。第7章-12では、イベントで後続の処理をつなぐ結果整合性を扱いました。この章では、それを一歩進めて、途中で失敗したときの取り消しまで含めた Saga パターンを扱います。

## 例: 有料プランでのタスク作成

タスク管理アプリに、次のような業務ルールがあるとします。

- 無料プランでは、未完了タスクを10件まで作成できる
- それを超える場合は、追加枠を購入する必要がある
- 追加枠の購入では、決済サービスで支払いを行い、成功したら利用枠を増やし、タスクを作成する

この処理は、3つの異なる Aggregate やサービスにまたがります。

1. 決済サービスで支払う
2. 利用枠(`Quota` Aggregate)を増やす
3. タスク(`Task` Aggregate)を作成する

これを1つのトランザクションで行うことはできません。決済サービスは外部システムですし、`Quota` と `Task` は別の Aggregate だからです。

## 失敗したときに何が起きるか

各ステップは順番に成功するとは限りません。

- 支払いは成功したが、利用枠の更新に失敗した
- 利用枠は増えたが、タスクの作成に失敗した

このとき、利用者はお金を払ったのにタスクを作れない、という状態になります。1つのトランザクションならロールバックで元に戻せますが、ここではそれができません。

## Saga: 補償処理で取り消す

Saga は、長い処理を一連のローカルトランザクションに分け、途中で失敗したら、それまでに完了したステップを**補償処理(Compensating Action)** で取り消すパターンです。

| ステップ | 処理 | 補償処理 |
|---|---|---|
| 1 | 支払う | 返金する |
| 2 | 利用枠を増やす | 利用枠を減らす |
| 3 | タスクを作成する | (最後のステップなので不要) |

ステップ3で失敗したら、ステップ2の補償(利用枠を減らす)、ステップ1の補償(返金する)を逆順に実行します。

補償処理は、ロールバックとは違います。ロールバックは「何もなかったことにする」のに対し、補償処理は「取り消しという新しい業務上の操作を行う」ものです。返金は、支払いをなかったことにするのではなく、返金という記録を新たに残します。

## Saga の2つの実装方式

### コレオグラフィ(Choreography)

各参加者がイベントを受け取り、自分の処理を行って、次のイベントを発行する方式です。全体を指揮する中心がありません。

```text
[決済] ──PaymentCompleted──→ [利用枠] ──QuotaIncreased──→ [タスク]
   ↑                              ↑                          │
   └──── RefundRequested ←── QuotaDecreased ←── TaskCreationFailed
```

参加者が少なく、流れが単純なら、コレオグラフィで十分です。ただし、参加者が増えると、全体の流れがどこにも書かれていないため、把握が難しくなります。

### オーケストレーション(Orchestration)

中心となる**Process Manager(またはオーケストレーター)** が、各参加者に指示を出し、結果を受け取って次の指示を決める方式です。

```text
                 ┌──────────────────┐
                 │ Process Manager  │
                 └──────────────────┘
                  ↓ 支払え   ↓ 枠を増やせ  ↓ 作成せよ
               [決済]      [利用枠]      [タスク]
```

流れが1箇所に書かれるため、把握しやすく、テストもしやすくなります。参加者が3つ以上になるなら、オーケストレーションを検討する価値があります。

## Rustで Process Manager を表現する

Process Manager は、Saga の進行状況を状態として持つオブジェクトです。Rustでは、状態を enum で表現すると、どの段階にいるかが明確になります。

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurchaseSagaState {
    Started,
    PaymentCompleted { payment_id: PaymentId },
    QuotaIncreased { payment_id: PaymentId },
    Completed,
    Compensating { payment_id: PaymentId, quota_increased: bool },
    Failed,
}

pub struct PurchaseSaga {
    id: SagaId,
    user_id: UserId,
    title: TaskTitle,
    state: PurchaseSagaState,
}
```

各ステップの結果を受け取ったら、状態を進め、次に実行すべきコマンドを返します。

```rust
pub enum SagaCommand {
    RequestPayment { user_id: UserId },
    IncreaseQuota { user_id: UserId },
    CreateTask { user_id: UserId, title: TaskTitle },
    DecreaseQuota { user_id: UserId },
    Refund { payment_id: PaymentId },
}

pub enum SagaEvent {
    PaymentSucceeded { payment_id: PaymentId },
    PaymentFailed,
    QuotaIncreased,
    TaskCreated,
    TaskCreationFailed,
    QuotaDecreased,
    Refunded,
}

impl PurchaseSaga {
    pub fn handle(&mut self, event: SagaEvent) -> Option<SagaCommand> {
        use PurchaseSagaState as S;

        match (&self.state, event) {
            (S::Started, SagaEvent::PaymentSucceeded { payment_id }) => {
                self.state = S::PaymentCompleted { payment_id };
                Some(SagaCommand::IncreaseQuota {
                    user_id: self.user_id.clone(),
                })
            }
            (S::Started, SagaEvent::PaymentFailed) => {
                self.state = S::Failed;
                None
            }
            (S::PaymentCompleted { payment_id }, SagaEvent::QuotaIncreased) => {
                self.state = S::QuotaIncreased {
                    payment_id: payment_id.clone(),
                };
                Some(SagaCommand::CreateTask {
                    user_id: self.user_id.clone(),
                    title: self.title.clone(),
                })
            }
            (S::QuotaIncreased { .. }, SagaEvent::TaskCreated) => {
                self.state = S::Completed;
                None
            }
            (S::QuotaIncreased { payment_id }, SagaEvent::TaskCreationFailed) => {
                // 補償処理を開始する: まず利用枠を戻す
                self.state = S::Compensating {
                    payment_id: payment_id.clone(),
                    quota_increased: true,
                };
                Some(SagaCommand::DecreaseQuota {
                    user_id: self.user_id.clone(),
                })
            }
            (S::Compensating { payment_id, .. }, SagaEvent::QuotaDecreased) => {
                let payment_id = payment_id.clone();
                self.state = S::Compensating {
                    payment_id: payment_id.clone(),
                    quota_increased: false,
                };
                Some(SagaCommand::Refund { payment_id })
            }
            (S::Compensating { .. }, SagaEvent::Refunded) => {
                self.state = S::Failed;
                None
            }
            // 想定外の組み合わせ(重複したイベントなど)は無視する
            _ => None,
        }
    }
}
```

`handle` は、I/Oを一切行わない純粋な関数です。「この状態でこのイベントを受け取ったら、次にこのコマンドを実行する」という判断だけを行います。実際のコマンド実行は、アプリケーション層が受け持ちます。

この形にすると、Saga の流れ全体を外部依存なしでテストできます。

```rust
#[test]
fn task_creation_failure_triggers_quota_rollback() {
    let mut saga = PurchaseSaga::start(user_id(), title());
    saga.handle(SagaEvent::PaymentSucceeded { payment_id: payment_id() });
    saga.handle(SagaEvent::QuotaIncreased);

    let command = saga.handle(SagaEvent::TaskCreationFailed);

    assert!(matches!(command, Some(SagaCommand::DecreaseQuota { .. })));
}
```

## Saga の状態も永続化する

Saga は長い時間をかけて進むことがあります。途中でプロセスが再起動しても続きから再開できるように、Saga の状態も Aggregate と同じように Repository で保存します。

また、Saga が受け取るイベントは、前章で扱ったように重複して届く可能性があります。上の `handle` で、想定外の組み合わせを無視しているのはそのためです。たとえば `Completed` の状態で `TaskCreated` をもう一度受け取っても、何も起きません。

## 補償処理の設計で注意すること

### 補償処理も失敗しうる

返金の処理が失敗することもあります。補償処理は、成功するまでリトライできるように、冪等に作る必要があります。それでも失敗し続ける場合は、人間による対応が必要になるため、アラートを出す仕組みを用意します。

### 取り消せない操作は最後に回す

メールの送信のように、取り消せない操作があります。こうした操作は、Saga の最後に置くのが原則です。前のステップがすべて成功してから実行すれば、補償処理が必要になりません。

### 途中の状態が見えることを受け入れる

Saga の途中では、「支払いは済んだが、タスクはまだない」という状態が外から見えます。これは結果整合性の一種です。業務担当者と相談し、途中状態をどう表示するか(「処理中」と表示するなど)を決めておきます。

## Saga が必要か見極める

Saga は強力ですが、状態管理、補償処理、冪等性など、考えることが多くなります。導入の前に、次の点を確認します。

- Aggregate の境界を見直せば、1つのトランザクションで済まないか
- 補償処理が本当に必要か。失敗したら手動で対応する運用で十分ではないか
- 結果整合性の単純なイベント連携(第7章-12)で足りないか

複数の外部システムが関わり、失敗時の取り消しが業務上必須である場合に、Saga は本領を発揮します。

## よくある誤解

- Saga を使えば、分散トランザクションと同じ保証が得られると思いがち
- 補償処理は、ロールバックと同じく状態を完全に元に戻すものだと考えがち
- マイクロサービスでなければ Saga は不要だと思いがち

Saga は、途中の状態が外から見える点で、トランザクションとは本質的に異なります。また、単一のアプリケーションでも、外部の決済サービスなどと連携すれば Saga と同じ問題に直面します。

次章では、状態ではなくイベントの履歴そのものを保存する Event Sourcing を見ます。
