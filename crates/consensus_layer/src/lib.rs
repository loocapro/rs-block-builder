use connection::PayloadStream;
use futures_core::Stream;
use futures_util::FutureExt;
use futures_util::StreamExt;
use reth_payload_builder::EthPayloadBuilderAttributes;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tracing::error;

use crate::connection::ClConnection;

pub mod clock;
mod connection;

pub mod types;

/// Listens for new payload attributes from CL and sends them to the payload builder
/// via the payload builder handle.
/// It's the trigger for our building jobs.
#[must_use = "Consensus layer does nothing unless polled"]
#[derive(Debug)]
pub struct ConsensusLayer {
    /// Future that resolves to a CL connection
    cl_connection: Pin<Box<ClConnection>>,
    /// Stream of payload attributes from CL
    payload_stream: Option<PayloadStream>,
    /// URL of the consensus layer
    url: String,
}

impl ConsensusLayer {
    pub fn new(url: String) -> Self {
        let cl_connection = ClConnection::new(url.clone()).boxed();

        Self {
            cl_connection,
            payload_stream: None,
            url,
        }
    }
}

impl Stream for ConsensusLayer {
    type Item = (EthPayloadBuilderAttributes, u64);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.payload_stream.as_mut() {
            Some(stream) => match stream.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(payload))) => {
                    let proposal_slot = payload.data.proposal_slot;
                    let payload = payload.into_reth_payload_attributes();
                    Poll::Ready(Some((payload, proposal_slot)))
                }
                Poll::Ready(Some(Err(err))) => {
                    error!(target: "consensus_layer::ext", error=format!("{err:?}"), "Error in payload stream");
                    Poll::Pending
                }
                Poll::Ready(None) => {
                    error!(target: "consensus_layer::ext", "Consensus layer subscription disconnected");
                    self.cl_connection = ClConnection::new(self.url.clone()).boxed();
                    let _ = self.cl_connection.poll_unpin(cx);
                    Poll::Pending
                }
                Poll::Pending => Poll::Pending,
            },
            None => match self.cl_connection.poll_unpin(cx) {
                Poll::Ready(stream) => {
                    self.payload_stream = Some(stream);
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use crate::types::{ForkVersionedResponse, SseExtendedPayloadAttributes, SsePayloadAttributes};
    use crate::ConsensusLayer;
    use futures_util::StreamExt;
    use hyper::service::make_service_fn;
    use hyper::Server;
    use mev_share_sse::server::{SSeBroadcastMessage, SseBroadcastService, SseBroadcaster};
    use tokio::sync::broadcast::Sender;
    use tokio::sync::mpsc::{self};

    fn spawn_server(tx: Sender<SSeBroadcastMessage>) -> (SocketAddr, SseBroadcaster) {
        let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
        let broadcaster = SseBroadcaster::new(tx);

        let b = broadcaster.clone();

        let svc = SseBroadcastService::new(move || b.ready_stream());

        let make_svc = make_service_fn(move |_| {
            let svc = svc.clone();
            async { Ok::<_, hyper::Error>(svc) }
        });

        let server = Server::bind(&addr).serve(make_svc);

        tokio::spawn(server);

        (addr, broadcaster)
    }

    impl Default for ForkVersionedResponse<SseExtendedPayloadAttributes> {
        fn default() -> Self {
            Self {
                version: Some(crate::types::ForkName::Capella),
                data: SseExtendedPayloadAttributes {
                    proposal_slot: 0,
                    proposer_index: 0,
                    parent_block_root: Default::default(),
                    parent_block_number: 0,
                    parent_block_hash: Default::default(),
                    payload_attributes: SsePayloadAttributes {
                        timestamp: 0,
                        prev_randao: Default::default(),
                        suggested_fee_recipient: Default::default(),
                        withdrawals: vec![],
                    },
                },
            }
        }
    }

    #[tokio::test]
    async fn cl_can_broadcast() {
        let (tx, _rx) = tokio::sync::broadcast::channel(1000);
        let (addr, broadcaster) = spawn_server(tx);
        let url = format!("http://{addr}");
        let mut cl = ConsensusLayer::new(url.to_string());
        let (tx, mut rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            while let Some(event) = cl.next().await {
                let _ = tx.send(event);
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let event = ForkVersionedResponse::default();
        broadcaster.send(&event).unwrap();

        let received = rx.recv().await.unwrap();

        assert_eq!(event.into_reth_payload_attributes().id, received.0.id);
    }
}
