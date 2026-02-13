use reth::primitives::{Address, TransactionSignedEcRecovered, U256};
use revm_primitives::{Account, ExecutionResult, HashMap};

use crate::utils::ofac_addresses::Ofac;

/// Simulated transaction that encapsulates recovered transaction and simulation outcome
#[derive(Debug, Clone)]
pub struct SimulatedTransaction {
    /// Recovered transaction
    tx: TransactionSignedEcRecovered,
    /// Tx simulation outcome
    outcome: TransactionSimulationOutcome,
}

impl SimulatedTransaction {
    /// Create new SimulatedTransaction struct
    pub fn new(tx: TransactionSignedEcRecovered, outcome: TransactionSimulationOutcome) -> Self {
        Self { tx, outcome }
    }

    /// Get transaction
    pub fn tx(&self) -> &TransactionSignedEcRecovered {
        &self.tx
    }

    /// Get bundle outcome
    pub fn outcome(&self) -> &TransactionSimulationOutcome {
        &self.outcome
    }
}

/// Outcome of simulation
/// Encapsulates evm result and state
/// Calculates gas used and fees paid
#[derive(Debug, Clone)]
pub struct TransactionSimulationOutcome {
    /// Result of EVM simulation
    pub evm_result: ExecutionResult,
    /// State of EVM simulation
    pub evm_state: HashMap<Address, Account>,
    /// Gas used during simulation
    pub gas_used: u64,
    /// Blob gas used during simulation
    pub blob_gas_used: u64,
    /// Priority fees calculated
    pub fees: U256,
    /// Coinbase transfers detected
    pub coinbase_transfer: U256,
}

impl TransactionSimulationOutcome {
    /// Get gas used
    pub fn gas_used(&self) -> u64 {
        self.gas_used
    }

    /// Get blob gas used
    pub fn blob_gas_used(&self) -> u64 {
        self.blob_gas_used
    }

    /// Get fees
    pub fn fees(&self) -> U256 {
        self.fees
    }

    /// Get coinbase transfer
    pub fn coinbase_transfer(&self) -> U256 {
        self.coinbase_transfer
    }

    /// Check internal calls interacted with ofac addresses and return the first found
    pub fn contains_ofac_addresses(&self) -> Option<Address> {
        self.evm_state
            .iter()
            .find(|(address, _)| address.contains_ofac_addresses())
            .map(|(address, _)| *address)
    }
}
