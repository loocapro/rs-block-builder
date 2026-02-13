#!/bin/sh

# Install curl and jq
apk add --no-cache curl jq

# Define the JSON-RPC request body
REQUEST_BODY='{
    "jsonrpc":"2.0",
    "method":"admin_nodeInfo",
    "params":[],
    "id":1
}'

# URL of the geth JSON-RPC endpoint
GETH_JSONRPC_URL="http://localhost:8555"

# Wait for the geth service to be fully up and operational
echo "Waiting for geth service..."
while ! curl -s --header "Content-Type: application/json" --data "${REQUEST_BODY}" ${GETH_JSONRPC_URL} > /dev/null 2>&1; do
  sleep 1
done

# Fetch the ENODE value
echo "Fetching ENODE..."
ENODE=$(curl -s --location --header "Content-Type: application/json" --data "${REQUEST_BODY}" ${GETH_JSONRPC_URL} | jq -r '.result.enode')

echo "Fetched enode: $ENODE"
# Write the enode to the shared volume
echo $ENODE > ./devnet/execution/bootstrap_node.txt
echo "enode written to /execution/bootstrap_node.txt"

REREAD_ENODE=$(cat ./devnet/execution/bootstrap_node.txt)
echo "Reread enode: $REREAD_ENODE"
