#!/bin/bash

cluster_name=deployfix

# Check if kwokctl is installed
if ! command -v kwokctl &>/dev/null; then
  echo "kwokctl could not be found"
  exit
fi

# Check if kubectl is installed
if ! command -v kubectl &>/dev/null; then
  echo "kubectl could not be found"
  exit
fi

# Check if python is installed
if ! command -v python &>/dev/null; then
  echo "python could not be found"
  exit
fi

# Check if cargo is installed
if ! command -v cargo &>/dev/null; then
  echo "cargo could not be found"
  exit
fi

recreate_cluster() {
  echo "Recreating cluster $cluster_name"
  delete_cluster
  create_cluster
}

create_cluster() {
  echo "Creating cluster $cluster_name"
  kwokctl create cluster --disable-qps-limits --kube-admission=false --kube-authorization=false --runtime=binary --name=$cluster_name
}

delete_cluster() {
  echo "Deleting cluster $cluster_name"
  kwokctl delete cluster --name=$cluster_name
}

recreate_output() {
  echo "Recreating output folder"
  rm -rf output
  mkdir output
}

# Check cluster if already exists
clusters=$(kwokctl get clusters)
# split string into array
IFS=$'\n' read -rd '' -a clusters <<<"$clusters"
for cluster in "${clusters[@]}"; do
  if [[ $cluster == *"$cluster_name"* ]]; then
    echo "Cluster $cluster_name already exists, recreate it first"
    delete_cluster
  fi
done

create_cluster

# Switch to cluster context
echo "Switching to cluster context"
kubectl config use-context kwok-$cluster_name

# Remove output folder
recreate_output

sleep 3

# Run dataset generation
echo "Running dataset generation"
python sim-scripts/generate_dataset.py

# Run Fake Deployment
echo "Running fake deployment"
python sim-scripts/run_fake_deployment.py

exit_code=$?
if [ $exit_code -ne 0 ]; then
  echo "Error: Fake deployment failed"
  exit 1
fi

# Run deployfix
echo "Running DeployFix"
RUST_LOG=info cargo run --release -- --log-dir output k8s go sample/dataset sample/dataset output --recommend --cycle-check

exit_code=$?
error_flag=0
not_match_flag=0
if [ $exit_code -ne 0 ]; then
  error_flag=1
fi

# Delete cluster
# delete_cluster

# Check subset
python sim-scripts/check_result.py
exit_code=$?
if [ $exit_code -ne 0 ]; then
  not_match_flag=1
  error_flag=1
fi

# Output results
echo "Result is stored in output folder"
if [ $error_flag -eq 1 ]; then
  echo -e "\e[31mError: deployfix found conflicts\e[0m"
  echo "Error: Affinity and anti-affinity rules are not satisfied"

  if [ $not_match_flag -eq 1 ]; then
    echo -e "\e[31mError: deployfix match failed\e[0m"
    echo "Error: Affinity and anti-affinity rules results are not matched with k8s"
    exit 1
  else
    echo -e "\e[32mSuccess: deployfix match succeeded\e[0m"
    echo "Success: Affinity and anti-affinity rules results are matched with k8s"
    exit 0
  fi

  exit 1
else
  echo -e "\e[32mSuccess: deployfix found no conflict\e[0m"
  echo "Success: Affinity and anti-affinity rules are satisfied"
  exit 0
fi
