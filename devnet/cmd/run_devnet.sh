# Cleans docker containers
./devnet/cmd/clean.sh
# Runs CL and validator
sudo -E docker compose -f ./devnet/docker-compose.yml up -d
echo "Started all containers"
# Writes geth enode to file
if ./devnet/cmd/fetch-enode.sh; then
    echo "ENODE successfully fetched and written."
else
    echo "Failed to fetch ENODE."
    exit 1
fi
# Runs the EL
./devnet/cmd/run_reth_builder.sh