use crate::application::TransactionRunner;

use super::InMemoryTaskRepository;

/// `InMemoryTaskRepository` と同じ保存領域を共有する、学習用の TransactionRunner 実装。
///
/// 処理の前に保存領域のスナップショットを取り、処理が `Err` を返したらスナップショットへ戻す。
/// DBのトランザクションにおける「ROLLBACK」をメモリ上で再現している。
///
/// 学習用の簡易実装なので、他のスレッドからの同時書き込みに対する分離は行っていない。
/// 本番のDB実装では、ここで `BEGIN` / `COMMIT` / `ROLLBACK` を発行する。
#[derive(Debug, Clone)]
pub struct InMemoryTransactionRunner {
    store: InMemoryTaskRepository,
}

impl InMemoryTransactionRunner {
    /// 渡した Repository と同じ保存領域を対象にする。
    pub fn new(repository: &InMemoryTaskRepository) -> Self {
        Self {
            store: repository.clone(),
        }
    }
}

impl<E> TransactionRunner<E> for InMemoryTransactionRunner {
    fn run<T, F>(&self, operation: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let snapshot = self.store.snapshot();
        let result = operation();

        if result.is_err() {
            self.store.restore(snapshot);
        }

        result
    }
}
