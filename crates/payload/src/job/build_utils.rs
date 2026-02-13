use crate::strategy::BuildOutcome;
use builder_primitives::build::Build;
use futures_core::ready;
use futures_util::{Future, FutureExt};
use reth_payload_builder::error::PayloadBuilderError;
use std::{
    pin::Pin,
    sync::{atomic::AtomicBool, Arc},
    task::{Context, Poll},
};
use tokio::sync::{oneshot, Semaphore};

/// Restricts how many generator tasks can be executed at once.
#[derive(Debug, Clone)]
pub struct PayloadTaskGuard(pub Arc<Semaphore>);

// === impl PayloadTaskGuard ===

impl PayloadTaskGuard {
    /// Creates a new `PayloadTaskGuard` with the given max number of tasks.
    pub fn new(max_payload_tasks: usize) -> Self {
        Self(Arc::new(Semaphore::new(max_payload_tasks)))
    }
}

/// A marker that can be used to cancel a job.
///
/// If dropped, it will set the `cancelled` flag to true.
#[derive(Default, Clone, Debug)]
pub struct Cancelled(Arc<AtomicBool>);

// === impl Cancelled ===

impl Cancelled {
    /// Returns true if the job was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// The future that returns the best payload to be served to the consensus layer.
///
/// This returns the payload that's supposed to be sent to the CL.
///
/// If payload has been built so far, it will return that, but it will check if there's a better
/// payload available from an in progress build job. If so it will return that.
///
/// If no payload has been built so far, it will either return an empty payload or the result of the
/// in progress build job, whatever finishes first.
#[derive(Debug)]
pub struct ResolveBestPayload {
    /// Best payload so far.
    pub best_payload: Option<Arc<Build>>,
    /// Regular payload job that's currently running that might produce a better payload.
    pub maybe_better: Option<PendingBuild>,
    /// The empty payload building job in progress.
    pub empty_payload: Option<oneshot::Receiver<Result<Build, PayloadBuilderError>>>,
}

impl Future for ResolveBestPayload {
    type Output = Result<Arc<Build>, PayloadBuilderError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        if let Some(best) = this.best_payload.take() {
            return Poll::Ready(Ok(best));
        }

        let mut empty_payload = this.empty_payload.take().expect("polled after completion");
        match empty_payload.poll_unpin(cx) {
            Poll::Ready(Ok(res)) => Poll::Ready(res.map(Arc::new)),
            Poll::Ready(Err(err)) => Poll::Ready(Err(err.into())),
            Poll::Pending => {
                this.empty_payload = Some(empty_payload);
                Poll::Pending
            }
        }
    }
}

/// A future that resolves to the result of the block building job.
#[derive(Debug)]
pub struct PendingBuild {
    /// The marker to cancel the job on drop
    pub _cancel: Cancelled,
    /// The channel to send the result to.
    pub build: oneshot::Receiver<Result<BuildOutcome, PayloadBuilderError>>,
}

impl Future for PendingBuild {
    type Output = Result<BuildOutcome, PayloadBuilderError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let res = ready!(self.build.poll_unpin(cx));
        Poll::Ready(res.map_err(Into::into).and_then(|res| res))
    }
}
