import yaml
import os
import json

def load_outputs():
    if not os.path.exists("output/conflicts-node.yaml"):
        deployfix_conflict_names = []
    else: 
        with open("output/conflicts-node.yaml", "r") as r:
            deployfix_yaml = yaml.safe_load(r)

            deployfix_conflict_names = deployfix_yaml["unscheduable_entities"]
            deployfix_conflict_names = [value["name"] for value in deployfix_conflict_names]
            deployfix_conflict_names = [name.split("=")[1] for name in deployfix_conflict_names]
            deployfix_conflict_names = [name.split("_")[0] for name in deployfix_conflict_names]

    with open("output/pending_pods.json", "r") as r:
        k8s_pending_pods = json.load(r)
        k8s_pending_pods = [name.split("-")[0] for name in k8s_pending_pods]

    return deployfix_conflict_names, k8s_pending_pods

def check_subset():
    deployfix_conflict_names, k8s_pending_pods = load_outputs()

    deployfix_conflict_names = set(deployfix_conflict_names)
    k8s_pending_pods = set(k8s_pending_pods)

    print(f"deployfix_conflict_names: {deployfix_conflict_names}")
    print(f"k8s_pending_pods: {k8s_pending_pods}")

    flag = deployfix_conflict_names.issubset(k8s_pending_pods)
    if not flag:
        print(f"Missing pods: {deployfix_conflict_names - k8s_pending_pods}")

    return flag

def main():
    flag = check_subset()

    if flag:
        print("deployfix conflicts is a subset of k8s")
    else:
        print("deployfix conflicts is not a subset of k8s")

    exit(0 if flag else 1)

if __name__ == "__main__":
    main()
