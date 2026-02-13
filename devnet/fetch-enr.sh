#!/bin/sh

# Install curl and jq
apk add --no-cache curl jq

# Wait for the beacon-chain service to be fully up and operational
echo "Waiting for beacon-chain service..."
while ! curl -s http://beacon-chain:3500/eth/v1/node/identity > /dev/null 2>&1; do
  sleep 1
done

# Fetch the ENR value
echo "Fetching ENR..."
ENR=$(curl -s http://beacon-chain:3500/eth/v1/node/identity | jq -r '.data.enr')

echo "Fetched ENR: $ENR"

# Write the ENR to the shared volume
echo $ENR > /shared-data/bootstrap_node.txt
echo "ENR written to /shared-data/bootstrap_node.txt"

REREAD_ENR=$(cat /shared-data/bootstrap_node.txt)
echo "Reread ENR: $REREAD_ENR"