use crate::job::build_utils::Cancelled;
use crate::strategy::bundle::strat::BundlesStrat;
use crate::strategy::mempool::MempoolStrat;
use builder_primitives::payload::BuildConfig;
use bundles::pool::BundlePool;
use reth::primitives::Withdrawals;
use reth::primitives::{constants::EMPTY_WITHDRAWALS, B256};
use reth::{primitives::U256, providers::StateProviderFactory, transaction_pool::TransactionPool};
use reth_payload_builder::database::CachedReads;
use reth_payload_builder::error::PayloadBuilderError;
use std::fmt::Debug;
use std::sync::Arc;
/// Bundle strategy implementation of BuildStrategy trait - mev bundles and mempool transactions
pub mod bundle;
/// Empty strategy implementation of BuildStrategy trait - no transactions
pub mod empty;
/// Mempool strategy implementation of BuildStrategy trait - mempool transactions
pub mod mempool;
/// Build state implementation
pub mod state;

use empty::EmptyStrat;

use self::state::BuildState;

/// A collection of arguments used for building payloads.
///
/// This struct encapsulates the essential components and configuration required for the payload
/// building process. It holds references to the Ethereum client, transaction pool, cached reads,
/// payload configuration, cancellation status, and the best payload achieved so far.
#[derive(Debug, Clone)]
pub struct BuildArguments<Client: Clone, Pool: Clone> {
    /// Client to access the current state of the chain.
    pub client: Client,
    /// Transaction pool
    pub pool: Pool,
    /// Bundle pool
    pub bundle_pool: BundlePool,
    /// Cached db reads
    pub cached_reads: CachedReads,
    /// Dynamic build configuration that changes on new slot
    pub build_config: BuildConfig,
    /// Utility to eventually cancel a paylod job
    pub cancel: Cancelled,
    /// Best build built
    pub best_build: Option<Arc<BuildState>>,
}

impl<Client: Clone, Pool: Clone> BuildArguments<Client, Pool> {
    /// Create new build arguments.
    pub fn new(
        client: Client,
        pool: Pool,
        bundle_pool: BundlePool,
        cached_reads: CachedReads,
        build_config: BuildConfig,
        cancel: Cancelled,
        best_build: Option<Arc<BuildState>>,
    ) -> Self {
        Self {
            client,
            pool,
            bundle_pool,
            cached_reads,
            build_config,
            cancel,
            best_build,
        }
    }
}

/// The possible outcomes of a payload building attempt.
#[derive(Debug)]
pub enum BuildOutcome {
    /// Successfully built a better block.
    Better {
        /// The new payload that was built with bid
        build_state: Arc<BuildState>,
        /// The cached reads that were used to build the payload.
        cached_reads: CachedReads,
    },
    /// Aborted payload building because resulted in worse block wrt. fees.
    Aborted {
        /// The total fees associated with the attempted payload.
        block_value: U256,
        /// The cached reads that were used to build the payload.
        cached_reads: CachedReads,
    },
    /// Build job was cancelled
    Cancelled,
}

/// Represents the outcome of committing withdrawals to the runtime database and post state.
/// Pre-shanghai these are `None` values.
#[derive(Debug, Default)]
pub struct WithdrawalsOutcome {
    /// List of withdrawals
    pub withdrawals: Option<Withdrawals>,
    /// withdrawals root
    pub withdrawals_root: Option<B256>,
}

impl WithdrawalsOutcome {
    /// No withdrawals pre shanghai
    fn pre_shanghai() -> Self {
        Self {
            withdrawals: None,
            withdrawals_root: None,
        }
    }

    fn empty() -> Self {
        Self {
            withdrawals: Some(Withdrawals::default()),
            withdrawals_root: Some(EMPTY_WITHDRAWALS),
        }
    }
}

// A trait for building payloads that encapsulate Ethereum transactions.
///
/// This trait provides the `try_build` method to construct a transaction payload
/// using `BuildArguments`. It returns a `Result` indicating success or a
/// `PayloadBuilderError` if building fails.
///
/// Generic parameters `Pool` and `Client` represent the transaction pool and
/// Ethereum client types.
pub trait BuildStrategy: Send + Sync + Clone {
    /// Tries to build a transaction payload using provided arguments.
    ///
    /// Constructs a transaction payload based on the given arguments,
    /// returning a `Result` indicating success or an error if building fails.
    ///
    /// # Arguments
    ///
    /// - `args`: Build arguments containing necessary components.
    ///
    /// # Returns
    ///
    /// A `Result` indicating the build outcome or an error.
    fn try_build<Client: StateProviderFactory + Clone, Pool: TransactionPool + Clone>(
        &self,
        args: BuildArguments<Client, Pool>,
    ) -> Result<BuildOutcome, PayloadBuilderError>;
}

macro_rules! dispatch_strats {
    ($($var:ident),*) => {
        /// All avalabile block building Strategy
        #[derive(Debug, Clone)]
        pub enum Strategy { $(
            #[allow(missing_docs)]
            $var($var),
        )*}

        impl Strategy {
            /// dispatch algo to strategy variant
            pub fn try_build<Client: StateProviderFactory + Clone, Pool: TransactionPool + Clone>(
                &self,
                args: BuildArguments<Client, Pool>,
            ) -> Result<BuildOutcome, PayloadBuilderError> {
                match &self {
                    $(Strategy::$var(inner) => inner.try_build(args),)*
                }
            }
        }

        $(
            impl From<$var> for Strategy {
                fn from(strat: $var) -> Strategy {
                    Strategy::$var(strat)
                }
            }

        )*
    }
}

dispatch_strats! {EmptyStrat, MempoolStrat, BundlesStrat}
