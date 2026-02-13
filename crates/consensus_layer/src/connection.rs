use futures_core::Future;
use futures_util::FutureExt;

use mev_share_sse::client::EventStream;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::task::JoinHandle;
use tracing::{debug, error};

use crate::types::{ForkVersionedResponse, SseExtendedPayloadAttributes};
use mev_share_sse::EventClient;

pub type PayloadStream = EventStream<ForkVersionedResponse<SseExtendedPayloadAttributes>>;

/// A future that continously tries to connect to the CL
/// until it returns a stream and it resolves
#[derive(Debug)]
pub struct ClConnection {
    /// Connection task
    inner: JoinHandle<PayloadStream>,
    /// URL of the consensus layer
    url: String,
}

impl ClConnection {
    pub fn new(url: String) -> Self {
        Self {
            inner: Self::connection_task(&url),
            url,
        }
    }

    pub fn boxed(self) -> Pin<Box<Self>> {
        Box::pin(self)
    }

    /// Spawns a connection task that tries to connect to the CL
    fn connection_task(url: &str) -> JoinHandle<PayloadStream> {
        let url = url.to_string();
        tokio::spawn(async move {
            let client = EventClient::new(reqwest::Client::new()).with_max_retries(10);
            loop {
                match client
                    .subscribe::<ForkVersionedResponse<SseExtendedPayloadAttributes>>(&url)
                    .await
                {
                    Ok(stream) => {
                        debug!(target: "consensus_layer::connection", "CL eth events connection acquired",);
                        return stream;
                    }
                    Err(e) => {
                        error!(target: "consensus_layer::connection", error=format!("{e:?}"), "Error connecting to CL");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                }
            }
        })
    }
}

impl Future for ClConnection {
    type Output = PayloadStream;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.inner.poll_unpin(cx) {
            Poll::Ready(Ok(stream)) => Poll::Ready(stream),
            Poll::Ready(Err(err)) => {
                error!(target: "consensus_layer::connection", error=format!("{err:?}"), "Error during connection to CL");
                self.inner = Self::connection_task(&self.url);
                let _ = self.inner.poll_unpin(cx);
                Poll::Pending
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use mev_share_sse::server::{SseBroadcastService, SseBroadcaster};
    use tokio::task::JoinHandle;

    use crate::connection::ClConnection;
    use hyper::service::make_service_fn;
    use hyper::Server;

    fn spawn_server() -> (JoinHandle<Result<(), hyper::Error>>, SocketAddr) {
        let addr = SocketAddr::from(([127, 0, 0, 1], 4000));

        let (tx, _rx) = tokio::sync::broadcast::channel(1000);
        let broadcaster = SseBroadcaster::new(tx);

        let b = broadcaster.clone();

        let svc = SseBroadcastService::new(move || b.ready_stream());

        let make_svc = make_service_fn(move |_| {
            let svc = svc.clone();
            async { Ok::<_, hyper::Error>(svc) }
        });

        let server = Server::bind(&addr).serve(make_svc);

        let server = tokio::spawn(server);
        (server, addr)
    }

    #[tokio::test]
    async fn cl_connection() {
        let (_, addr) = spawn_server();
        let url = format!("http://{addr}");
        let cl_connection = ClConnection::new(url.to_string());
        let _ = cl_connection.await;
    }
}
