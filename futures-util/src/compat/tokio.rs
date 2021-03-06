use crate::{future::FutureExt, try_future::TryFutureExt};
use futures_core::future::FutureObj;
use futures_core::task::{Spawn, SpawnErrorKind, SpawnObjError};
use tokio_executor::{DefaultExecutor, Executor as TokioExecutor};

/// A spawner that delegates to `tokio`'s
/// [`DefaultExecutor`](tokio_executor::DefaultExecutor), will panic if used in
/// the context of a task that is not running on `tokio`'s executor.
///
/// *NOTE* The future of this struct in `futures` is uncertain. It may be
/// deprecated before or soon after the initial 0.3 release and moved to a
/// feature in `tokio` instead.
///
/// # Examples
///
/// ```ignore
/// #![feature(async_await, await_macro, futures_api, pin)]
/// use futures::spawn;
/// use futures::channel::oneshot;
/// use futures::compat::TokioDefaultSpawn;
/// use futures::executor::block_on;
/// use futures::future::{FutureExt, TryFutureExt};
/// use std::thread;
///
/// let (sender, receiver) = oneshot::channel::<i32>();
///
/// thread::spawn(move || {
///     let future = async move {
///         spawn!(async move {
///             sender.send(5).unwrap()
///         }).unwrap();
///
///     };
///
///     let compat_future = future
///         .boxed()
///         .unit_error()
///         .compat(TokioDefaultSpawn);
///
///     tokio::run(compat_future);
/// }).join().unwrap();
///
/// assert_eq!(block_on(receiver).unwrap(), 5);
/// ```
#[derive(Debug, Copy, Clone)]
pub struct TokioDefaultSpawn;

impl Spawn for TokioDefaultSpawn {
    fn spawn_obj(
        &mut self,
        task: FutureObj<'static, ()>,
    ) -> Result<(), SpawnObjError> {
        let fut = Box::new(task.unit_error().compat(*self));
        DefaultExecutor::current().spawn(fut).map_err(|err| {
            panic!(
                "tokio failed to spawn and doesn't return the future: {:?}",
                err
            )
        })
    }

    fn status(&self) -> Result<(), SpawnErrorKind> {
        DefaultExecutor::current().status().map_err(|err| {
            if err.is_shutdown() {
                SpawnErrorKind::shutdown()
            } else {
                panic!(
                    "tokio executor failed for non-shutdown reason: {:?}",
                    err
                )
            }
        })
    }
}
