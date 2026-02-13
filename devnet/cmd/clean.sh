#!/bin/bash
echo "Will stop all containers"
sudo docker compose -f ./devnet/docker-compose.yml down

echo "Stopped all containers"
rm -rf ./devnet/execution/bootstrap_node.txt
# Remove other specified files and directories
rm -rf ./devnet/consensus/beacondata ./devnet/consensus/beacondata-two ./devnet/consensus/validatordata ./devnet/consensus/genesis.ssz ./devnet/consensus/beacondata-payload-validator
echo "Removed old files"