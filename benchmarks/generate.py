#!/usr/bin/env python3
"""Deterministic benchmark datasets for Arqlet. Standard library only.

Writes graph JSON files into benchmarks/datasets/ (git-ignored) and records their SHA-256
hashes, sizes and sources in benchmarks/manifest.json (tracked). Hand-authored and seeded
datasets are byte-identical across runs. `layerweave-code` reflects the repository state
recorded in the manifest's gitRevision.

Usage: python3 benchmarks/generate.py [--out DIR]
"""
import argparse
import hashlib
import json
import random
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GENERATOR_VERSION = 2


class Builder:
    """Collects nodes and edges. Edge ids are sequential and therefore deterministic."""

    def __init__(self, direction):
        self.default = direction
        self.directions = {}
        self.nodes = []
        self.by_id = {}
        self.edges = []

    def node(self, node_id, label, kind="symbol", parent=None, shape=None):
        assert node_id not in self.by_id, node_id
        entry = {"id": node_id, "kind": kind, "label": label, "parentId": parent}
        if shape:
            # The outline the engine lays out and the viewer draws: "rect", "square" or "circle". Omitted when default.
            entry["shape"] = shape
        self.nodes.append(entry)
        self.by_id[node_id] = entry
        return node_id

    def container(self, node_id, label, parent=None, direction=None, shape=None):
        self.node(node_id, label, "container", parent, shape)
        if direction:
            self.directions[node_id] = direction
        return node_id

    def set_parent(self, node_id, parent):
        self.by_id[node_id]["parentId"] = parent

    def edge(self, source, target, kind="calls"):
        assert source in self.by_id and (target is None or target in self.by_id)
        self.edges.append({"id": f"e{len(self.edges) + 1}", "kind": kind, "source": source, "target": target})

    def graph(self, **options):
        return {
            "defaultDirection": self.default,
            "containerDirections": self.directions,
            "nodes": self.nodes,
            "edges": self.edges,
            **options,
        }


def cloud_architecture():
    b = Builder("lr")
    b.node("users", "Users", "actor")
    b.node("cdn", "CDN", "managed")
    b.node("dns", "DNS", "managed")
    b.container("region", "eu-central-1", None, "lr")
    b.container("vpc", "VPC 10.0.0.0/16", "region", "lr")
    b.node("s3", "Object storage", "storage", "region")
    b.node("queue", "Message queue", "managed", "region")
    b.node("monitoring", "Monitoring", "managed", "region")
    b.node("lb", "Load balancer", "load-balancer", "vpc")
    b.edge("users", "dns", "resolve")
    b.edge("users", "cdn", "https")
    b.edge("cdn", "lb", "origin")
    b.edge("cdn", "s3", "static assets")
    for zone in ["a", "b", "c"]:
        az = b.container(f"az-{zone}", f"Availability zone {zone}", "vpc", "tb")
        public = b.container(f"public-{zone}", "public subnet", az, "lr")
        private = b.container(f"private-{zone}", "private subnet", az, "lr")
        data = b.container(f"data-{zone}", "data subnet", az, "lr")
        b.node(f"web-{zone}", f"web {zone}", "service", public)
        b.node(f"app-{zone}", f"app {zone}", "service", private)
        b.node(f"cache-{zone}", f"cache {zone}", "cache", private)
        b.node(f"db-{zone}", f"db {zone}" + (" primary" if zone == "a" else " replica"), "database", data)
        b.edge("lb", f"web-{zone}", "forward")
        b.edge(f"web-{zone}", f"app-{zone}", "rpc")
        b.edge(f"app-{zone}", f"cache-{zone}", "get/set")
        b.edge(f"app-{zone}", "queue", "publish")
        b.edge(f"web-{zone}", "monitoring", "metrics")
        b.edge(f"app-{zone}", "s3", "upload")
    b.node("worker", "Worker", "service", "region")
    b.edge("queue", "worker", "consume")
    b.edge("worker", "queue", "retry")
    b.edge("worker", "s3", "write results")
    b.edge("app-a", "db-a", "read/write")
    b.edge("app-a", "db-a", "read")
    b.edge("app-b", "db-a", "read")
    b.edge("app-c", "db-a", "read")
    b.edge("db-a", "db-b", "replicate")
    b.edge("db-a", "db-c", "replicate")
    b.edge("worker", "db-a", "write")
    return b.graph()


def activity_order(items=6):
    b = Builder("tb")
    for lane in ["customer", "shop", "warehouse", "payment"]:
        b.container(f"lane:{lane}", lane.capitalize(), None, "lr")
    b.node("start", "Order placed", "start", "lane:customer")
    b.node("check-cart", "Check cart", "activity", "lane:customer")
    b.node("correct", "Correct order", "activity", "lane:customer")
    b.node("receive", "Receive parcel", "activity", "lane:customer")
    b.node("done", "Done", "end", "lane:customer")
    b.node("validate", "Valid order?", "decision", "lane:shop")
    b.node("reserve", "Reserve stock", "activity", "lane:shop")
    b.node("split", "Split items", "fork", "lane:shop")
    b.node("join", "Items ready", "join", "lane:shop")
    b.node("invoice", "Create invoice", "activity", "lane:shop")
    b.node("notify", "Notify customer", "activity", "lane:shop")
    b.node("pack", "Pack parcel", "activity", "lane:warehouse")
    b.node("ship", "Ship parcel", "activity", "lane:warehouse")
    b.node("charge", "Charge card", "activity", "lane:payment")
    b.node("approved", "Payment approved?", "decision", "lane:payment")
    b.edge("start", "check-cart", "next")
    b.edge("check-cart", "validate", "submit")
    b.edge("validate", "reserve", "yes")
    b.edge("validate", "notify", "no")
    b.edge("notify", "correct", "notified")
    b.edge("correct", "validate", "resubmit")
    b.edge("reserve", "split", "reserved")
    b.edge("reserve", "charge", "reserved")
    b.edge("charge", "approved", "charged")
    b.edge("approved", "invoice", "yes")
    b.edge("approved", "notify", "no")
    b.edge("invoice", "join", "invoiced")
    for i in range(items):
        pick, check = f"pick-{i}", f"check-{i}"
        b.node(pick, f"Pick item {i}", "activity", "lane:warehouse")
        b.node(check, "Quality ok?", "decision", "lane:warehouse")
        b.edge("split", pick, f"item {i}")
        b.edge(pick, check, "picked")
        b.edge(check, pick, "fail")
        b.edge(check, "join", "pass")
    b.edge("join", "pack", "ready")
    b.edge("pack", "ship", "packed")
    b.edge("ship", "receive", "shipped")
    b.edge("receive", "done", "delivered")
    return b.graph()


def deployment_cluster():
    b = Builder("tb")
    b.node("internet", "Internet", "external")
    b.node("dns", "Managed DNS", "external")
    b.node("registry", "Container registry", "external")
    b.node("object-store", "Object store", "external")
    b.container("cluster", "prod-eu cluster", None, "lr")
    b.container("ns-ingress", "namespace ingress", "cluster", "lr")
    b.container("ns-app", "namespace app", "cluster", "lr")
    b.container("ns-data", "namespace data", "cluster", "lr")
    b.container("ns-obs", "namespace observability", "cluster", "lr")
    b.node("ingress", "ingress-controller", "controller", "ns-ingress")
    b.node("svc-web", "svc web", "service", "ns-app")
    b.node("svc-api", "svc api", "service", "ns-app")
    b.node("svc-db", "svc postgres", "service", "ns-data")
    b.node("redis", "redis", "pod", "ns-data")
    b.node("object-proxy", "object proxy", "service", "ns-app")
    b.node("prometheus", "prometheus", "service", "ns-obs")
    b.node("grafana", "grafana", "service", "ns-obs")
    b.node("alertmanager", "alertmanager", "service", "ns-obs")
    pods = {"web": 3, "api": 3, "worker": 2}
    for name, count in pods.items():
        deployment = b.container(f"deploy-{name}", f"deployment {name}", "ns-app", "lr")
        for i in range(count):
            b.node(f"{name}-{i}", f"{name}-{i}", "pod", deployment)
    b.container("sts-postgres", "statefulset postgres", "ns-data", "lr")
    for i in range(3):
        b.node(f"pg-{i}", f"postgres-{i}", "pod", "sts-postgres")
    b.edge("internet", "dns", "resolve")
    b.edge("dns", "ingress", "resolves to")
    b.edge("ingress", "svc-web", "route /")
    b.edge("ingress", "svc-api", "route /api")
    b.edge("svc-api", "object-proxy", "route /files")
    b.edge("svc-web", "svc-api", "api call")
    b.edge("svc-db", "pg-0", "primary")
    b.edge("svc-db", "pg-1", "replica")
    b.edge("svc-db", "pg-2", "replica")
    b.edge("pg-0", "pg-1", "replicate")
    b.edge("pg-0", "pg-2", "replicate")
    for i in range(pods["web"]):
        b.edge("svc-web", f"web-{i}", "balance")
        b.edge(f"web-{i}", "svc-api", "calls")
    for i in range(pods["api"]):
        b.edge("svc-api", f"api-{i}", "balance")
        b.edge(f"api-{i}", "svc-db", "queries")
        b.edge(f"api-{i}", "redis", "cache")
        b.edge(f"api-{i}", "redis", "enqueue")
    for i in range(pods["worker"]):
        b.edge("redis", f"worker-{i}", "dequeue")
        b.edge(f"worker-{i}", "svc-api", "callback")
        b.edge(f"worker-{i}", "object-store", "upload")
    for pod in [f"{n}-{i}" for n, c in pods.items() for i in range(c)] + [f"pg-{i}" for i in range(3)]:
        b.edge("prometheus", pod, "scrape")
    b.edge("grafana", "prometheus", "query")
    b.edge("prometheus", "alertmanager", "alert")
    for name, count in pods.items():
        for i in range(count):
            b.edge("registry", f"{name}-{i}", "pull image")
    return b.graph()


def state_order():
    b = Builder("lr")
    b.node("initial", "", "initial")
    b.node("final", "", "final")
    b.node("created", "Created", "state")
    b.node("paid", "Paid", "state")
    b.node("cancelled", "Cancelled", "state")
    b.node("refunded", "Refunded", "state")
    b.node("completed", "Completed", "state")
    b.container("active", "Active", None, "tb")
    b.node("picking", "Picking", "state", "active")
    b.node("packing", "Packing", "state", "active")
    b.node("shipping", "Shipping", "state", "active")
    b.edge("initial", "created", "place")
    b.edge("created", "paid", "pay")
    b.edge("created", "cancelled", "timeout")
    b.edge("paid", "active", "start fulfilment")
    b.edge("picking", "packing", "picked")
    b.edge("packing", "picking", "defect")
    b.edge("picking", "picking", "retry")
    b.edge("packing", "shipping", "packed")
    b.edge("shipping", "completed", "delivered")
    b.edge("active", "cancelled", "user cancel")
    b.edge("active", "cancelled", "admin cancel")
    b.edge("cancelled", "refunded", "refund")
    b.edge("refunded", "final", "settled")
    b.edge("completed", "final", "close")
    return b.graph()


def state_tcp():
    b = Builder("lr")
    states = ["CLOSED", "LISTEN", "SYN_SENT", "SYN_RCVD", "ESTABLISHED", "FIN_WAIT_1",
              "FIN_WAIT_2", "CLOSE_WAIT", "CLOSING", "LAST_ACK", "TIME_WAIT"]
    for state in states:
        b.node(state, state, "state")
    transitions = [
        ("CLOSED", "LISTEN", "passive open"), ("CLOSED", "SYN_SENT", "active open"),
        ("LISTEN", "SYN_RCVD", "recv SYN"), ("LISTEN", "SYN_SENT", "send SYN"),
        ("LISTEN", "CLOSED", "close"), ("SYN_SENT", "SYN_RCVD", "recv SYN"),
        ("SYN_SENT", "ESTABLISHED", "recv SYN+ACK"), ("SYN_SENT", "CLOSED", "close"),
        ("SYN_RCVD", "ESTABLISHED", "recv ACK"), ("SYN_RCVD", "FIN_WAIT_1", "close"),
        ("ESTABLISHED", "FIN_WAIT_1", "close"), ("ESTABLISHED", "CLOSE_WAIT", "recv FIN"),
        ("FIN_WAIT_1", "FIN_WAIT_2", "recv ACK"), ("FIN_WAIT_1", "CLOSING", "recv FIN"),
        ("FIN_WAIT_2", "TIME_WAIT", "recv FIN"), ("CLOSING", "TIME_WAIT", "recv ACK"),
        ("CLOSE_WAIT", "LAST_ACK", "close"), ("LAST_ACK", "CLOSED", "recv ACK"),
        ("TIME_WAIT", "CLOSED", "timeout"),
    ]
    for source, target, event in transitions:
        b.edge(source, target, event)
    return b.graph()


def workflow_dag(seed=7, stages=8):
    rnd = random.Random(seed)
    b = Builder("lr")
    kinds = {}
    layers = []
    for s in range(stages):
        stage = b.container(f"stage:{s}", f"Stage {s}", None, "tb")
        tasks = []
        for t in range(rnd.randint(6, 12)):
            kind = rnd.choice(["extract", "transform", "load", "check"])
            node_id = b.node(f"task:{s}:{t}", f"{kind} {s}.{t}", kind, stage)
            kinds[node_id] = kind
            tasks.append(node_id)
        layers.append(tasks)
    for s in range(1, stages):
        for task in layers[s]:
            previous = layers[s - 1]
            for source in rnd.sample(previous, rnd.randint(1, min(3, len(previous)))):
                b.edge(source, task, "data")
            if s >= 3 and rnd.random() < 0.15:
                b.edge(rnd.choice(layers[s - 3]), task, "skip")
        for check in [t for t in layers[s] if kinds[t] == "check"]:
            if rnd.random() < 0.5:
                transforms = [t for t in layers[s - 1] if kinds[t] == "transform"] or layers[s - 1]
                b.edge(check, rnd.choice(transforms), "retry")
    return b.graph()


def arqlet_pipeline():
    b = Builder("tb")
    crates = ["model", "view", "layout", "routing", "scene", "runtime"]
    for crate in crates:
        b.container(f"crate:{crate}", f"arqlet-{crate}", None, "lr")
    b.node("validate", "validate()", "function", "crate:model")
    b.node("snapshot", "Snapshot schema", "type", "crate:model")
    b.node("project", "project()", "function", "crate:view")
    b.node("select", "bounded view selection", "function", "crate:view")
    b.node("rank", "rank assignment", "phase", "crate:layout")
    b.node("order", "ordering sweeps", "phase", "crate:layout")
    b.node("coords", "coordinate assignment", "phase", "crate:layout")
    b.node("containers", "container placement", "phase", "crate:layout")
    b.node("ports", "port resolution", "phase", "crate:layout")
    b.node("route", "initial routes", "phase", "crate:routing")
    b.node("improve", "lane search improve()", "phase", "crate:routing")
    b.node("quality", "quality()", "function", "crate:layout")
    b.node("better", "better() comparison", "function", "crate:layout")
    b.node("geometry", "validate_geometry()", "function", "crate:scene")
    b.node("dispatch", "dispatch()", "function", "crate:runtime")
    b.node("import", "import_graph()", "function", "crate:runtime")
    b.node("add", "add_node()", "function", "crate:runtime")
    b.node("toggle", "toggle_container()", "function", "crate:runtime")
    b.node("direction", "set_direction()", "function", "crate:runtime")
    b.node("refine", "refine_snapshot()", "function", "crate:runtime")
    b.node("candidates", "four ordering candidates", "phase", "crate:runtime")
    b.node("reroute", "routing-only candidate", "phase", "crate:runtime")
    b.node("select-best", "select best valid", "decision", "crate:runtime")
    b.node("revision", "revision check", "decision", "crate:runtime")
    b.node("stale", "discard stale result", "end", "crate:runtime")
    b.node("result", "accepted snapshot", "end", "crate:runtime")
    b.edge("dispatch", "import", "op import")
    b.edge("dispatch", "add", "op add")
    b.edge("dispatch", "toggle", "op toggle")
    b.edge("dispatch", "direction", "op direction")
    b.edge("dispatch", "refine", "op refine")
    b.edge("import", "select", "BFS")
    b.edge("select", "project", "projection")
    b.edge("project", "validate", "validate")
    b.edge("validate", "rank", "view graph")
    b.edge("rank", "order", "layers")
    b.edge("order", "coords", "positions")
    b.edge("coords", "containers", "bounded placement")
    b.edge("containers", "ports", "boxes")
    b.edge("ports", "route", "endpoints")
    b.edge("route", "improve", "routes")
    b.edge("improve", "geometry", "check")
    b.edge("geometry", "result", "valid")
    b.edge("add", "revision", "expected revision")
    b.edge("revision", "stale", "mismatch")
    b.edge("revision", "project", "match")
    b.edge("toggle", "project", "view state")
    b.edge("direction", "project", "source graph")
    b.edge("refine", "candidates", "fan out")
    b.edge("refine", "reroute", "fan out")
    b.edge("candidates", "quality", "measure")
    b.edge("reroute", "quality", "measure")
    b.edge("quality", "better", "compare")
    b.edge("better", "select-best", "keep best")
    b.edge("select-best", "candidates", "next candidate")
    b.edge("select-best", "revision", "best accepted")
    b.edge("revision", "result", "match")
    b.edge("refine", "snapshot", "input")
    b.edge("result", "revision", "later edit")
    return b.graph()


def layerweave_code(root=ROOT):
    """Crates, modules and functions of this repository, with heuristic call edges.

    A call is recorded when a function body contains `name(` for a function defined in this
    repository: same file first, otherwise a unique definition. Ambiguous names are skipped.
    Functions in `#[cfg(test)]` sections are excluded.
    """
    b = Builder("lr")
    crates = {p.parent.name: p.parent for p in sorted((root / "crates").glob("*/Cargo.toml"))}
    for name in crates:
        b.container(f"crate:{name}", f"arqlet-{name}", None, "lr")
    for name, directory in crates.items():
        text = (directory / "Cargo.toml").read_text()
        for match in re.finditer(r"^(arqlet-\w+)\s*=\s*\{\s*path", text, re.M):
            target = match.group(1).removeprefix("arqlet-")
            if target in crates:
                b.edge(f"crate:{name}", f"crate:{target}", "depends")
    functions = []
    definitions = {}
    for name, directory in crates.items():
        for path in sorted((directory / "src").rglob("*.rs")):
            rel = path.relative_to(root).as_posix()
            text = path.read_text()
            cut = text.find("#[cfg(test)]")
            code = text if cut < 0 else text[:cut]
            module = b.container(f"mod:{rel}", rel.split("/src/", 1)[1] if "/src/" in rel else rel,
                                 f"crate:{name}", "lr")
            for match in re.finditer(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?fn\s+(\w+)", code, re.M):
                fname = match.group(1)
                node_id = f"fn:{rel}::{fname}"
                suffix = 2
                while node_id in b.by_id:
                    node_id = f"fn:{rel}::{fname}#{suffix}"
                    suffix += 1
                b.node(node_id, fname, "function", module)
                start = code.find("{", match.end())
                semi = code.find(";", match.end())
                body = ""
                if start >= 0 and (semi < 0 or start < semi):
                    depth, end = 0, start
                    while end < len(code):
                        if code[end] == "{":
                            depth += 1
                        elif code[end] == "}":
                            depth -= 1
                            if depth == 0:
                                break
                        end += 1
                    body = code[start:end]
                functions.append((node_id, rel, fname, body))
                definitions.setdefault(fname, []).append((node_id, rel))
    for source, rel, fname, body in functions:
        seen = set()
        for called in re.findall(r"\b(\w+)\s*\(", body):
            if called in seen or called not in definitions:
                continue
            seen.add(called)
            candidates = definitions[called]
            local = [c for c in candidates if c[1] == rel]
            if local:
                target = local[0][0]
            elif len(candidates) == 1:
                target = candidates[0][0]
            else:
                continue
            b.edge(source, target, "calls")
    return b.graph()


def synthetic(n, seed):
    """Random hierarchy with locality, cycles, self-loops, parallel and unresolved edges."""
    rnd = random.Random(seed)
    b = Builder("hybrid")
    items = [b.node(f"n{i}", f"n{i}", "symbol") for i in range(n)]
    level, counter = 0, 0
    while len(items) > 8 and level < 3:
        grouped, i = [], 0
        while i < len(items):
            size = rnd.randint(4, 12)
            chunk = items[i:i + size]
            i += size
            cid = b.container(f"g{level}-{counter}", f"group {level}-{counter}", None, rnd.choice(["lr", "tb"]))
            counter += 1
            for member in chunk:
                b.set_parent(member, cid)
            grouped.append(cid)
        items, level = grouped, level + 1
    for _ in range(3 * n):
        s = rnd.randrange(n)
        roll = rnd.random()
        if roll < 0.03:
            t = s
        elif roll < 0.85:
            t = min(n - 1, max(0, s + rnd.randint(-20, 20)))
        else:
            t = rnd.randrange(n)
        kind = rnd.choice(["calls", "reads", "writes"])
        if rnd.random() < 0.05:
            b.edge(f"n{s}", None, kind)
            continue
        b.edge(f"n{s}", f"n{t}", kind)
        if rnd.random() < 0.02:
            b.edge(f"n{s}", f"n{t}", kind)
    return b.graph()


def organic_ladder(n):
    """Connected organic graph with deterministic sparse chords for stress/repair scaling."""
    seed = 20261027 + n
    rnd = random.Random(seed)
    b = Builder("auto")
    for i in range(n):
        b.node(f"o{i}", f"node {i}", "symbol")
    for i in range(n - 1):
        b.edge(f"o{i}", f"o{i + 1}")
    for i in range(0, n - 2, 5):
        distance = rnd.randint(2, min(9, n - 1 - i))
        b.edge(f"o{i}", f"o{i + distance}")
    return b.graph(defaultStrategy="organic", defaultRouting="straight")


def tree_ladder(n):
    """Balanced binary tree for measuring tree placement without route-search noise."""
    b = Builder("tb")
    for i in range(n):
        b.node(f"t{i}", f"node {i}", "symbol")
    for i in range(1, n):
        b.edge(f"t{(i - 1) // 2}", f"t{i}")
    return b.graph(defaultStrategy="tree", defaultRouting="straight")


def nested_tree_radial():
    # The committed benchmark input, copied as it is: five nested container levels, tree and forest areas with
    # alternating tree and radial strategies (benchmarks/inputs/nested-tree-radial.json).
    return json.loads((ROOT / "benchmarks" / "inputs" / "nested-tree-radial.json").read_text(encoding="utf-8"))


def org_chart(seed=5):
    rnd = random.Random(seed)
    b = Builder("tb")
    ceo = b.node("ceo", "CEO", "person")
    for d, division in enumerate(["Product", "Engineering", "Operations", "Finance"]):
        div = b.container(f"div-{d}", division, None, "lr")
        head = b.node(f"head-{d}", f"{division} head", "person", div)
        b.edge(head, ceo, "reports to")
        for k in range(rnd.randint(2, 3)):
            dep = b.container(f"dep-{d}-{k}", f"{division} department {k}", div, "tb")
            lead = b.node(f"lead-{d}-{k}", f"Lead {d}.{k}", "person", dep)
            b.edge(lead, head, "reports to")
            for t in range(rnd.randint(1, 3)):
                team = b.container(f"team-{d}-{k}-{t}", f"team {d}.{k}.{t}", dep, "lr")
                for p in range(rnd.randint(2, 4)):
                    person = b.node(f"p-{d}-{k}-{t}-{p}", f"Person {d}.{k}.{t}.{p}", "person", team)
                    b.edge(person, lead, "reports to")
    people = [n["id"] for n in b.nodes if n["id"].startswith("p-")]
    for _ in range(12):
        a, c = rnd.sample(people, 2)
        b.edge(a, c, "dotted line")
    return b.graph()


def uml_class():
    b = Builder("tb")
    for package in ["domain", "persistence", "api"]:
        b.container(f"pkg:{package}", f"package {package}", None, "lr")
    classes = [
        ("domain", "Order"), ("domain", "OrderLine"), ("domain", "Customer"), ("domain", "Address"),
        ("domain", "Product"), ("domain", "Payment"), ("domain", "CreditCard"), ("domain", "BankTransfer"),
        ("persistence", "OrderRepository"), ("persistence", "CustomerRepository"),
        ("persistence", "ProductRepository"), ("persistence", "Database"),
        ("api", "OrderController"), ("api", "CustomerController"), ("api", "Router"),
        ("api", "Serializer"), ("api", "Logger"),
    ]
    attributes = {
        "Order": ["id", "status", "total"], "OrderLine": ["quantity", "price"],
        "Customer": ["name", "email"], "Address": ["street", "city"], "Product": ["sku", "price"],
        "Payment": ["amount"],
    }
    for package, name in classes:
        cls = b.container(f"cls:{name}", name, f"pkg:{package}", "lr")
        for attribute in attributes.get(name, ["id"]):
            b.node(f"attr:{name}.{attribute}", attribute, "attribute", cls)
    relations = [
        ("CreditCard", "Payment", "extends"), ("BankTransfer", "Payment", "extends"),
        ("OrderRepository", "Database", "uses"), ("CustomerRepository", "Database", "uses"),
        ("ProductRepository", "Database", "uses"), ("OrderController", "OrderRepository", "uses"),
        ("CustomerController", "CustomerRepository", "uses"), ("Router", "OrderController", "composes"),
        ("Router", "CustomerController", "composes"), ("OrderController", "Serializer", "uses"),
        ("CustomerController", "Serializer", "uses"), ("Order", "OrderLine", "composes"),
        ("Order", "Customer", "associates"), ("Order", "Customer", "placed by"),
        ("Order", "Payment", "associates"), ("OrderLine", "Product", "associates"),
        ("Customer", "Address", "composes"), ("Product", "ProductRepository", "uses"),
        ("OrderRepository", "Order", "uses"), ("Order", "Order", "uses"),
        ("Customer", "Order", "associates"), ("Serializer", "Logger", "uses"),
        ("Database", "Logger", "uses"), ("OrderController", "Logger", "uses"),
    ]
    for source, target, kind in relations:
        b.edge(f"cls:{source}", f"cls:{target}", kind)
    return b.graph()


def er_diagram():
    # The entity-relationship layout is free (auto): entities take no fixed direction, so a stress (organic) arrangement
    # can place and move them.
    b = Builder("auto")
    entities = {
        "Customer": ["id", "name", "email"], "Order": ["id", "placed_at", "status"],
        "OrderItem": ["quantity", "unit_price"], "Product": ["id", "sku", "title"],
        "Category": ["id", "name"], "Supplier": ["id", "name", "country"],
        "Warehouse": ["id", "city"], "Stock": ["quantity"], "Invoice": ["id", "amount", "due"],
        "Payment": ["id", "method", "paid_at"], "Shipment": ["id", "carrier", "shipped_at"],
        "Address": ["id", "street", "city", "zip"],
    }
    for name, attributes in entities.items():
        entity = b.node(f"ent:{name}", name, "entity")
        for attribute in attributes:
            b.node(f"attr:{name}.{attribute}", attribute, "attribute", entity)
    relationships = [
        ("Customer", "places", "Order", "1", "N"), ("Order", "contains", "OrderItem", "1", "N"),
        ("Product", "appears in", "OrderItem", "1", "N"), ("Product", "belongs to", "Category", "N", "1"),
        ("Product", "supplied by", "Supplier", "N", "1"), ("Warehouse", "holds", "Stock", "1", "N"),
        ("Product", "stocked as", "Stock", "1", "N"), ("Order", "billed by", "Invoice", "1", "1"),
        ("Invoice", "settled by", "Payment", "1", "N"), ("Order", "shipped via", "Shipment", "1", "N"),
        ("Customer", "lives at", "Address", "N", "M"), ("Category", "parent of", "Category", "1", "N"),
    ]
    # A relationship is an edge between its entities; its kind is the relationship's type and cardinality.
    for source, verb, target, source_card, target_card in relationships:
        b.edge(f"ent:{source}", f"ent:{target}", f"{verb} ({source_card}:{target_card})")
    return b.graph()


def bpmn_pools():
    b = Builder("lr")
    b.container("pool:customer", "Customer", None, "tb")
    b.container("pool:shop", "Online shop", None, "lr")
    b.container("lane:sales", "Sales", "pool:shop", "tb")
    b.container("lane:fulfilment", "Fulfilment", "pool:shop", "tb")
    b.container("pool:logistics", "Logistics provider", None, "tb")
    b.node("c-start", "Order needed", "start", "pool:customer")
    b.node("c-place", "Place order", "task", "pool:customer")
    b.node("c-wait", "Wait for payment", "task", "pool:customer")
    b.node("c-receive", "Receive goods", "task", "pool:customer")
    b.node("c-end", "Goods received", "end", "pool:customer")
    b.node("s-msg", "Order received", "message-start", "lane:sales")
    b.node("s-validate", "Validate order", "task", "lane:sales")
    b.node("s-stock", "In stock?", "gateway", "lane:sales")
    b.node("s-reserve", "Reserve items", "task", "lane:sales")
    b.node("s-request", "Request payment", "task", "lane:sales")
    b.node("s-timeout", "Payment timeout", "timer", "lane:sales")
    b.node("s-split", "Split", "parallel-gateway", "lane:sales")
    b.node("s-join", "Join", "parallel-gateway", "lane:sales")
    b.node("s-paid", "Payment ok?", "gateway", "lane:sales")
    b.node("s-cancel", "Order cancelled", "end", "lane:sales")
    b.node("f-pick", "Pick and pack", "task", "lane:fulfilment")
    b.node("f-handover", "Hand over", "task", "lane:fulfilment")
    b.node("f-notify", "Notify customer", "task", "lane:fulfilment")
    b.node("l-start", "Shipment request", "message-start", "pool:logistics")
    b.node("l-deliver", "Deliver", "task", "pool:logistics")
    b.node("l-end", "Delivered", "message-end", "pool:logistics")
    b.edge("c-start", "c-place", "sequence")
    b.edge("c-place", "s-msg", "message: order")
    b.edge("s-msg", "s-validate", "sequence")
    b.edge("s-validate", "s-stock", "sequence")
    b.edge("s-stock", "s-cancel", "no")
    b.edge("s-stock", "s-reserve", "yes")
    b.edge("s-reserve", "s-split", "sequence")
    b.edge("s-split", "s-request", "sequence")
    b.edge("s-split", "f-pick", "sequence")
    b.edge("s-request", "s-paid", "sequence")
    b.edge("s-request", "s-timeout", "boundary")
    b.edge("s-timeout", "s-cancel", "sequence")
    b.edge("s-paid", "s-request", "retry")
    b.edge("s-paid", "s-join", "yes")
    b.edge("f-pick", "s-join", "sequence")
    b.edge("s-join", "f-handover", "sequence")
    b.edge("f-handover", "l-start", "message: shipment")
    b.edge("l-start", "l-deliver", "sequence")
    b.edge("l-deliver", "l-end", "sequence")
    b.edge("l-end", "f-notify", "message: delivered")
    b.edge("f-notify", "c-wait", "message: notification")
    b.edge("c-wait", "c-receive", "sequence")
    b.edge("c-receive", "c-end", "sequence")
    b.edge("s-cancel", "c-wait", "message: cancelled")
    return b.graph()


def network_vlan(seed=9):
    rnd = random.Random(seed)
    b = Builder("tb")
    b.node("internet", "Internet", "external")
    b.node("isp-b", "Backup ISP", "external")
    b.container("dc", "Data center", None, "tb")
    b.node("fw-a", "Firewall A", "firewall", "dc")
    b.node("fw-b", "Firewall B", "firewall", "dc")
    b.node("core-a", "Core switch A", "switch", "dc")
    b.node("core-b", "Core switch B", "switch", "dc")
    b.edge("internet", "fw-a", "wan")
    b.edge("isp-b", "fw-b", "wan backup")
    b.edge("fw-a", "core-a", "trunk")
    b.edge("fw-b", "core-b", "trunk")
    b.edge("core-a", "core-b", "peer link")
    b.edge("core-a", "core-b", "peer link 2")
    vlans = [(10, "users"), (20, "servers"), (30, "iot"), (99, "management")]
    host_counts = {10: rnd.randint(5, 8), 20: rnd.randint(4, 6), 30: rnd.randint(4, 7), 99: 2}
    for vlan, name in vlans:
        group = b.container(f"vlan{vlan}", f"VLAN {vlan} {name}", "dc", "lr")
        switch = b.node(f"sw{vlan}", f"access switch {vlan}", "switch", group)
        b.edge("core-a", switch, f"vlan {vlan}")
        b.edge("core-b", switch, f"vlan {vlan} standby")
        for h in range(host_counts[vlan]):
            host = b.node(f"h{vlan}-{h}", f"{name}-{h}", "host", group)
            b.edge(switch, host, "access")
        b.edge("fw-a", switch, "deny" if vlan == 30 else "allow")
        b.edge("fw-b", switch, "deny" if vlan == 30 else "allow")
    b.edge("sw99", "fw-a", "manage")
    b.edge("sw30", "internet", "blocked")
    b.edge("sw10", "sw20", "backup route")
    return b.graph()


def petri_net():
    b = Builder("lr")
    for name in ["lock", "buffer", "slots", "maintenance"]:
        b.node(name, name, "place")
    for i in range(3):
        b.node(f"p{i}-idle", f"producer {i} idle", "place")
        b.node(f"p{i}-start", f"start produce {i}", "transition")
        b.node(f"p{i}-busy", f"producer {i} busy", "place")
        b.node(f"p{i}-done", f"finish produce {i}", "transition")
        b.edge(f"p{i}-idle", f"p{i}-start", "arc")
        b.edge("lock", f"p{i}-start", "arc")
        b.edge("slots", f"p{i}-start", "arc")
        b.edge(f"p{i}-start", f"p{i}-busy", "arc")
        b.edge(f"p{i}-busy", f"p{i}-done", "arc")
        b.edge(f"p{i}-done", f"p{i}-idle", "arc")
        b.edge(f"p{i}-done", "buffer", "arc")
        b.edge(f"p{i}-done", "lock", "arc")
    b.edge("maintenance", "p0-start", "inhibit")
    for j in range(2):
        b.node(f"c{j}-idle", f"consumer {j} idle", "place")
        b.node(f"c{j}-take", f"take item {j}", "transition")
        b.node(f"c{j}-busy", f"consumer {j} busy", "place")
        b.node(f"c{j}-finish", f"finish consume {j}", "transition")
        b.edge("buffer", f"c{j}-take", "arc")
        b.edge(f"c{j}-idle", f"c{j}-take", "arc")
        b.edge("lock", f"c{j}-take", "arc")
        b.edge(f"c{j}-take", f"c{j}-busy", "arc")
        b.edge(f"c{j}-busy", f"c{j}-finish", "arc")
        b.edge(f"c{j}-finish", f"c{j}-idle", "arc")
        b.edge(f"c{j}-finish", "slots", "arc")
        b.edge(f"c{j}-finish", "lock", "arc")
    return b.graph()


def petri_net_shapes():
    """The Petri net with its notation: places are circles, transitions are rectangles. An organic area with straight edges
    shows the shapes, the clearance and the repair at once; the graph is small, so its first layout is quick."""
    graph = petri_net()
    for node in graph["nodes"]:
        if node["kind"] == "place":
            node["shape"] = "circle"
    graph["defaultDirection"] = "auto"
    graph["defaultStrategy"] = "organic"
    graph["defaultRouting"] = "straight"
    return graph


def mixed_layouts():
    """Areas with different layouts in one drawing: the root and the circle container are organic, one container is layered
    left to right and another top to bottom. Round and rectangular nodes, straight edges across the borders."""
    b = Builder("auto")
    b.node("users", "users", "actor", None, "circle")
    b.node("gateway", "gateway", "service")
    b.node("db", "database", "store", None, "circle")
    b.container("mesh", "service mesh", None, "auto", "circle")
    for name, shape in [("auth", "circle"), ("orders", None), ("billing", None), ("search", "circle"), ("cache", None)]:
        b.node(name, name, "service", "mesh", shape)
    b.container("pipeline", "build pipeline", None, "lr")
    for name in ["checkout", "compile", "test", "package"]:
        b.node(name, name, "step", "pipeline")
    b.container("ops", "operations", None, "tb")
    for name, shape in [("alerts", "circle"), ("pager", None), ("runbook", None)]:
        b.node(name, name, "tool", "ops", shape)
    for source, target in [("users", "gateway"), ("gateway", "auth"), ("gateway", "orders"), ("orders", "billing"),
                           ("orders", "cache"), ("search", "cache"), ("billing", "db"), ("orders", "db"),
                           ("checkout", "compile"), ("compile", "test"), ("test", "package"), ("package", "gateway"),
                           ("alerts", "pager"), ("pager", "runbook"), ("alerts", "orders"), ("auth", "search")]:
        b.edge(source, target, "calls")
    return b.graph(
        defaultStrategy="organic",
        defaultRouting="straight",
        containerStrategies={"pipeline": "none", "ops": "none"},
    )


def mixed_large(seed=21):
    """A large nested model with different layouts in one drawing: an organic root, a circle container with an organic area, layered
    containers (left to right, top to bottom) with nested sub-containers, round and rectangular nodes, straight and orthogonal
    edges. About 100 nodes; it shows the incremental paths and the conflict marking at a size where a full layout takes seconds."""
    rng = random.Random(seed)
    b = Builder("auto")
    leaves = []
    plans = [("mesh", "auto", "circle", 12), ("pipeline", "lr", None, 14), ("ops", "tb", None, 12), ("data", "lr", None, 14), ("edge", "auto", None, 10)]
    for name, direction, shape, count in plans:
        b.container(name, name, None, direction, shape)
        groups = [name] + [f"{name}-{i}" for i in range(1, 4)]
        for group in groups[1:]:
            b.container(group, group, name, "tb" if direction != "tb" else "lr")
        for i in range(count):
            parent = groups[1 + i % 3] if i % 4 else name
            leaf = b.node(f"{name}-n{i}", f"{name} {i}", "service", parent, "circle" if i % 5 == 0 else None)
            leaves.append(leaf)
    for i in range(14):
        leaf = b.node(f"loose-{i}", f"loose {i}", "actor", None, "circle" if i % 3 == 0 else None)
        leaves.append(leaf)
    for i, source in enumerate(leaves):
        # Mostly one edge per node, a second one for every fourth: a sparse model reads as a diagram.
        for _ in range(2 if i % 4 == 0 else 1):
            target = rng.choice(leaves)
            if target != source:
                b.edge(source, target, "calls")
    return b.graph(
        defaultStrategy="organic",
        defaultRouting="straight",
        containerStrategies={"pipeline": "none", "ops": "none", "data": "none"},
    )


def self_loops():
    """Nodes with no, one, two and three self-loops, parallel and antiparallel edges, in an organic root and in a layered container,
    round and rectangular. Self-loops are never drawn straight; they take the loop rule (ADR 0014)."""
    b = Builder("auto")
    for i, (name, shape) in enumerate([("idle", None), ("busy", "circle"), ("retry", None), ("wait", "circle"), ("done", None)]):
        b.node(name, name, "state", None, shape)
    b.container("machine", "state machine", None, "lr")
    for name in ["init", "run", "fail"]:
        b.node(f"m-{name}", name, "state", "machine")
    for source, target in [("idle", "busy"), ("busy", "idle"), ("busy", "retry"), ("retry", "busy"), ("retry", "wait"), ("wait", "done"),
                           ("idle", "idle"), ("busy", "busy"), ("busy", "busy"), ("retry", "retry"), ("retry", "retry"), ("retry", "retry"),
                           ("wait", "wait"), ("m-init", "m-run"), ("m-run", "m-fail"), ("m-fail", "m-run"), ("m-run", "m-run"),
                           ("m-fail", "m-fail"), ("done", "m-init"), ("idle", "idle")]:
        b.edge(source, target, "transition")
    return b.graph(defaultStrategy="organic", defaultRouting="straight", containerStrategies={"machine": "none"})


def fixed_ports():
    """Edges that leave their node at a placed port: a hub with four ports on different sides and a chain whose ends are fixed. The
    refinement must keep every port where it was placed (explicit_constraints.rs)."""
    b = Builder("lr")
    b.node("hub", "hub", "service")
    for name in ["a", "b", "c", "d"]:
        b.node(name, name, "service")
        b.edge("hub", name, "calls")
    for source, target in [("a", "b"), ("c", "d"), ("b", "d")]:
        b.edge(source, target, "calls")
    graph = b.graph()
    sides = [("hub-east", "right", 0.25), ("hub-north", "top", 0.5), ("hub-south", "bottom", 0.5), ("hub-west", "left", 0.75)]
    graph["ports"] = [{"id": pid, "nodeId": "hub", "side": side, "offset": offset} for pid, side, offset in sides]
    graph["edgePorts"] = [{"edgeId": f"e{i + 1}", "sourcePort": pid, "targetPort": None} for i, (pid, _, _) in enumerate(sides)]
    return graph


def can_bus():
    """Vehicle speed limiter on a CAN bus: the ECUs that publish speed, engine and position data, the ECUs that receive them,
    and the limiter's wired outputs. The CAN messages (named by their kind) form one explicit bus between the sending
    software and the receiving software (a backbone), the limiter's outputs a second one. Inside, the limiter ECU holds its software as a layered diagram
    (left to right), the engine ECU its software modules as an organic network, and the instrument cluster and the telematics
    logger share one housing (physical packaging)."""
    b = Builder("tb")
    for node_id, label in [("abs", "ABS ECU (wheel speed)"), ("gps", "GPS module"), ("switch", "Limiter switch")]:
        b.node(node_id, label, "ecu")
    # The engine ECU: its software modules as an organic network.
    b.container("engine", "Engine ECU")
    for node_id, label in [("eng-can", "CAN stack"), ("eng-rpm", "RPM sensing"), ("eng-torque", "Torque model"),
                           ("eng-inj", "Injection control"), ("eng-diag", "OBD diagnostics")]:
        b.node(node_id, label, "software", "engine")
    for source, target in [("eng-rpm", "eng-torque"), ("eng-torque", "eng-inj"), ("eng-inj", "eng-rpm"), ("eng-torque", "eng-can"),
                           ("eng-diag", "eng-can"), ("eng-diag", "eng-inj")]:
        b.edge(source, target, "signal")
    # The limiter ECU: its software as a layered diagram from the CAN input to the PWM output.
    b.container("limiter", "Speed limiter ECU", direction="lr")
    for node_id, label in [("lim-can", "CAN driver"), ("lim-speed", "Speed estimator"), ("lim-ctrl", "Limiter controller"),
                           ("lim-pwm", "PWM output"), ("lim-diag", "Diagnostics")]:
        b.node(node_id, label, "software", "limiter")
    for source, target in [("lim-can", "lim-speed"), ("lim-speed", "lim-ctrl"), ("lim-ctrl", "lim-pwm"), ("lim-can", "lim-diag"),
                           ("lim-ctrl", "lim-diag")]:
        b.edge(source, target, "dataflow")
    # One housing for the instrument cluster and the telematics logger.
    b.container("housing", "Body controller housing", direction="tb")
    for node_id, label in [("cluster", "Instrument cluster"), ("logger", "Telematics logger")]:
        b.node(node_id, label, "ecu", "housing")
    for node_id, label in [("throttle", "Throttle actuator"), ("buzzer", "Warning buzzer"), ("lamp", "Limiter lamp")]:
        b.node(node_id, label, "actuator")
    # The CAN messages, named by their kind (the edge label): from the sending ECU or its CAN stack to the receiving ECU or
    # its CAN driver. An edge between two software components runs inside both ECUs and over the bus between them.
    first_can = len(b.edges)
    for source, target, message in [("abs", "lim-can", "WheelSpeed"), ("abs", "cluster", "WheelSpeed"), ("abs", "logger", "WheelSpeed"),
                                    ("eng-can", "lim-can", "EngineTorque"), ("eng-can", "cluster", "EngineRpm"),
                                    ("gps", "lim-can", "GpsSpeed"), ("gps", "logger", "GpsPosition"),
                                    ("switch", "lim-can", "LimitRequest")]:
        b.edge(source, target, message)
    can_edges = [e["id"] for e in b.edges[first_can:]]
    # The limiter's wired outputs, from its software to the actuators.
    first_wire = len(b.edges)
    for source, target, signal in [("lim-pwm", "throttle", "ThrottleCmd"), ("lim-diag", "buzzer", "Warning"),
                                   ("lim-diag", "lamp", "LampOn")]:
        b.edge(source, target, signal)
    wired = [e["id"] for e in b.edges[first_wire:]]
    return b.graph(containerStrategies={"engine": "organic"}, containerRouting={"engine": "straight"},
                   buses=[{"id": "can", "edges": can_edges, "label": "CAN 500 kbit/s"},
                          {"id": "limiter-out", "edges": wired, "label": "Limiter outputs (wired)"}])


def git_state():
    def run(*args):
        result = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True)
        return result.stdout.strip() if result.returncode == 0 else None
    status = run("status", "--porcelain")
    return {"gitRevision": run("rev-parse", "--short", "HEAD"), "gitDirty": None if status is None else bool(status)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=ROOT / "benchmarks" / "datasets")
    parser.add_argument("--demo-out", type=Path, default=None,
                        help="also copy demo datasets and an index.json for the web app")
    parser.add_argument("--no-manifest", action="store_true", help="leave benchmarks/manifest.json unchanged")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    # name, builder, title (UI label), description, source, demo (shown in the web app)
    datasets = [
        ("cloud-architecture", cloud_architecture, "Cloud architecture", "Cloud architecture: regions, zones, subnets, services, data stores.", "hand-authored", True),
        ("activity-order", activity_order, "Activity diagram", "UML-style activity diagram with swimlanes, forks, joins, decisions and rework loops.", "hand-authored", True),
        ("deployment-cluster", deployment_cluster, "Deployment (Kubernetes)", "Kubernetes-style deployment: cluster, namespaces, deployments, pods, services, scrapes.", "hand-authored", True),
        ("state-order", state_order, "State machine (order)", "Hierarchical state machine with a composite state, self-loop, parallel transitions and cycles.", "hand-authored", True),
        ("state-tcp", state_tcp, "State machine (TCP)", "Protocol state machine (TCP connection states), dense cycles.", "hand-authored", True),
        ("workflow-dag", workflow_dag, "Workflow-DAG", "Data-pipeline DAG in eight stages with skip edges and retry loops.", "seeded (seed 7)", True),
        ("arqlet-pipeline", arqlet_pipeline, "Arqlet-Pipeline", "Arqlet's own layout and operation pipeline, grouped by crate, with feedback loops.", "hand-authored", True),
        ("layerweave-code", layerweave_code, "Arqlet-Code", "Arqlet's own crates, modules and functions with heuristic call edges.", "repository source", True),
        ("org-chart", org_chart, "Org chart", "Company hierarchy with reporting lines and dotted-line matrix edges.", "seeded (seed 5)", True),
        ("nested-tree-radial", nested_tree_radial, "Nested tree and radial", "Five nested container levels; every area is a tree or forest, alternating tree and radial strategies.", "benchmark input", True),
        ("uml-class", uml_class, "UML class diagram", "Class diagram with packages, attributes, composition, inheritance, bidirectional associations.", "hand-authored", True),
        ("er-diagram", er_diagram, "ER diagram", "Entity-relationship diagram; relationships are nodes with cardinality edges, includes a self-relation.", "hand-authored", True),
        ("bpmn-pools", bpmn_pools, "BPMN with pools", "BPMN collaboration with three pools, lanes, message flows, a boundary timer and a retry loop.", "hand-authored", True),
        ("network-vlan", network_vlan, "Network with VLANs", "Redundant data-center network: firewalls, core switches, VLAN groups, hosts, parallel links.", "seeded (seed 9)", True),
        ("petri-net", petri_net, "Petri net", "Producer/consumer Petri net with a lock, a bounded buffer, and an inhibitor arc.", "hand-authored", True),
        ("petri-net-shapes", petri_net_shapes, "Petri net (shapes)", "Petri net with circular places and rectangular transitions, organic placement and straight edges.", "hand-authored", True),
        ("mixed-layouts", mixed_layouts, "Mixed layouts", "Organic root and circle container, a layered left-to-right container and a layered top-to-bottom container, straight edges.", "hand-authored", True),
        ("mixed-large", mixed_large, "Mixed layouts (large)", "About 100 nodes: organic root, a circle container, layered containers with nested sub-containers, round and rectangular nodes, straight edges. The engine reports the conflicts that remain (straight edges through nodes, a child outside its area).", "seeded (seed 21)", True),
        ("self-loops", self_loops, "Self-loops", "States with none, one, two and three self-loops, parallel and antiparallel edges, round and rectangular, organic root and a layered container.", "hand-authored", True),
        ("fixed-ports", fixed_ports, "Placed ports", "A hub whose four edges leave at ports placed on different sides; the optimisation keeps them.", "hand-authored", True),
        ("can-bus", can_bus, "CAN bus (vehicle)", "Vehicle speed limiter on a CAN bus: ECUs on one shared bus line, the limiter's wired outputs as a second bus; the limiter ECU holds a layered software diagram, the engine ECU an organic one, and two ECUs share a housing.", "hand-authored", True),
        ("synthetic-2k", lambda: synthetic(2000, 11), "Synthetic 2k", "Random hierarchy, 2,000 leaves, cycles, self-loops, parallel and unresolved edges.", "seeded (seed 11)", False),
        ("synthetic-20k", lambda: synthetic(20000, 12), "Synthetic 20k", "Random hierarchy, 20,000 leaves; scale probe for the view import path.", "seeded (seed 12)", False),
    ]
    for size in [25, 50, 100, 200, 300]:
        datasets.extend([
            (f"organic-{size}", lambda size=size: organic_ladder(size), f"Organic {size}",
             "Connected organic graph with sparse deterministic chords; default straight routing isolates placement and repair.",
             f"seeded (seed {20261027 + size})", False),
            (f"tree-{size}", lambda size=size: tree_ladder(size), f"Tree {size}",
             "Balanced binary tree with straight routing for measuring tree placement independently of route search.",
             "deterministic", False),
        ])
    entries = []
    demo = []
    for name, build, title, description, source, shown in datasets:
        graph = build()
        text = json.dumps(graph, indent=1, sort_keys=True, ensure_ascii=False) + "\n"
        (args.out / f"{name}.json").write_text(text)
        entry = {
            "name": name,
            "title": title,
            "file": f"datasets/{name}.json",
            "sha256": hashlib.sha256(text.encode()).hexdigest(),
            "bytes": len(text.encode()),
            "nodes": len(graph["nodes"]),
            "edges": len(graph["edges"]),
            "unresolvedEdges": sum(1 for e in graph["edges"] if e["target"] is None),
            "source": source,
            "description": description,
            "demo": shown,
        }
        entries.append(entry)
        if shown:
            demo.append((entry, text))
        print(f"{name}: {len(graph['nodes'])} nodes, {len(graph['edges'])} edges")
    if args.demo_out:
        args.demo_out.mkdir(parents=True, exist_ok=True)
        index = []
        for entry, text in demo:
            (args.demo_out / f"{entry['name']}.json").write_text(text)
            index.append({"name": entry["name"], "title": entry["title"], "nodes": entry["nodes"], "edges": entry["edges"],
                          "description": entry["description"], "source": entry["source"]})
        (args.demo_out / "index.json").write_text(json.dumps(index, indent=1, ensure_ascii=False) + "\n")
    if not args.no_manifest:
        manifest = {"generator": f"benchmarks/generate.py v{GENERATOR_VERSION}", **git_state(), "datasets": entries}
        (ROOT / "benchmarks" / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
