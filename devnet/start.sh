#!/bin/bash

# Reth db path
# As deafult reth returns the mainnet path
# We need to change it to the devnet path
DB_PATH=$(cargo run --bin builder db path)
echo "Reth mainnet db path is " $DB_PATH
CHAIN_ID="32382"
# Replace mainnet with devnet chain_id
DB_PATH="${DB_PATH/mainnet/$CHAIN_ID}"
# Remove db substring from path
DB_PATH="${DB_PATH/db/}"
echo "Reth devnet db path is " $DB_PATH

# Cleans txpool: this file is written on every reth shutdown, 
# so we must clear it to avoid loading unrelated transactions between runs
rm -rf "${DB_PATH}txpool-transactions-backup.rlp"

# Check if reth db does not exists and skip dropping
if [ ! -d "$DB_PATH/db" ]; then
    echo "Reth db does not exist, will not clear it"
    ./devnet/cmd/run_devnet.sh
    exit 1
fi

echo "Clearing reth db at " $DB_PATH

# Cleans reth db
echo "y" | cargo run --bin builder db drop --datadir "$DB_PATH"

echo "Cleared reth db and txpool"

# Runs devnet
./devnet/cmd/run_devnet.sh