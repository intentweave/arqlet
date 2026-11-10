"""Six interaction scenes for the browser package. Deterministic: rerunning writes identical files.

Each scene is a graph in the contract format (`defaultDirection`, `containerRouting`, ...). Scenes
exercise what the browser must show and edit: shapes and their outlines, organic and tree strategies,
per-container routing, content of changing sizes, and a dense routing field.
"""
import json
import pathlib

OUT = pathlib.Path(__file__).resolve().parent


def node(id, label, parent=None, shape=None, kind="node"):
    n = {"id": id, "kind": kind, "label": label, "parentId": parent}
    if shape:
        n["shape"] = shape
    return n


def edge(id, source, target, kind="flow"):
    return {"id": id, "kind": kind, "source": source, "target": target}


def mixed_shapes():
    nodes = [node("hub", "Hub", shape="circle")]
    edges = []
    for i, shape in enumerate(["rect", "circle", "square", "circle", "rect", "circle"]):
        nid = f"n{i}"
        nodes.append(node(nid, f"Item {i}", shape=shape))
        edges.append(edge(f"h{i}", "hub", nid))
    nodes += [node("sink", "Sink", shape="circle"), node("tail", "Tail")]
    edges += [edge("s1", "n1", "sink"), edge("s2", "n3", "sink"), edge("t1", "sink", "tail"), edge("c1", "n0", "n2"), edge("c2", "n4", "n5")]
    return {"nodes": nodes, "edges": edges, "defaultDirection": "auto"}


def organic_clusters():
    nodes, edges = [], []
    clusters = {"A": 5, "B": 4, "C": 6}
    for c, size in clusters.items():
        nodes.append(node(f"{c}", f"Cluster {c}", kind="group"))
        for i in range(size):
            nid = f"{c}{i}"
            nodes.append(node(nid, f"{c} {i}", shape="circle" if i % 3 == 0 else None))
            if i > 0:
                edges.append(edge(f"{c}e{i}", f"{c}{i - 1}", nid))
        edges.append(edge(f"{c}loop", f"{c}1", f"{c}{size - 1}"))
    edges += [edge("ab", "A2", "B1"), edge("bc", "B2", "C3"), edge("ca", "C0", "A4")]
    return {"nodes": nodes, "edges": edges, "defaultDirection": "auto", "defaultStrategy": "organic"}


def tree_and_radial():
    nodes = [node("root", "Root"), node("left", "Left"), node("right", "Right", shape="circle")]
    edges = [edge("rl", "root", "left"), edge("rr", "root", "right")]
    for i in range(3):
        nodes.append(node(f"l{i}", f"Leaf {i}", parent=None))
        edges.append(edge(f"ll{i}", "left", f"l{i}"))
    nodes.append(node("ring", "Ring", kind="group"))
    for i in range(5):
        nodes.append(node(f"r{i}", f"Ring {i}", parent="ring", shape="circle" if i % 2 else None))
        edges.append(edge(f"rr{i}", "right" if i == 0 else f"r{i - 1}", f"r{i}"))
    return {
        "nodes": nodes,
        "edges": edges,
        "defaultDirection": "lr",
        "defaultStrategy": "tree",
        "containerDirections": {"ring": "auto"},
        "containerStrategies": {"ring": "radial"},
    }


def nested_mixed():
    nodes = [
        node("outer", "Outer", kind="group"),
        node("inner", "Inner", parent="outer", kind="group"),
        node("p", "P", parent="inner"),
        node("q", "Q", parent="inner", shape="circle"),
        node("r", "R", parent="outer"),
        node("s", "S"),
    ]
    edges = [edge("pq", "p", "q"), edge("pr", "p", "r"), edge("qs", "q", "s"), edge("rs", "r", "s")]
    return {
        "nodes": nodes,
        "edges": edges,
        "defaultDirection": "lr",
        "containerDirections": {"outer": "tb", "inner": "auto"},
        "defaultRouting": "orthogonal",
        "containerRouting": {"inner": "straight"},
    }


def changing_content():
    labels = ["Short", "A label that is considerably longer than the rest of the row", "Mid length text",
              "Another long label to wrap across two lines", "X", "Medium sized label here"]
    nodes = [node(f"c{i}", label) for i, label in enumerate(labels)]
    edges = [edge(f"ce{i}", f"c{i}", f"c{i + 1}") for i in range(len(labels) - 1)]
    return {"nodes": nodes, "edges": edges, "defaultDirection": "lr"}


def dense_field():
    nodes = [node(f"d{i}", f"D{i}") for i in range(16)]
    edges = []
    k = 0
    for i in range(16):
        for step in (1, 5):
            j = (i + step) % 16
            edges.append(edge(f"de{k}", f"d{i}", f"d{j}"))
            k += 1
    return {"nodes": nodes, "edges": edges, "defaultDirection": "auto"}


def large_mixed(n):
    """Mixed rectangles and circles in a grid of groups: n nodes in clusters of 10."""
    nodes, edges = [], []
    for g in range(n // 10):
        group = f"g{g}"
        nodes.append(node(group, f"Group {g}", kind="group"))
        for i in range(10):
            nid = f"{group}n{i}"
            shape = ["rect", "circle", "square"][(g + i) % 3]
            nodes.append(node(nid, f"Item {g}.{i}", parent=group, shape=None if shape == "rect" else shape))
            if i:
                edges.append(edge(f"{group}e{i}", f"{group}n{i - 1}", nid))
        if g:
            edges.append(edge(f"x{g}", f"g{g - 1}n0", f"{group}n0"))
    return {"nodes": nodes, "edges": edges, "defaultDirection": "auto"}


def large_organic(n):
    """Free placement of clusters: each cluster is a container of about 25 nodes (a ring with a chord); clusters are linked.
    Organic places the children of the root (the clusters) and each cluster's own area stays small, within its bound."""
    nodes, edges = [], []
    clusters = max(1, n // 25)
    for c in range(clusters):
        group = f"k{c}"
        size = n // clusters if c < clusters - 1 else n - (n // clusters) * (clusters - 1)
        nodes.append(node(group, f"Cluster {c}", kind="group"))
        for i in range(size):
            nodes.append(node(f"{group}_{i}", f"{group} {i}", parent=group, shape="circle" if i % 4 == 0 else None))
            edges.append(edge(f"{group}r{i}", f"{group}_{i}", f"{group}_{(i + 1) % size}"))
        edges.append(edge(f"{group}c", f"{group}_0", f"{group}_{size // 2}"))
        if c:
            edges.append(edge(f"link{c}", f"k{c - 1}_1", f"{group}_1"))
    return {"nodes": nodes, "edges": edges, "defaultDirection": "auto", "defaultStrategy": "organic"}


def large_tree(n):
    """A tree of n nodes, branching 3, under the root; the root places it along lr."""
    nodes = [node("t0", "Root")]
    edges = []
    for i in range(1, n):
        nodes.append(node(f"t{i}", f"Node {i}", shape="circle" if i % 5 == 0 else None))
        edges.append(edge(f"te{i}", f"t{(i - 1) // 3}", f"t{i}"))
    return {"nodes": nodes, "edges": edges, "defaultDirection": "lr", "defaultStrategy": "tree"}


def large_dense(n):
    """A dense field: every node links to the next and to the one five steps on, so routes must weave."""
    nodes = [node(f"d{i}", f"D{i}") for i in range(n)]
    edges = []
    k = 0
    for i in range(n):
        for step in (1, 5):
            edges.append(edge(f"de{k}", f"d{i}", f"d{(i + step) % n}"))
            k += 1
    return {"nodes": nodes, "edges": edges, "defaultDirection": "auto"}


LARGE = {
    "large-mixed-200": lambda: large_mixed(200),
    "large-mixed-450": lambda: large_mixed(450),
    "large-organic-200": lambda: large_organic(200),
    "large-organic-450": lambda: large_organic(450),
    "large-tree-200": lambda: large_tree(200),
    "large-tree-450": lambda: large_tree(450),
    "large-dense-200": lambda: large_dense(200),
    "large-dense-450": lambda: large_dense(450),
}

SCENES = {
    "mixed-shapes": mixed_shapes,
    "organic-clusters": organic_clusters,
    "tree-radial": tree_and_radial,
    "nested-mixed": nested_mixed,
    "changing-content": changing_content,
    "dense-field": dense_field,
}


def main():
    for name, build in {**SCENES, **LARGE}.items():
        text = json.dumps(build(), indent=2, sort_keys=True) + "\n"
        (OUT / f"{name}.json").write_text(text)
        print(name, len(text))


if __name__ == "__main__":
    main()
