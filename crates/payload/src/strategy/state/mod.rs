use builder_primitives::{
    build::{Build, BuildId, MAX_BUILDER_TRANSFER_GAS_LIMIT},
    payload::{BuildConfig, PayloadConfig},
};
use bundles::bundle::RecoveredBundle;
use reth::primitives::{
    constants::{
        eip4844::MAX_DATA_GAS_PER_BLOCK, BEACON_NONCE, EMPTY_RECEIPTS, EMPTY_TRANSACTIONS,
    },
    proofs,
    revm::env::fill_tx_env_with_recovered,
    sign_message, AccessList, Address, Block, ChainSpec, Header, Receipt, Receipts, Transaction,
    TransactionKind, TransactionSigned, TransactionSignedEcRecovered, TxEip1559, Withdrawals, B256,
    EMPTY_OMMER_ROOT_HASH, U256,
};

use reth::primitives::eip4844::calculate_excess_blob_gas;
use reth::providers::{BundleStateWithReceipts, StateProviderFactory};
use reth::revm::state_change::{
    apply_beacon_root_contract_call, post_block_withdrawals_balance_increments,
};
use reth::revm::{
    db::states::bundle_state::BundleRetention,
    primitives::{EVMError, Env, ResultAndState},
    DatabaseCommit, State,
};
use reth::transaction_pool::TransactionPool;
use reth_interfaces::{RethError, RethResult};
use reth_payload_builder::{database::CachedReads, EthBuiltPayload};
use reth_revm::database::StateProviderDatabase;
use revm::{db::BundleState, Database};
use revm_primitives::Bytes;
use std::{fmt, sync::Arc, time::Instant};
use thiserror::Error;
use tracing::{error, info};

use reth_payload_builder::error::PayloadBuilderError;

use self::{db::SimulationDB, exec_details::ExecutionDetails};

use super::WithdrawalsOutcome;

/// Execution and simulation database types
pub mod db;
/// execution details
pub mod exec_details;
use db::{simulation_db, ExecutionDB};
/// Transaction simulation types
pub mod transaction;
use transaction::{SimulatedTransaction, TransactionSimulationOutcome};
/// Bundle simulation types
pub mod bundle;
use bundle::{BundleSimulationOutcome, SimulatedBundle};

/// Error type for transaction execution
#[derive(Debug, Error)]
pub enum BuildStateExecutionError {
    /// Transaction results in gas limit exceeded
    #[error("Block max gas exceeded")]
    MaxGasExceeded,
    /// Transaction results in blob gas limit exceeded
    #[error("Block max blob gas exceeded")]
    MaxBlobGasExceeded,
    /// Payload builder error
    #[error("{0}")]
    PayloadBuilder(#[from] PayloadBuilderError),
    /// Bundle execution error
    #[error("{0}")]
    BundleExection(#[from] EVMError<RethError>),
    /// Bundle reverted
    #[error("Bundle reverted at tx {0}")]
    BundleReverted(usize),
    /// Ofac address detected
    #[error("Simulation outcome contains ofac blacklisted address {0}")]
    OfacAddressDetected(Address),
    /// Refund transaction from builder to refund recipient failed to simulate
    #[error("Builder refund tx failed: {0}")]
    BuilderRefundTx(String),
}

/// State object containing build parameters, context, and progress
/// All build related modifications are implemented as methods on this state machine
#[derive(Clone)]
pub struct BuildState {
    /// Build identifier
    pub id: BuildId,
    /// Timestamp of build state initialized
    pub timestamp: Instant,
    /// Static configuration for payloads
    payload_config: Arc<PayloadConfig>,
    /// Dynamic build configuration that changes on every slot
    build_config: Arc<BuildConfig>,
    /// execution details for current build state
    exec_details: ExecutionDetails,
}

impl BuildState {
    /// Creates a new `BuildState` instance from given build and payload configurations.
    /// This function initializes the build environment with the coinbase address from the payload configuration
    /// and sets up the build and payload configurations within `Arc`s for shared ownership.
    ///
    /// # Arguments
    ///
    /// * `build_config` - Configuration for the build process including slot, attributes, and gas limits.
    /// * `payload_config` - Configuration for the payload including builder address and other static configurations.
    ///
    /// # Returns
    ///
    /// Returns a new `BuildState` instance.
    pub fn new(
        mut build_config: BuildConfig,
        payload_config: PayloadConfig,
        timestamp: Instant,
    ) -> Self {
        build_config.block_env_with_coinbase(payload_config.builder_address);

        BuildState {
            id: BuildId::random(),
            timestamp,
            payload_config: Arc::new(payload_config),
            build_config: Arc::new(build_config),
            exec_details: ExecutionDetails::default(),
        }
    }
    /// Logs helper
    pub fn to_log(&self) -> String {
        format!(
            "build_id = {:?}, slot = {}, block_number = {}",
            self.id, self.build_config.slot, self.build_config.block_number
        )
    }

    /// Provides a mutable reference to the execution details.
    ///
    /// # Returns
    ///
    /// Returns a mutable reference to the `ExecutionDetails` of the current build state.
    pub fn exec_details_mut(&mut self) -> &mut ExecutionDetails {
        &mut self.exec_details
    }

    /// Provides an immutable reference to the execution details.
    ///
    /// # Returns
    ///
    /// Returns an immutable reference to the `ExecutionDetails` of the current build state.
    pub fn exec_details(&self) -> &ExecutionDetails {
        &self.exec_details
    }

    /// Provides an immutable reference to the payload configuration.
    ///
    /// # Returns
    ///
    /// Returns an immutable reference to the `PayloadConfig` of the current build state.
    pub fn payload_config(&self) -> &PayloadConfig {
        &self.payload_config
    }

    /// Provides an immutable reference to the build configuration.
    ///
    /// # Returns
    ///
    /// Returns an immutable reference to the `BuildConfig` of the current build state.
    pub fn build_config(&self) -> &BuildConfig {
        &self.build_config
    }

    /// Get total block value
    pub fn block_value(&self) -> U256 {
        self.exec_details.sum_fees + self.exec_details.sum_coinbase_transfers
    }

    /// Get timestamp
    pub fn timestamp(&self) -> Instant {
        self.timestamp
    }

    /// Set builder payment
    pub fn with_payment(&mut self, builder_payment: U256) {
        self.exec_details_mut().set_builder_payment(builder_payment);
    }

    /// Set bundle state
    pub fn with_bundle_state(&mut self, bundle_state: BundleState) {
        self.exec_details.set_bundle_state(bundle_state);
    }
}

impl BuildState {
    /// Executes the withdrawals and commits them to the _runtime_ Database and BundleState.
    ///
    /// Returns the withdrawals root.
    ///
    /// Returns `None` values pre shanghai
    pub fn commit_withdrawals(
        db: &mut ExecutionDB<'_>,
        chain_spec: &ChainSpec,
        timestamp: u64,
        withdrawals: Withdrawals,
    ) -> RethResult<WithdrawalsOutcome> {
        if !chain_spec.is_shanghai_active_at_timestamp(timestamp) {
            return Ok(WithdrawalsOutcome::pre_shanghai());
        }

        if withdrawals.is_empty() {
            return Ok(WithdrawalsOutcome::empty());
        }

        let balance_increments =
            post_block_withdrawals_balance_increments(chain_spec, timestamp, &withdrawals);

        db.increment_balances(balance_increments)?;

        let withdrawals_root = proofs::calculate_withdrawals_root(&withdrawals);

        // calculate withdrawals root
        Ok(WithdrawalsOutcome {
            withdrawals: Some(withdrawals),
            withdrawals_root: Some(withdrawals_root),
        })
    }

    /// Apply the [EIP-4788](https://eips.ethereum.org/EIPS/eip-4788) pre block contract call.
    ///
    /// This constructs a new [EVM](revm::EVM) with the given DB, and environment ([CfgEnv] and
    /// [BlockEnv]) to execute the pre block contract call.
    ///
    /// The parent beacon block root used for the call is gathered from the given
    /// [PayloadBuilderAttributes].
    ///
    /// This uses [apply_beacon_root_contract_call] to ultimately apply the beacon root contract state
    /// change.
    pub fn pre_block_beacon_root_contract_call(
        &mut self,
        db: &mut ExecutionDB<'_>,
    ) -> Result<(), PayloadBuilderError> {
        // Configure the environment for the block.
        let env = Env::boxed(
            self.build_config.initialized_cfg.cfg_env.clone(),
            self.build_config.initialized_block_env.clone(),
            Default::default(),
        );

        // apply pre-block EIP-4788 contract call
        let mut evm_pre_block = revm::Evm::builder().with_db(db).with_env(env).build();

        // initialize a block from the env, because the pre block call needs the block itself
        apply_beacon_root_contract_call(
            &self.payload_config.chain_spec,
            self.build_config.attributes.timestamp,
            self.build_config.block_number,
            self.build_config.parent_block.parent_beacon_block_root,
            &mut evm_pre_block,
        )
        .map_err(|err| PayloadBuilderError::Internal(err.into()))
    }

    /// Construct builder transfer transaction to the `recipient` address for `value` amount
    /// Builder transfer transactions are used to refund bundles with proper refund arguments and
    /// to pay the bid for the block to the validator at the end of the block
    pub fn create_builder_transfer_transaction<DB: Database>(
        &mut self,
        mut db: DB,
        recipient: Address,
        payment: U256,
    ) -> Result<TransactionSignedEcRecovered, PayloadBuilderError>
    where
        PayloadBuilderError: From<<DB as revm::Database>::Error>,
    {
        let builder_account = db
            .basic(revm_primitives::Address(
                *self.payload_config.builder_address,
            ))
            .map_err(PayloadBuilderError::from)?
            .unwrap_or_default();

        let basefee = self.build_config.initialized_block_env.basefee;

        let new_tx = Transaction::Eip1559(TxEip1559 {
            chain_id: self.build_config.initialized_cfg.chain_id,
            nonce: builder_account.nonce,
            gas_limit: MAX_BUILDER_TRANSFER_GAS_LIMIT,
            to: TransactionKind::Call(recipient),
            value: payment,
            max_fee_per_gas: basefee.to::<u128>(),
            max_priority_fee_per_gas: 0,
            access_list: AccessList::default(),
            input: Bytes::default(),
        });

        let tx_signature_hash = new_tx.signature_hash();
        let signature = sign_message(
            B256::from_slice(self.payload_config.secret_key.as_ref()),
            tx_signature_hash,
        )
        .unwrap();
        let tx_signed = TransactionSigned::from_transaction_and_signature(new_tx, signature);

        let tx_recovered = tx_signed
            .into_ecrecovered()
            .expect("Failed to recover transaction");
        Ok(tx_recovered)
    }

    /// Verify that transaction can be added to current block state
    /// Takes transactions most recent simulation outcome
    pub fn verify_simulated_transaction(
        &self,
        simulated: &SimulatedTransaction,
    ) -> Result<(), BuildStateExecutionError> {
        // ensure we still have capacity for this transaction
        if self.exec_details.cumulative_gas_used
            + simulated.outcome().gas_used()
            + MAX_BUILDER_TRANSFER_GAS_LIMIT
            > self.build_config.block_gas_limit
        {
            // we can't fit this transaction into the block, so we need to mark it as invalid
            // which also removes all dependent transaction from the iterator before we can
            // continue
            return Err(BuildStateExecutionError::MaxGasExceeded);
        }

        // There's only limited amount of blob space available per block, so we need to check if the
        // EIP-4844 can still fit in the block
        if self.exec_details.cumulative_blob_gas_used + simulated.outcome().blob_gas_used()
            > MAX_DATA_GAS_PER_BLOCK
        {
            // we can't fit this _blob_ transaction into the block, so we mark it as invalid,
            // which removes its dependent transactions from the iterator. This is similar to
            // the gas limit condition for regular transactions above.
            return Err(BuildStateExecutionError::MaxBlobGasExceeded);
        }

        Ok(())
    }

    /// Verify that bundle can be added to current block state
    /// Takes transactions most recent simulation outcomes
    pub fn verify_simulated_bundle(
        &self,
        simulated: &SimulatedBundle,
    ) -> Result<(), BuildStateExecutionError> {
        let BundleSimulationOutcome {
            reverted,
            gas_used,
            blob_gas_used,
            ..
        } = simulated.outcome();

        // make sure no transactions reverted
        if let Some(revert_tx) = reverted {
            return Err(BuildStateExecutionError::BundleReverted(*revert_tx));
        }

        // ensure we still have capacity for this transaction
        if self.exec_details.cumulative_gas_used + gas_used + MAX_BUILDER_TRANSFER_GAS_LIMIT
            > self.build_config.block_gas_limit
        {
            // we can't fit this transaction into the block, so we need to mark it as invalid
            // which also removes all dependent transaction from the iterator before we can
            // continue
            return Err(BuildStateExecutionError::MaxGasExceeded);
        }

        // There's only limited amount of blob space available per block, so we need to check if the
        // EIP-4844 can still fit in the block
        if self.exec_details.cumulative_blob_gas_used + blob_gas_used > MAX_DATA_GAS_PER_BLOCK {
            // we can't fit this _blob_ transaction into the block, so we mark it as invalid,
            // which removes its dependent transactions from the iterator. This is similar to
            // the gas limit condition for regular transactions above.
            return Err(BuildStateExecutionError::MaxBlobGasExceeded);
        }

        Ok(())
    }

    /// Simulate transaction on current build state stack without modifying state
    /// Takes simulation db as an argument to extend state changes to this existing simulation
    /// environment
    ///
    /// Returns [`TransactionSimulationOutcome`] for proper handling by caller
    pub fn simulate_tx_on_db(
        &mut self,
        db: &mut SimulationDB<&mut ExecutionDB<'_>>,
        tx: TransactionSignedEcRecovered,
    ) -> Result<SimulatedTransaction, PayloadBuilderError> {
        // get coinbase balance before
        let coinbase_account_balance = db
            .basic(self.build_config.initialized_block_env.coinbase)?
            .map(|acct| acct.balance)
            .unwrap_or_default();
        let coinbase_balance_before = coinbase_account_balance;

        // Configure the environment for the block.
        let env = Env::boxed(
            self.build_config.initialized_cfg.cfg_env.clone(),
            self.build_config.initialized_block_env.clone(),
            Default::default(),
        );

        let mut evm = revm::Evm::builder().with_db(db).with_env(env).build();

        fill_tx_env_with_recovered(evm.tx_mut(), &tx);

        let ResultAndState {
            result: evm_result,
            state: evm_state,
        } = evm
            .transact()
            .map_err(PayloadBuilderError::EvmExecutionError)?;

        let gas_used = evm_result.gas_used();
        let blob_gas_used = tx
            .transaction
            .as_eip4844()
            .map(|tx| tx.blob_gas())
            .unwrap_or(0);

        let basefee = self.build_config.initialized_block_env.basefee;

        // update add to total fees
        let miner_fee = tx
            .effective_tip_per_gas(Some(basefee.to::<u64>()))
            .expect("fee is always valid; execution succeeded");
        let fees = U256::from(miner_fee) * U256::from(gas_used);

        // get coinbase balance after
        let coinbase_balance_after = evm_state
            .get(&self.build_config.initialized_block_env.coinbase)
            .map(|acct| acct.info.balance)
            .unwrap_or_default();
        let coinbase_diff = coinbase_balance_after.saturating_sub(coinbase_balance_before);
        let coinbase_transfer = coinbase_diff.saturating_sub(fees);

        Ok(SimulatedTransaction::new(
            tx,
            TransactionSimulationOutcome {
                evm_result,
                evm_state: evm_state.clone(),
                gas_used,
                blob_gas_used,
                fees,
                coinbase_transfer,
            },
        ))
    }

    /// Simulate transaction on current build state stack without modifying state
    /// Take execution db as an argument and creates a new simulation environment
    ///
    /// Returns [`TransactionSimulationOutcome`] for proper handling by caller
    pub fn simulate_tx(
        &mut self,
        db: &mut ExecutionDB<'_>,
        tx: TransactionSignedEcRecovered,
    ) -> Result<SimulatedTransaction, PayloadBuilderError> {
        let mut db = simulation_db(db);
        self.simulate_tx_on_db(&mut db, tx)
    }

    /// Simulate bundle transactions on current build state stack without modifying state
    ///
    /// Returns [`SimulatedBundle`] for proper handling by caller
    pub fn simulate_bundle(
        &mut self,
        db: &mut ExecutionDB<'_>,
        bundle: RecoveredBundle,
    ) -> Result<SimulatedBundle, BuildStateExecutionError> {
        let mut db = simulation_db(db);

        // get coinbase balance before
        let coinbase_account_balance = db
            .basic(self.build_config.initialized_block_env.coinbase)
            .map_err(PayloadBuilderError::from)?
            .map(|acct| acct.balance)
            .unwrap_or_default();
        let mut coinbase_balance_before = coinbase_account_balance;
        let mut coinbase_balance_after: U256;
        let mut simulated_txs = Vec::new();

        // Configure the environment for the block.

        let env = Env::boxed(
            self.build_config.initialized_cfg.cfg_env.clone(),
            self.build_config.initialized_block_env.clone(),
            Default::default(),
        );

        // apply pre-block EIP-4788 contract call
        let mut evm = revm::Evm::builder().with_db(db).with_env(env).build();

        for tx in bundle.recovered_txs() {
            fill_tx_env_with_recovered(evm.tx_mut(), &tx);

            let ResultAndState {
                result: evm_result,
                state: evm_state,
            } = evm
                .transact()
                .map_err(PayloadBuilderError::EvmExecutionError)?;

            let gas_used = evm_result.gas_used();
            let blob_gas_used = tx
                .transaction
                .as_eip4844()
                .map(|tx| tx.blob_gas())
                .unwrap_or(0);

            // update add to total fees
            let miner_fee = tx
                .effective_tip_per_gas(Some(self.build_config.basefee.to::<u64>()))
                .expect("fee is always valid; execution succeeded");
            let fees = U256::from(miner_fee) * U256::from(gas_used);

            // get coinbase balance after
            coinbase_balance_after = evm_state
                .get(&self.build_config.initialized_block_env.coinbase)
                .map(|acct| acct.info.balance)
                .unwrap_or_default();
            let coinbase_diff = coinbase_balance_after.saturating_sub(coinbase_balance_before);
            let coinbase_transfer = coinbase_diff.saturating_sub(fees);
            coinbase_balance_before = coinbase_balance_after;

            let outcome = TransactionSimulationOutcome {
                evm_result,
                evm_state: evm_state.clone(),
                gas_used,
                blob_gas_used,
                fees,
                coinbase_transfer,
            };

            if let Some(ofac_addr) = outcome.contains_ofac_addresses() {
                return Err(BuildStateExecutionError::OfacAddressDetected(ofac_addr));
            }

            simulated_txs.push(SimulatedTransaction::new(tx, outcome));

            evm.context.evm.db.commit(evm_state);
        }

        let mut simulated =
            SimulatedBundle::new(bundle, simulated_txs, self.build_config.basefee.to::<u64>());

        // simulate refund transaction if bundle requires a refund
        match self.simulate_refund_tx(&mut evm.context.evm.db, &simulated) {
            // if valid refund is simulated, update simulated bundle
            Ok(Some(refund_simulated_tx)) => simulated.add_tx(refund_simulated_tx),
            // propogate any error back up to caller, skip this bundle
            Err(err) => return Err(BuildStateExecutionError::BuilderRefundTx(err.to_string())),
            // no refund applicable, continue
            _ => {}
        }

        Ok(simulated)
    }

    /// Simulate refund transactions on current build state stack without modifying state
    /// Takes simulation db as an argument to extend refund transaction state changes to existing
    /// bundle simulation environment
    ///
    /// Returns [`SimulatedTransaction`] for proper handling by caller
    pub fn simulate_refund_tx(
        &mut self,
        mut db: &mut SimulationDB<&mut ExecutionDB<'_>>,
        simulated: &SimulatedBundle,
    ) -> Result<Option<SimulatedTransaction>, BuildStateExecutionError> {
        let refund = simulated.outcome().refund();

        // if no refund, return none
        if refund.is_zero() {
            return Ok(None);
        }

        // if no recipient found, return none
        let recipient = match simulated.recovered_bundle().refund_recipient() {
            Some(recipient) => recipient,
            None => return Ok(None),
        };

        // build refund transaction
        let refund_tx = self.create_builder_transfer_transaction(&mut db, recipient, refund)?;

        // simulate builder refund transaction on build state
        let refund_simulated_tx = self.simulate_tx_on_db(db, refund_tx)?;

        // ensure refund tx does not revert
        if !refund_simulated_tx.outcome().evm_result.is_success() {
            return Err(BuildStateExecutionError::BuilderRefundTx(
                "Transaction reverted".to_string(),
            ));
        }

        Ok(Some(refund_simulated_tx))
    }

    /// Commit simulation outcomes on current build state stack
    ///
    /// Expects only successfull simulation outcomes on the current stack
    pub fn commit<I>(&mut self, db: &mut ExecutionDB<'_>, simulated: I)
    where
        I: IntoIterator<Item = SimulatedTransaction>,
    {
        for tx_simulated in simulated {
            let tx = tx_simulated.tx().clone();
            let TransactionSimulationOutcome {
                evm_result,
                evm_state,
                gas_used,
                blob_gas_used,
                fees,
                coinbase_transfer,
            } = tx_simulated.outcome().clone();

            // commit changes
            db.commit(evm_state);

            self.exec_details.cumulative_gas_used += gas_used;
            self.exec_details.cumulative_blob_gas_used += blob_gas_used;
            self.exec_details.sum_fees += fees;
            self.exec_details.sum_coinbase_transfers += coinbase_transfer;

            // Push transaction changeset and calculate header bloom filter for receipt.
            self.exec_details.receipts.push(Some(Receipt {
                tx_type: tx.tx_type(),
                success: evm_result.is_success(),
                cumulative_gas_used: self.exec_details.cumulative_gas_used,
                logs: evm_result.into_logs().into_iter().collect(),
            }));

            // append transaction to the list of executed transactions
            self.exec_details.executed_txs.push(tx.into_signed());
        }
    }

    /// Execute transaction on current build state stack
    /// Responsible for both simulating and committing if successfull
    ///
    /// Extends parameters to include skip verify option
    ///
    /// Returns [`BuildStateExecutionError`] for proper handling if transaction fails
    pub fn execute_transaction_skip_verify(
        &mut self,
        db: &mut ExecutionDB<'_>,
        tx: TransactionSignedEcRecovered,
        skip_verify: bool,
    ) -> Result<(), BuildStateExecutionError> {
        // simulate transaction
        let simulated = self.simulate_tx(db, tx)?;

        if !skip_verify {
            // verify transaction can be added to block
            self.verify_simulated_transaction(&simulated)?;
        }

        // commit simulated transaction to db
        self.commit(db, Some(simulated));

        Ok(())
    }

    /// Execute transaction on current build state stack
    /// Responsible for both simulating and committing if successfull
    ///
    /// Returns [`BuildStateExecutionError`] for proper handling if transaction fails
    pub fn execute_transaction(
        &mut self,
        db: &mut ExecutionDB<'_>,
        tx: TransactionSignedEcRecovered,
    ) -> Result<(), BuildStateExecutionError> {
        self.execute_transaction_skip_verify(db, tx, false)
    }

    /// Execute bundle of transactions on current build state stack
    /// Responsible for both simulating and committing if successfull
    ///
    /// Returns [`BuildStateExecutionError`] for proper handling if transaction fails
    pub fn execute_bundle(
        &mut self,
        db: &mut ExecutionDB<'_>,
        bundle: RecoveredBundle,
    ) -> Result<(), BuildStateExecutionError> {
        // simulate transactions
        let simulated = self.simulate_bundle(db, bundle)?;

        // verify bundle can be added to block
        self.verify_simulated_bundle(&simulated)?;

        // commit simulated bundle to db
        self.commit(db, simulated.simulated_txs());

        Ok(())
    }

    /// Convert [`BuildState`] into a final [`Build`]
    ///
    /// Consumed BuildState and responsible for computing all block parameters
    pub fn into_build<Client, Pool>(
        self,
        client: Client,
        pool: Pool,
    ) -> Result<Build, PayloadBuilderError>
    where
        Client: StateProviderFactory,
        Pool: TransactionPool,
    {
        let mut this = self.clone();

        let BuildState {
            id,
            payload_config,
            build_config,
            mut exec_details,
            ..
        } = self;

        let state_provider = client.state_by_block_hash(build_config.parent_block.hash())?;
        let state = StateProviderDatabase::new(state_provider);

        let mut cached_reads = CachedReads::default();
        let bundle_state = exec_details.bundle_state.take().unwrap_or_default();
        let mut db = State::builder()
            .with_database_ref(cached_reads.as_db(&state))
            .with_bundle_update()
            .with_bundle_prestate(bundle_state)
            .build();

        let builder_payment = exec_details.builder_payment.unwrap_or(U256::ZERO);
        let builder_payment_tx = this.create_builder_transfer_transaction(
            &mut db,
            build_config.validator_info.fee_recipient,
            builder_payment,
        )?;

        // execute builder payment transaction on build state
        this.execute_transaction_skip_verify(&mut db, builder_payment_tx.clone(), true)
            .map_err(|err| {
                error!(
                    target: "payload::builder",
                    slot=build_config.slot,
                    ?err,
                    "Builder payment tx failed, skipping bid."
                );
                // TODO: Refactor error handling: State must have its own enum with its own errors
                PayloadBuilderError::Internal(reth_interfaces::RethError::Custom(format!(
                    "Builder payment tx failed: {err:?}"
                )))
            })?;

        let WithdrawalsOutcome {
            withdrawals_root,
            withdrawals,
        } = BuildState::commit_withdrawals(
            &mut db,
            &payload_config.chain_spec,
            build_config.attributes.timestamp,
            build_config.attributes.withdrawals.clone(),
        )?;

        // merge all transitions into bundle state, this would apply the withdrawal balance changes and
        // 4788 contract call
        db.merge_transitions(BundleRetention::PlainState);

        let bundle = BundleStateWithReceipts::new(
            db.take_bundle(),
            Receipts::from_vec(vec![this.exec_details.receipts]),
            build_config.block_number,
        );
        let receipts_root = bundle
            .receipts_root_slow(build_config.block_number)
            .expect("Number is in range");
        let logs_bloom = bundle
            .block_logs_bloom(build_config.block_number)
            .expect("Number is in range");

        // calculate the state root
        let state_provider = client.state_by_block_hash(build_config.parent_block.hash())?;
        let state_root = state_provider.state_root(bundle.state())?;

        // create the block header
        let transactions_root = proofs::calculate_transaction_root(&this.exec_details.executed_txs);

        // initialize empty blob sidecars at first. If cancun is active then this will
        let mut blob_sidecars = Vec::new();
        let mut excess_blob_gas = None;
        let mut blob_gas_used = None;

        // only determine cancun fields when active
        if payload_config
            .chain_spec
            .is_cancun_active_at_timestamp(build_config.attributes.timestamp)
        {
            // grab the blob sidecars from the executed txs
            blob_sidecars = pool.get_all_blobs_exact(
                this.exec_details
                    .executed_txs
                    .iter()
                    .filter(|tx| tx.is_eip4844())
                    .map(|tx| tx.hash)
                    .collect(),
            )?;

            excess_blob_gas = if payload_config
                .chain_spec
                .is_cancun_active_at_timestamp(build_config.parent_block.timestamp)
            {
                let parent_excess_blob_gas = build_config
                    .parent_block
                    .excess_blob_gas
                    .unwrap_or_default();
                let parent_blob_gas_used =
                    build_config.parent_block.blob_gas_used.unwrap_or_default();
                Some(calculate_excess_blob_gas(
                    parent_excess_blob_gas,
                    parent_blob_gas_used,
                ))
            } else {
                // for the first post-fork block, both parent.blob_gas_used and parent.excess_blob_gas
                // are evaluated as 0
                Some(calculate_excess_blob_gas(0, 0))
            };

            blob_gas_used = Some(this.exec_details.cumulative_blob_gas_used);
        }

        let header = Header {
            parent_hash: build_config.parent_block.hash(),
            ommers_hash: EMPTY_OMMER_ROOT_HASH,
            beneficiary: build_config.initialized_block_env.coinbase,
            state_root,
            transactions_root,
            receipts_root,
            withdrawals_root,
            logs_bloom,
            timestamp: build_config.attributes.timestamp,
            mix_hash: build_config.attributes.prev_randao,
            nonce: BEACON_NONCE,
            base_fee_per_gas: Some(build_config.basefee.to::<u64>()),
            number: build_config.parent_block.number + 1,
            gas_limit: build_config.block_gas_limit,
            difficulty: U256::ZERO,
            gas_used: this.exec_details.cumulative_gas_used,
            extra_data: payload_config.extra_data.clone(),
            parent_beacon_block_root: build_config.attributes.parent_beacon_block_root,
            blob_gas_used,
            excess_blob_gas,
        };

        // seal the block
        let block = Block {
            header,
            body: this.exec_details.executed_txs,
            ommers: vec![],
            withdrawals,
        };

        let sealed_block = block.seal_slow();

        let mut payload = EthBuiltPayload::new(
            build_config.attributes.id,
            sealed_block,
            this.exec_details.sum_fees,
        );

        // extend the payload with the blob sidecars from the executed txs
        payload.extend_sidecars(blob_sidecars);

        // construct finalized build
        let payload = Build {
            id,
            validator_info: build_config.validator_info.clone(),
            payload,
            bid: builder_payment,
        };

        Ok(payload)
    }
    /// Convert [`BuildState`] into an empty [`Build`]
    pub fn into_empty_build<Client, Pool>(
        self,
        client: Client,
        pool: Pool,
    ) -> Result<Build, PayloadBuilderError>
    where
        Client: StateProviderFactory,
        Pool: TransactionPool,
    {
        let this = self.clone();

        let BuildState {
            id,
            build_config,
            mut exec_details,
            payload_config,
            ..
        } = self;

        let parent_hash = build_config.parent_block.hash();

        let state_provider = client.state_by_block_hash(parent_hash)?;
        let state = StateProviderDatabase::new(state_provider);

        let mut cached_reads = CachedReads::default();
        let bundle_state = exec_details.bundle_state.take().unwrap_or_default();
        let mut db = State::builder()
            .with_database_ref(cached_reads.as_db(&state))
            .with_bundle_update()
            .with_bundle_prestate(bundle_state)
            .build();

        let WithdrawalsOutcome {
            withdrawals_root,
            withdrawals,
        } = BuildState::commit_withdrawals(
            &mut db,
            &payload_config.chain_spec,
            build_config.attributes.timestamp,
            build_config.attributes.withdrawals.clone(),
        )?;

        // merge all transitions into bundle state, this would apply the withdrawal balance changes and
        // 4788 contract call
        db.merge_transitions(BundleRetention::PlainState);

        let bundle = BundleStateWithReceipts::new(
            db.take_bundle(),
            Receipts::from_vec(vec![this.exec_details.receipts]),
            build_config.block_number,
        );

        // calculate the state root
        let state_provider = client.state_by_block_hash(parent_hash)?;
        let state_root = state_provider.state_root(bundle.state())?;

        // initialize empty blob sidecars at first. If cancun is active then this will
        let mut blob_sidecars = Vec::new();
        let mut excess_blob_gas = None;
        let mut blob_gas_used = None;

        // only determine cancun fields when active
        if payload_config
            .chain_spec
            .is_cancun_active_at_timestamp(build_config.attributes.timestamp)
        {
            info!("cancun payload");
            // grab the blob sidecars from the executed txs
            blob_sidecars = pool.get_all_blobs_exact(
                this.exec_details
                    .executed_txs
                    .iter()
                    .filter(|tx| tx.is_eip4844())
                    .map(|tx| tx.hash)
                    .collect(),
            )?;

            excess_blob_gas = if payload_config
                .chain_spec
                .is_cancun_active_at_timestamp(build_config.parent_block.timestamp)
            {
                let parent_excess_blob_gas = build_config
                    .parent_block
                    .excess_blob_gas
                    .unwrap_or_default();
                let parent_blob_gas_used =
                    build_config.parent_block.blob_gas_used.unwrap_or_default();
                Some(calculate_excess_blob_gas(
                    parent_excess_blob_gas,
                    parent_blob_gas_used,
                ))
            } else {
                // for the first post-fork block, both parent.blob_gas_used and parent.excess_blob_gas
                // are evaluated as 0
                Some(calculate_excess_blob_gas(0, 0))
            };

            blob_gas_used = Some(this.exec_details.cumulative_blob_gas_used);
        }
        let header = Header {
            parent_hash: build_config.parent_block.hash(),
            ommers_hash: EMPTY_OMMER_ROOT_HASH,
            beneficiary: build_config.validator_info.fee_recipient,
            state_root,
            transactions_root: EMPTY_TRANSACTIONS,
            withdrawals_root,
            receipts_root: EMPTY_RECEIPTS,
            logs_bloom: Default::default(),
            timestamp: build_config.attributes.timestamp,
            mix_hash: build_config.attributes.prev_randao,
            nonce: BEACON_NONCE,
            base_fee_per_gas: Some(build_config.basefee.to::<u64>()),
            number: build_config.parent_block.number + 1,
            gas_limit: this.build_config.block_gas_limit,
            difficulty: U256::ZERO,
            gas_used: 0,
            extra_data: payload_config.extra_data.clone(),
            blob_gas_used,
            excess_blob_gas,
            parent_beacon_block_root: build_config.attributes.parent_beacon_block_root,
        };

        let block = Block {
            header,
            body: vec![],
            ommers: vec![],
            withdrawals,
        };
        let sealed_block = block.seal_slow();

        let mut payload = EthBuiltPayload::new(
            build_config.attributes.id,
            sealed_block,
            this.exec_details.sum_fees,
        );

        // extend the payload with the blob sidecars from the executed txs
        payload.extend_sidecars(blob_sidecars);

        // construct finalized build
        let payload = Build {
            id,
            validator_info: build_config.validator_info.clone(),
            payload,
            bid: U256::ZERO,
        };

        Ok(payload)
    }
}

impl From<Arc<BuildState>> for BuildState {
    fn from(value: Arc<BuildState>) -> Self {
        (*value).clone()
    }
}

impl fmt::Debug for BuildState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "build_id = {:?}, slot = {}, block_number = {}",
            self.id, self.build_config.slot, self.build_config.block_number
        )
    }
}

#[cfg(test)]
mod tests {
    use revm_primitives::{address, Address, ExecutionResult, Output, SuccessReason};

    use crate::strategy::state::transaction::TransactionSimulationOutcome;

    #[test]
    fn test_ofac_address_in_simulation_outcome() {
        let ofac_address = address!("8576acc5c05d6ce88f4e49bf65bdf0c62f91353c");
        let non_ofac_address = Address::random();

        let evm_result = ExecutionResult::Success {
            reason: SuccessReason::Return,
            gas_used: 0,
            gas_refunded: 0,
            logs: vec![],
            output: Output::Call(revm_primitives::Bytes::default()),
        };
        let sim_outcome = TransactionSimulationOutcome {
            evm_result: evm_result.clone(),
            evm_state: vec![(ofac_address, Default::default())]
                .into_iter()
                .collect(),
            gas_used: 0,
            blob_gas_used: 0,
            fees: Default::default(),
            coinbase_transfer: Default::default(),
        };
        assert!(sim_outcome.contains_ofac_addresses().is_some());

        let sim_outcome = TransactionSimulationOutcome {
            evm_result,
            evm_state: vec![(non_ofac_address, Default::default())]
                .into_iter()
                .collect(),
            gas_used: 0,
            blob_gas_used: 0,
            fees: Default::default(),
            coinbase_transfer: Default::default(),
        };
        assert!(sim_outcome.contains_ofac_addresses().is_none());
    }
}
