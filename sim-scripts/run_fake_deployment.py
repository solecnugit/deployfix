import os
import logging
import subprocess
import sys
import yaml
import kubernetes as k8s
import kubernetes.config as k8sConfig
import kubernetes.client as k8sClient
import kubernetes.utils as k8sUtils
import urllib3
import time
import json
import numpy as np

from typing import Optional

urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)


def uniform(a, b):
    return np.random.randint(a, b)


# Set up logging
def setup_logging():
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(message)s",
        handlers=[logging.StreamHandler()],
    )

def setup_k8s_client():
    k8sConfig.load_kube_config()

    # Disable SSL verification
    client = k8sClient.ApiClient()
    client.configuration.verify_ssl = False

    client = k8sClient.ApiClient(configuration=client.configuration)
    api = k8sClient.CoreV1Api(client)

    return client, api

setup_logging()

k8s_client, k8s_api = setup_k8s_client()

def run_command(cmd: str, stdin: Optional[str], timeout: int = 5):
    logging.info(f"Running command: {cmd}")

    process = subprocess.Popen(
        cmd.split(" "), stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.PIPE
    )

    if stdin and process.stdin:
        process.stdin.write(stdin.encode("utf-8"))
        process.stdin.close()

    ret = None
    if timeout:
        try:
            ret = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            process.kill()
            raise

    if not process.stdout:
        stdout = ""
    else:
        stdout = process.stdout.read().decode("utf-8")

    if not process.stderr:
        stderr = ""
    else:
        stderr = process.stderr.read().decode("utf-8")

    if stdout:
        logging.info(stdout)
    if stderr:
        logging.error(stderr)

    if ret != 0:
        raise RuntimeError(f"Command failed: {cmd}")

    return stdout, stderr

NODE_TAGS_VALUE_COUNT = 10
NODE_TAG_TYPES_LIMIT = 5
NODE_TAG_VALUE_LIMIT = 4
NODE_RULE_LIMIT = 2

def create_node(node_name: str):
    fake_node_spec = f"""
        apiVersion: v1
        kind: Node
        metadata:
            annotations:
                node.alpha.kubernetes.io/ttl: "0"
                kwok.x-k8s.io/node: fake
            labels:
                beta.kubernetes.io/arch: amd64
                beta.kubernetes.io/os: linux
                kubernetes.io/arch: amd64
                kubernetes.io/hostname: {node_name}
                kubernetes.io/os: linux
                kubernetes.io/role: agent
                node-role.kubernetes.io/agent: ""
                type: kwok
            name: {node_name}
        status:
            allocatable:
                cpu: 32
                memory: 256Gi
                pods: 110
            capacity:
                cpu: 32
                memory: 256Gi
                pods: 110
            nodeInfo:
                architecture: amd64
                bootID: ""
                containerRuntimeVersion: ""
                kernelVersion: ""
                kubeProxyVersion: fake
                kubeletVersion: fake
                machineID: ""
                operatingSystem: linux
                osImage: ""
                systemUUID: ""
            phase: Running
    """

    yaml_dict = yaml.safe_load(fake_node_spec)
    for i in range(uniform(0, NODE_RULE_LIMIT * 10)):
        tag = f"tag{uniform(1, NODE_TAG_TYPES_LIMIT)}"
        val = f"value{uniform(1, NODE_TAG_VALUE_LIMIT)}"
        yaml_dict["metadata"]["labels"][tag] = val

    k8sUtils.create_from_dict(k8s_client, yaml_dict)

def create_pod_from_file(yaml_file_path: str):
    with open(yaml_file_path, "r") as f:
        yaml_dict = yaml.safe_load(f)
        k8sUtils.create_from_dict(k8s_client, yaml_dict)

def create_pod_from_dir(yaml_dir_path: str):
    for file in os.listdir(yaml_dir_path):
        if file.endswith(".yaml"):
            create_pod_from_file(os.path.join(yaml_dir_path, file))

def cleanup():
    def drop_all_nodes():
        nodes = k8s_api.list_node(watch=False)
        nodes = nodes.items

        for node in nodes:
            k8s_api.delete_node(node.metadata.name)

    def drop_all_pods():
        pods = k8s_api.list_pod_for_all_namespaces(watch=False)
        pods = pods.items

        for pod in pods:
            k8s_api.delete_namespaced_pod(pod.metadata.name, pod.metadata.namespace)

    drop_all_pods()
    drop_all_nodes()


def setup(node_size: int = 200):
    for i in range(node_size):
        create_node(f"node{i}")

def collect_pending_pods():
    pods = k8s_api.list_pod_for_all_namespaces(watch=False)
    pods = list(pods.items)

    if not pods:
        logging.warning("No pods found")
        raise RuntimeError()

    pending_pods = set()
    running_pods = set()

    for pod in pods:
        pod_name = pod.metadata.name.split("-")[0]
        if pod.status.phase == "Pending":
            pending_pods.add(pod_name)
        elif pod.status.phase == "Running":
            running_pods.add(pod_name)
        else:
            raise RuntimeError(f"Unknown pod status: {pod.status.phase}")

    ret = list(pending_pods - running_pods)

    return ret

def collect_pod_distribution():
    pods = k8s_api.list_pod_for_all_namespaces(watch=False)
    pods = pods.items

    pod_distribution = {}

    for pod in pods:
        if pod.status.phase == "Running":
            pod_distribution[pod.metadata.name] = pod.spec.node_name

    return pod_distribution

def main():
    args = sys.argv[1:]

    if len(args) == 0 or args[0] != "--skip-setup":
        logging.info("Starting fake deployment")

        cleanup()
        setup(node_size=120)
        logging.info("Nodes created")

        create_pod_from_dir("sample/dataset")
        logging.info("Pods created")
        logging.info("Waiting for 60 seconds for scheduling")

        time.sleep(60)

    pending_pods = None

    for rety in range(3):
        try:
            pending_pods = collect_pending_pods()
        except RuntimeError as e:
            logging.error(
                f"No pods found, some errors may occured due to kwok, check the logs, retry: {rety}/3"
            )
            logging.error("Waiting for another 60 seconds")

        time.sleep(60)

    if pending_pods is None:
        raise RuntimeError("some errors may occured due to kwok, check the logs")

    with open("output/pending_pods.json", "w") as f:
        json.dump(pending_pods, f)

if __name__ == "__main__":
    main()
