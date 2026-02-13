use reth::{
    primitives::{Address, B256, U256},
    providers::StateProvider,
};

use reth::revm::State;
use reth_payload_builder::database::CachedReadsDBRef;
use reth_revm::database::StateProviderDatabase;
use revm::{db::CacheDB, Database, DatabaseRef};
use revm_primitives::{db::WrapDatabaseRef, AccountInfo, Bytecode};
use std::cell::RefCell;

/// EVM Execution database
/// Type of EVM State built on database ref of local cache
/// Cache points to state provider
pub type ExecutionDB<'db> = State<
    WrapDatabaseRef<
        CachedReadsDBRef<'db, &'db StateProviderDatabase<Box<dyn StateProvider + 'db>>>,
    >,
>;

/// Reference wrapper type for an execution database
pub struct ExecutionDBRef<DB: Database> {
    inner: RefCell<DB>,
}

impl<DB> From<DB> for ExecutionDBRef<DB>
where
    DB: Database,
{
    fn from(value: DB) -> Self {
        ExecutionDBRef {
            inner: RefCell::new(value),
        }
    }
}

impl<DB: Database> DatabaseRef for ExecutionDBRef<DB> {
    type Error = <DB as Database>::Error;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.inner.borrow_mut().basic(address)
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.inner.borrow_mut().code_by_hash(code_hash)
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.inner.borrow_mut().storage(address, index)
    }

    fn block_hash_ref(&self, number: U256) -> Result<B256, Self::Error> {
        self.inner.borrow_mut().block_hash(number)
    }
}

/// Type for wrapped database environment on top of an execution database
pub type SimulationDB<DB> = CacheDB<ExecutionDBRef<DB>>;

/// Wrap execution database in new cache database for simulation
pub fn simulation_db<DB: Database>(db: DB) -> SimulationDB<DB> {
    CacheDB::new(ExecutionDBRef::from(db))
}
