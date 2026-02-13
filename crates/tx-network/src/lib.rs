use std::marker::PhantomData;

use futures_util::StreamExt;
use merkle_sdk::prelude::Connection;
use pooled_tx::PooledTx;
use reth::{
    rpc::eth::error::EthApiError,
    transaction_pool::{error::PoolError, TransactionOrigin, TransactionPool},
};
use thiserror::Error;
use tracing::error;

mod pooled_tx;

#[derive(Debug, Error)]
pub enum TxNetworkError {
    #[error("Error converting ethers tx to pooled tx")]
    Conversion(#[from] EthApiError),
    #[error("Pool error while adding tx to pool")]
    Pool(#[from] PoolError),
}

/// Represents a network for processing transactions.
///
/// `TxNetwork` is a generic structure that works with any transaction pool
/// implementation that satisfies the `TransactionPool` trait. It's responsible
/// for retrieving transactions from a specified source and adding them to the
/// transaction pool.
pub struct TxNetwork<TxPool: TransactionPool + Clone + Unpin + 'static> {
    _tx_pool: PhantomData<TxPool>,
}

impl<TxPool: TransactionPool + Clone + Unpin + 'static> TxNetwork<TxPool> {
    /// Runs the transaction network to process and add transactions to the pool.
    ///
    /// This function initializes a connection using the provided `api_key`,
    /// then continuously fetches and processes transactions until the stream
    /// ends or an error occurs.
    ///
    /// # Arguments
    /// * `api_key` - An API key used to authenticate and establish the connection.
    /// * `tx_pool` - The transaction pool where processed transactions are added.
    ///
    /// # Returns
    /// A result indicating successful operation or an error (`TxNetworkError`).
    pub async fn run(api_key: String, tx_pool: TxPool) -> Result<(), TxNetworkError> {
        if let Ok(conn) = Connection::with_key(api_key).mainnet().build().await {
            let mut stream = conn.into_stream();
            while let Some(Ok(tx)) = stream.next().await {
                if let Ok(pooled_tx) = PooledTx::into(&tx) {
                    if let Err(err) = tx_pool
                        .add_transaction(TransactionOrigin::Private, pooled_tx)
                        .await
                    {
                        error!(target: "tx-network", ?err,"Error adding tx to pool");
                    }
                }
            }
        }
        Ok(())
    }
}
