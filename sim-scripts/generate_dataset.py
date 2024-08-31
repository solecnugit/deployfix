from dataclasses import dataclass
from typing import Any, List, Literal, Optional

import yaml
import numpy as np
import random as rd
import os
import os.path

np.random.seed(2)
rd.seed(2)

target_dir = "sample/dataset"

N = 120

REQUIRE_VALUE_LIMIT_PER_APP = 0.25
EXCLUDE_VALUE_LIMIT_PER_APP = 0.25

REQUIRE_RULES_LIMIT_PER_APP = 1
EXCLUDE_RULES_LIMIT_PER_APP = 1

APP_REPLICAS = N // 10

ENABLE_NODE_AFFINITY = False

NODE_TAGS_VALUE_COUNT = 5
NODE_TAG_TYPES_LIMIT = 5
NODE_TAG_VALUE_LIMIT = 4
NODE_RULE_LIMIT = 2

INVERSE_PERCENT = 10


@dataclass
class RequireRule:
    op: Literal["In", "NotIn"]
    values: List[str]


@dataclass
class ExcludeRule:
    op: Literal["In", "NotIn"]
    values: List[str]


@dataclass
class NodeRule:
    key: str
    op: Literal["In", "NotIn"]
    values: List[str]


def make_yaml(
    name: str,
    requires: Optional[List[RequireRule]],
    excludes: Optional[List[ExcludeRule]],
    node_rules: Optional[List[NodeRule]],
) -> dict[str, Any]:
    podAffinity, podAntiAffinity, nodeAffinity = None, None, None

    if requires:
        podAffinity = {
            "requiredDuringSchedulingIgnoredDuringExecution": [
                {
                    "labelSelector": {
                        "matchExpressions": [
                            {
                                "key": "app",
                                "operator": require.op,
                                "values": require.values,
                            }
                            for require in requires
                        ]
                    },
                    "topologyKey": "kubernetes.io/hostname",
                }
            ]
        }

    if excludes:
        podAntiAffinity = {
            "requiredDuringSchedulingIgnoredDuringExecution": [
                {
                    "labelSelector": {
                        "matchExpressions": [
                            {
                                "key": "app",
                                "operator": exclude.op,
                                "values": exclude.values,
                            }
                            for exclude in excludes
                        ]
                    },
                    "topologyKey": "kubernetes.io/hostname",
                }
            ]
        }

    if node_rules:
        nodeAffinity = {
            "requiredDuringSchedulingIgnoredDuringExecution": {
                "nodeSelectorTerms": [
                    {
                        "matchExpressions": [
                            {
                                "key": node_rule.key,
                                "operator": node_rule.op,
                                "values": node_rule.values,
                            }
                            for node_rule in node_rules
                        ]
                    }
                ]
            }
        }

    spec = {
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {"name": name},
        "spec": {
            "selector": {"matchLabels": {"app": name}},
            "replicas": uniform(1, APP_REPLICAS),
            "template": {
                "metadata": {"name": name, "labels": {"app": name}},
                "spec": {
                    "containers": [
                        {"name": name, "image": "registry.k8s.io/pause:2.0"}
                    ],
                    "tolerations": [
                        {
                            "key": "node.kubernetes.io/not-ready",
                            "operator": "Exists",
                            "effect": "NoSchedule",
                        }
                    ],
                },
            },
        },
    }

    if podAffinity or podAntiAffinity:
        spec["spec"]["template"]["spec"]["affinity"] = {}

    if podAffinity:
        spec["spec"]["template"]["spec"]["affinity"]["podAffinity"] = podAffinity

    if podAntiAffinity:
        spec["spec"]["template"]["spec"]["affinity"][
            "podAntiAffinity"
        ] = podAntiAffinity

    if ENABLE_NODE_AFFINITY and nodeAffinity:
        if "affinity" not in spec["spec"]["template"]["spec"]:
            spec["spec"]["template"]["spec"]["affinity"] = {}

        spec["spec"]["template"]["spec"]["affinity"]["nodeAffinity"] = nodeAffinity

    return spec


def write_yaml(
    name: str,
    requires: List[RequireRule],
    excludes: List[ExcludeRule],
    nodes: List[NodeRule],
):
    with open(os.path.join(target_dir, f"{name}.yaml"), "w") as f:
        yaml.dump(make_yaml(name, requires, excludes, nodes), f)


def uniform(a, b):
    return np.random.randint(a, b)


def possion(a):
    return int(np.random.poisson(a))


def main():
    apps = {
        f"app{i}": {
            "requires": [
                RequireRule(
                    "In" if uniform(0, 100) > INVERSE_PERCENT else "NotIn",
                    list(
                        {
                            f"app{uniform(1, N)}"
                            for _ in range(possion(REQUIRE_VALUE_LIMIT_PER_APP))
                        }
                    ),
                )
                for _ in range(possion(REQUIRE_RULES_LIMIT_PER_APP))
            ],
            "excludes": [
                ExcludeRule(
                    "In" if uniform(0, 100) > INVERSE_PERCENT else "NotIn",
                    list(
                        {
                            f"app{uniform(1, N)}"
                            for _ in range(possion(EXCLUDE_VALUE_LIMIT_PER_APP))
                        }
                    ),
                )
                for _ in range(possion(EXCLUDE_RULES_LIMIT_PER_APP))
            ],
            "node": [
                NodeRule(
                    f"tag{uniform(1, NODE_TAG_TYPES_LIMIT)}",
                    "In" if uniform(0, 100) > 50 else "NotIn",
                    list(
                        {
                            f"val{uniform(1, NODE_TAG_VALUE_LIMIT)}"
                            for _ in range(possion(NODE_TAGS_VALUE_COUNT))
                        }
                    ),
                )
                for _ in range(possion(NODE_RULE_LIMIT))
            ],
        }
        for i in range(1, N)
    }

    # Cleanup empty rules
    for name, app in apps.items():
        app["requires"] = list(filter(lambda x: x.values, app["requires"]))
        if not app["requires"]:
            del app["requires"]

        app["excludes"] = list(filter(lambda x: x.values, app["excludes"]))
        if not app["excludes"]:
            del app["excludes"]

        app["node"] = list(filter(lambda x: x.values, app["node"]))
        if not app["node"]:
            del app["node"]

    if not os.path.exists(target_dir):
        os.mkdir(target_dir)
    else:
        for f in os.listdir(target_dir):
            os.remove(os.path.join(target_dir, f))

    for name, app in apps.items():
        write_yaml(
            name,
            app["requires"] if "requires" in app else None,  # type: ignore
            app["excludes"] if "excludes" in app else None,  # type: ignore
            app["node"] if "node" in app else None,  # type: ignore
        )


if __name__ == "__main__":
    main()
