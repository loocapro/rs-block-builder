#!/bin/bash
# Define the node executable and the arguments
NODE_CMD="cargo run --bin builder node"

# Source the .env file to load environment variables
if [ -f ./devnet/.env ]; then
    source ./devnet/.env
else
    echo "Error: .env file not found."
    exit 1
fi

# Read the BOOT_ENODE value from the file
if [ -f ./devnet/execution/bootstrap_node.txt ]; then
    BOOT_NODE=$(<./devnet/execution/bootstrap_node.txt)
else
    echo "Error: bootstrap_node.txt file not found."
    exit 1
fi

ARGS="--chain ./devnet/execution/genesis.json \
--txpool.no-local-transactions-propagation \
--discovery.port 30304 \
--metrics 0.0.0.0:9005 \
--authrpc.addr 0.0.0.0 \
--authrpc.port 8551 \
--authrpc.jwtsecret ./devnet/jwtsecret/jwt.hex \
--http \
--http.addr 0.0.0.0 \
--http.port 8545 \
--http.api eth \
--builder \
--builder.enable-rpc"

export RUST_LOG="info"

# reth::cli=debug,rpc-ext=debug,builder::service=debug,consensus_layer=debug,payload=debug,relay=debug,mev_share_sse=debug,tx-network=debug,bundle_pool=debug,relay::aggregator=off,payload::bidder::service=off,payload::job::stream=off

echo "Starting reth builder node"

# Source the .env file to load environment variables
if [ -f ./devnet/.env ]; then
    source ./devnet/.env
else
    echo "Error: .env file not found."
    exit 1
fi

# Check if environment type is set to local
if [ "$ENV_TYPE" == "local" ]; then
    # Execute the node command with the arguments (foreground)
    $NODE_CMD $ARGS
else
    # Execute the node command with the arguments (background)
    $NODE_CMD $ARGS &
    PID=$!

    # Write the PID to a file
    echo $PID > /tmp/reth_builder_pid.txt

    echo "Started in background with PID $PID"
fi