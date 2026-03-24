pub trait TransactionRunner<E> {
    fn run<T, F>(&self, operation: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>;
}