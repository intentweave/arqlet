#!/usr/bin/env python3
"""Independent oracle for the Cypher fixtures (ADR 0021, stage M0).

Every expected result in `cases.jsonl` is computed here, in plain Python straight from the graph files, with no Cypher
engine involved. The queries are the claim; the Python functions are the independent reading of what they must return.

    python3 examples/cypher/oracle.py --write     # regenerate cases.jsonl
    python3 examples/cypher/oracle.py --check     # fail if cases.jsonl or the graph hashes differ
    python3 examples/cypher/oracle.py --summary   # counts per graph and status

Data model (ADR 0021, M0): a node's properties are the virtual ones (`id`, `kind`, `label`, `parentId`) plus the payload
table's; an edge's are `id`, `kind`, `weight` (when present) plus the payload table's. The `kind` of a node is its single
label, the `kind` of an edge its relationship type. Edges without a target, or with a target the graph does not have,
are excluded before any query runs.
"""
import argparse
import collections
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "cases.jsonl")
SUMS = os.path.join(HERE, "graphs", "SHA256SUMS")


class Graph:
    def __init__(self, name, payload=None):
        raw = json.load(open(os.path.join(HERE, "graphs", f"{name}.json")))
        extra = json.load(open(os.path.join(HERE, "payloads", f"{payload}.json"))) if payload else {"nodes": {}, "edges": {}}
        self.nodes = {}
        for n in raw["nodes"]:
            props = {"id": n["id"], "kind": n["kind"], "label": n["label"], "parentId": n.get("parentId")}
            for k, v in extra["nodes"].get(n["id"], {}).items():
                assert k not in props, f"payload key {k} collides with a virtual property"
                props[k] = v
            self.nodes[n["id"]] = props
        self.edges = []
        for e in raw["edges"]:
            if e.get("target") is None or e["target"] not in self.nodes:
                continue
            props = {"id": e["id"], "kind": e["kind"]}
            if e.get("weight") is not None:
                props["weight"] = e["weight"]
            for k, v in extra["edges"].get(e["id"], {}).items():
                assert k not in props, f"payload key {k} collides with a virtual property"
                props[k] = v
            self.edges.append({"id": e["id"], "kind": e["kind"], "source": e["source"], "target": e["target"], "props": props})

    def of_kind(self, *kinds):
        return [e for e in self.edges if e["kind"] in kinds]

    def nodes_of(self, kind):
        return [p for p in self.nodes.values() if p["kind"] == kind]


def count(n):
    return [[n]]


def undirected_matches(edges):
    """`(a)-[r]-(b)`: a self-loop matches once, any other edge twice (once per direction)."""
    return sum(1 if e["source"] == e["target"] else 2 for e in edges)


def two_hops(edges):
    """Pairs of distinct edges e1, e2 with e1.target == e2.source (relationship uniqueness within one pattern)."""
    return [(a, b) for a in edges for b in edges if a["id"] != b["id"] and a["target"] == b["source"]]


def top_by(counter, n):
    return [[k, v] for k, v in sorted(counter.items(), key=lambda kv: (-kv[1], kv[0]))[:n]]


# ---- cases ---------------------------------------------------------------------------------------------------------
# (id, graph, payload, query, status, ordered, columns, expected-rows function or error class, note)
# status: supported  - must give exactly these rows
#         later      - in ADR 0021's "Later" column: must be rejected now; rows recorded for the day it is supported
#         rejected   - outside the first release; must be rejected with `error`
#         open       - the ADR is silent; decide before implementing (rows recorded when they are clear)
#         invalid    - not valid Cypher; must give a syntax error


def cases():
    C = []

    def add(cid, graph, query, status, fn=None, columns=None, ordered=False, payload=None, error=None, note=None):
        C.append((cid, graph, payload, query, status, ordered, columns, fn, error, note))

    # small.json: a self-loop, a parallel edge and an unresolved edge among five `calls` edges.
    add("small-01", "small", "MATCH (a)-[r:calls]->(b) RETURN a.id, b.id", "supported", lambda g: [[e["source"], e["target"]] for e in g.of_kind("calls")], ["a.id", "b.id"], note="the unresolved edge is excluded; the self-loop and the parallel edge each count")
    add("small-02", "small", "MATCH (a)-[r:calls]->(a) RETURN r.id", "supported", lambda g: [[e["id"]] for e in g.of_kind("calls") if e["source"] == e["target"]], ["r.id"], note="a pattern that reuses a variable finds the self-loop")
    add("small-03", "small", "MATCH (n) WHERE n.parentId = 'root' RETURN n.label ORDER BY n.label", "supported", lambda g: [[p["label"]] for p in sorted(g.nodes.values(), key=lambda p: p["label"]) if p["parentId"] == "root"], ["n.label"], ordered=True)
    add("small-04", "small", "MATCH (n:directory) RETURN count(n)", "supported", lambda g: count(len(g.nodes_of("directory"))), ["count(n)"])
    add("small-05", "small", "MATCH (a)-[r:calls]-(b) RETURN count(*)", "supported", lambda g: count(undirected_matches(g.of_kind("calls"))), ["count(*)"], note="undirected: the self-loop matches once, every other edge twice")
    add("small-06", "small", "MATCH ()-[r]->() RETURN count(r)", "supported", lambda g: count(len(g.edges)), ["count(r)"], note="five edges in the file, four resolved")
    add("small-07", "small", "MATCH (n) WHERE n.nope IS NULL RETURN count(n)", "supported", lambda g: count(len(g.nodes)), ["count(n)"], note="a property the node does not have is null")

    # layerweave-code.json: 148 nodes; five labels occur more than once (`run` twice, `lib.rs` eight times).
    add("code-01", "layerweave-code", "MATCH (n:function) RETURN count(n)", "supported", lambda g: count(len(g.nodes_of("function"))), ["count(n)"])
    add("code-02", "layerweave-code", "MATCH (a)-[r:calls]->(b) RETURN count(r)", "supported", lambda g: count(len(g.of_kind("calls"))), ["count(r)"])
    add("code-03", "layerweave-code", "MATCH (a)-[r]->(a) RETURN a.label ORDER BY a.label", "supported", lambda g: [[g.nodes[e["source"]]["label"]] for e in sorted((e for e in g.edges if e["source"] == e["target"]), key=lambda e: g.nodes[e["source"]]["label"])], ["a.label"], ordered=True, note="four self-loops of any kind")
    add("code-04", "layerweave-code", "MATCH (a)-[:calls]->(b)-[:calls]->(c) RETURN count(*)", "supported", lambda g: count(len(two_hops(g.of_kind("calls")))), ["count(*)"], note="relationship uniqueness: the two edges of a path differ")
    add("code-05", "layerweave-code", "MATCH (a)-[:calls]->(b) RETURN a.label, count(b) AS n ORDER BY n DESC, a.label LIMIT 3", "supported", lambda g: top_by(collections.Counter(g.nodes[e["source"]]["label"] for e in g.of_kind("calls")), 3), ["a.label", "n"], ordered=True, note="groups by the label value: two nodes called `run` form one group of 10")
    add("code-06", "layerweave-code", "MATCH (a)-[:calls]->(b) RETURN a.id, count(b) AS n ORDER BY n DESC, a.id LIMIT 3", "supported", lambda g: top_by(collections.Counter(e["source"] for e in g.of_kind("calls")), 3), ["a.id", "n"], ordered=True, note="groups by node: the same query by id gives different leaders than by label")
    add("code-07", "layerweave-code", "MATCH (n) WHERE n.parentId IS NULL RETURN count(n)", "supported", lambda g: count(sum(1 for p in g.nodes.values() if p["parentId"] is None)), ["count(n)"])
    add("code-08", "layerweave-code", "MATCH (a:container)-[r:depends]->(b:container) RETURN count(r)", "supported", lambda g: count(sum(1 for e in g.of_kind("depends") if g.nodes[e["source"]]["kind"] == "container" and g.nodes[e["target"]]["kind"] == "container")), ["count(r)"])

    # org-chart.json: edge kinds with a space need backticks.
    add("org-01", "org-chart", "MATCH (a)-[r:`reports to`]->(b) RETURN count(r)", "supported", lambda g: count(len(g.of_kind("reports to"))), ["count(r)"])
    add("org-02", "org-chart", "MATCH ()-[r:`dotted line`]->() RETURN count(r)", "supported", lambda g: count(len(g.of_kind("dotted line"))), ["count(r)"])
    add("org-03", "org-chart", "MATCH (p:person) RETURN count(p)", "supported", lambda g: count(len(g.nodes_of("person"))), ["count(p)"])
    add("org-04", "org-chart", "MATCH (a)-[:`reports to`]->(m) RETURN m.label, count(a) AS reports ORDER BY reports DESC, m.label LIMIT 3", "supported", lambda g: top_by(collections.Counter(g.nodes[e["target"]]["label"] for e in g.of_kind("reports to")), 3), ["m.label", "reports"], ordered=True)

    # network-vlan.json
    add("net-01", "network-vlan", "MATCH (n:host) RETURN count(n)", "supported", lambda g: count(len(g.nodes_of("host"))), ["count(n)"])
    add("net-02", "network-vlan", "MATCH (a)-[r:deny]->(b) RETURN a.label, b.label ORDER BY a.label, b.label", "supported", lambda g: [[g.nodes[e["source"]]["label"], g.nodes[e["target"]]["label"]] for e in sorted(g.of_kind("deny"), key=lambda e: (g.nodes[e["source"]]["label"], g.nodes[e["target"]]["label"]))], ["a.label", "b.label"], ordered=True)
    add("net-03", "network-vlan", "MATCH (a)-[r:`wan backup`]->(b) RETURN count(r)", "supported", lambda g: count(len(g.of_kind("wan backup"))), ["count(r)"])
    add("net-04", "network-vlan", "MATCH (a:switch)-[r]-(b:switch) RETURN count(*)", "supported", lambda g: count(undirected_matches([e for e in g.edges if g.nodes[e["source"]]["kind"] == "switch" and g.nodes[e["target"]]["kind"] == "switch"])), ["count(*)"], note="eleven switch-to-switch edges of different kinds, each matched in both directions")
    add("net-05", "network-vlan", "MATCH (f:firewall)-[r:allow|deny]->(s:switch) RETURN f.label, count(r) AS rules ORDER BY f.label", "supported", lambda g: sorted([[k, v] for k, v in collections.Counter(g.nodes[e["source"]]["label"] for e in g.of_kind("allow", "deny") if g.nodes[e["source"]]["kind"] == "firewall" and g.nodes[e["target"]]["kind"] == "switch").items()]), ["f.label", "rules"], ordered=True, note="relationship type alternatives with a grouping")

    # Constructs the first release must reject (ADR 0021, items 4 and 5). decypher drops the first four silently.
    add("reject-01", "small", "MATCH p = shortestPath((a)-[*]-(b)) RETURN p", "later", error="unsupported", note="decypher drops shortestPath (upstream #133); the syntax-tree guard must reject it")
    add("reject-02", "small", "MATCH (a)-[:calls]->{1,3}(b) RETURN b", "later", error="unsupported", note="decypher lowers the quantifier to one hop (upstream #134)")
    add("reject-03", "small", "MATCH (n WHERE n.label = 'Parse') RETURN n", "later", error="unsupported", note="decypher drops the WHERE inside the pattern (upstream #132)")
    add("reject-04", "small", "MATCH (a)((x)-[:calls]->(y)){2}(b) RETURN b", "later", error="unsupported", note="decypher splits a quantified path pattern into three patterns (upstream #134)")
    add("reject-05", "small", "CREATE (n:function {label: 'x'})", "rejected", error="unsupported", note="writes are outside the first release")
    add("reject-06", "small", "MATCH (n) DETACH DELETE n", "rejected", error="unsupported")
    add("reject-07", "small", "CALL db.index.vector.queryNodes('emb', 5, $v) YIELD node, score RETURN node", "rejected", error="unsupported", note="procedures other than the ones we define")
    add("reject-08", "small", "RETURN foo_bar(1) AS x", "rejected", error="unsupported", note="decypher accepts unknown function names; our function table does not")
    add("reject-09", "small", "MATCH (n RETURN n", "invalid", error="syntax")

    # The cases of the spike (2026-11-21): a graph with a cycle, a self-loop and parallel edges. Properties come from the payload.
    P = "spike"

    def names(g, pred=lambda p: True):
        return [[p["name"]] for p in g.nodes.values() if p["kind"] == "Person" and pred(p)]

    def knows_paths(g, lo, hi):
        edges = g.of_kind("KNOWS")
        paths = [[e] for e in edges]
        found = []
        for length in range(1, hi + 1):
            if length >= lo:
                found.extend(paths)
            paths = [p + [e] for p in paths for e in edges if e["source"] == p[-1]["target"] and all(e["id"] != q["id"] for q in p)]
        return found

    add("spike-01", "spike", "MATCH (n:Person) RETURN n.name", "supported", lambda g: names(g), ["n.name"], payload=P)
    add("spike-02", "spike", "MATCH (a)-[r:KNOWS]->(b) RETURN a.name, b.name", "supported", lambda g: [[g.nodes[e["source"]]["name"], g.nodes[e["target"]]["name"]] for e in g.of_kind("KNOWS")], ["a.name", "b.name"], payload=P, note="six edges, two of them parallel and one a self-loop")
    add("spike-03", "spike", "MATCH (a)-[r:KNOWS|FOLLOWS]-(b) RETURN count(*)", "supported", lambda g: count(undirected_matches(g.of_kind("KNOWS", "FOLLOWS"))), ["count(*)"], payload=P, note="13: six KNOWS edges and one FOLLOWS edge; the self-loop once")
    def reached_names(g):
        return [[n] for n in sorted({g.nodes[p[-1]["target"]]["name"] for p in knows_paths(g, 1, 2)})]

    def optional_follows(g):
        rows = []
        for p in g.nodes.values():
            follows = [e for e in g.of_kind("FOLLOWS") if e["source"] == p["id"]]
            if follows:
                rows.extend([p["name"], g.nodes[e["target"]]["name"]] for e in follows)
            else:
                rows.append([p["name"], None])
        return rows

    add("spike-04", "spike", "MATCH (a)-[:KNOWS*1..2]->(b) RETURN DISTINCT b.name", "supported", reached_names, ["b.name"], payload=P)
    add("spike-05", "spike", "MATCH (a)-[:KNOWS*2]->(b) RETURN count(*)", "supported", lambda g: count(len(knows_paths(g, 2, 2))), ["count(*)"], payload=P, note="10 paths of exactly two distinct edges")
    add("spike-06", "spike", "MATCH (a)-[r1:KNOWS]->(b)<-[r2:KNOWS]-(c) RETURN count(*)", "supported", lambda g: count(sum(1 for x in g.of_kind("KNOWS") for y in g.of_kind("KNOWS") if x["id"] != y["id"] and x["target"] == y["target"])), ["count(*)"], payload=P, note="8: r1 and r2 are different edges")
    add("spike-07", "spike", "MATCH (a:Person) OPTIONAL MATCH (a)-[:FOLLOWS]->(b) RETURN a.name, b.name", "supported", optional_follows, ["a.name", "b.name"], payload=P, note="three rows with null, one with Ann")
    add("spike-08", "spike", "MATCH (a)-[:KNOWS]->(b) WITH a, count(b) AS c WHERE c > 1 RETURN a.name, c", "supported", lambda g: [[g.nodes[k]["name"], v] for k, v in collections.Counter(e["source"] for e in g.of_kind("KNOWS")).items() if v > 1], ["a.name", "c"], payload=P)
    add("spike-09", "spike", "MATCH (n:Person) WHERE n.nick IS NULL RETURN n.name", "supported", lambda g: names(g, lambda p: "nick" not in p), ["n.name"], payload=P)
    add("spike-10", "spike", "MATCH (n:Person) WHERE n.nick = 'x' RETURN n.name", "supported", lambda g: [], ["n.name"], payload=P, note="comparing null gives null, which filters the row out")
    add("spike-11", "spike", "MATCH (n:Person) WHERE n.name STARTS WITH 'A' OR n.name =~ 'B.*' RETURN n.name", "later", lambda g: names(g, lambda p: p["name"].startswith("A") or p["name"].startswith("B")), ["n.name"], payload=P, error="unsupported", note="`=~` is in ADR 0021's Later column: Cypher's regular expressions are Java's, and `arqlet-query` has no third-party dependency")
    add("spike-20", "spike", "MATCH (n:Person) WHERE n.name STARTS WITH 'A' OR n.name ENDS WITH 'b' OR n.name CONTAINS 'i' RETURN n.name", "supported", lambda g: names(g, lambda p: p["name"].startswith("A") or p["name"].endswith("b") or "i" in p["name"]), ["n.name"], payload=P, note="the string operators that stay in the first release: Ann starts with A, Bob ends with b, Cid contains i")
    add("spike-12", "spike", "MATCH (a:Person) WHERE EXISTS { MATCH (a)-[:KNOWS]->(b) WHERE b.age > 35 } RETURN a.name", "later", lambda g: [[p["name"]] for p in g.nodes.values() if any(e["source"] == p["id"] and g.nodes[e["target"]]["age"] > 35 for e in g.of_kind("KNOWS"))], ["a.name"], payload=P, error="unsupported")
    add("spike-13", "spike", "UNWIND [1, 2, 3] AS x RETURN x * 2", "supported", lambda g: [[2], [4], [6]], ["x * 2"], payload=P)
    add("spike-14", "spike", "MATCH (n:Person) RETURN n.name ORDER BY n.age DESC SKIP 1 LIMIT 2", "supported", lambda g: [[p["name"]] for p in sorted(g.nodes.values(), key=lambda p: -p["age"])][1:3], ["n.name"], ordered=True, payload=P)
    add("spike-15", "spike", "MATCH p = shortestPath((a:Person {name: 'Dee'})-[*]->(b:Person {name: 'Bob'})) RETURN length(p)", "later", lambda g: count(2), ["length(p)"], payload=P, error="unsupported", note="Dee -> Ann -> Bob")
    add("spike-16", "spike", "MATCH (a:Person {name: 'Ann'}) RETURN size([(a)-[:KNOWS]->(b) | b.name])", "open", lambda g: count(sum(1 for e in g.of_kind("KNOWS") if e["source"] == "ann")), ["size([(a)-[:KNOWS]->(b) | b.name])"], payload=P, note="pattern comprehension is not in the ADR's subset table")
    add("spike-17", "spike", "MATCH (n:Person) RETURN CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END AS g ORDER BY g", "supported", lambda g: [["minor" if p["age"] < 18 else "adult"] for p in sorted(g.nodes.values(), key=lambda p: ("minor" if p["age"] < 18 else "adult"))], ["g"], ordered=True, payload=P)
    add("spike-18", "spike", "MATCH ()-[r:KNOWS {since: 2020}]->() RETURN r.id", "supported", lambda g: [[e["id"]] for e in g.of_kind("KNOWS") if e["props"].get("since") == 2020], ["r.id"], payload=P, note="a payload property on an edge, used in the pattern")
    add("spike-19", "spike", "MATCH (n:Person) RETURN n.nick", "supported", lambda g: [[None] for p in g.nodes.values() if p["kind"] == "Person"], ["n.nick"], payload=P, note="null as an output value")

    # The function table and UNION (2026-11-25). `id` and `elementId` give the model's string id.
    def people(g):
        return [p for p in g.nodes.values() if p["kind"] == "Person"]

    def per_person(fn):
        return lambda g: [fn(p) for p in people(g)]

    def edge_counts_by_type(g):
        c = collections.Counter(e["kind"] for e in g.edges)
        return [[k, c[k]] for k in sorted(c)]

    def follows_or_null(g):
        rows = []
        for p in people(g):
            follows = [e for e in g.of_kind("FOLLOWS") if e["source"] == p["id"]]
            for e in follows:
                m = g.nodes[e["target"]]
                rows.append([p["name"], e["kind"], m["id"], m["name"].upper(), len(m["name"])])
            if not follows:
                rows.append([p["name"], None, None, None, None])
        return rows

    def known_per_person(g):
        known = collections.defaultdict(list)
        for e in g.of_kind("KNOWS"):
            known[e["source"]].append(g.nodes[e["target"]]["name"])
        return [[g.nodes[k]["name"], len(v), g.nodes[k]["name"].upper()] for k, v in known.items()]

    add("fn-01", "spike", "MATCH (n:Person) RETURN id(n), elementId(n), labels(n)", "supported", per_person(lambda p: [p["id"], p["id"], [p["kind"]]]), ["id(n)", "elementId(n)", "labels(n)"], payload=P, note="a node has one label, its kind")
    add("fn-02", "spike", "MATCH (a)-[r:FOLLOWS]->(b) RETURN id(r), type(r), id(a), id(b)", "supported", lambda g: [[e["id"], e["kind"], e["source"], e["target"]] for e in g.of_kind("FOLLOWS")], ["id(r)", "type(r)", "id(a)", "id(b)"], payload=P)
    add("fn-03", "spike", "MATCH ()-[r]->() RETURN type(r) AS t, count(*) AS c ORDER BY t", "supported", edge_counts_by_type, ["t", "c"], ordered=True, payload=P, note="a function as a grouping key")
    add("fn-04", "spike", "MATCH (n:Person) RETURN toUpper(n.name), toLower(n.name), size(n.name), reverse(n.name)", "supported", per_person(lambda p: [p["name"].upper(), p["name"].lower(), len(p["name"]), p["name"][::-1]]), ["toUpper(n.name)", "toLower(n.name)", "size(n.name)", "reverse(n.name)"], payload=P)
    add("fn-05", "spike", "MATCH (n:Person) WHERE toLower(n.name) STARTS WITH 'a' RETURN n.name", "supported", lambda g: [[p["name"]] for p in people(g) if p["name"].lower().startswith("a")], ["n.name"], payload=P, note="a function in WHERE")
    add("fn-06", "spike", "RETURN size('stra\u00dfe') AS a, toUpper('stra\u00dfe') AS b, reverse('a\u00f1b') AS c, size([1, 2, 3]) AS d, reverse([1, 2, 3]) AS e", "supported", lambda g: [[len("stra\u00dfe"), "stra\u00dfe".upper(), "a\u00f1b"[::-1], 3, [3, 2, 1]]], ["a", "b", "c", "d", "e"], payload=P, note="sizes count characters, not bytes; upper case may be longer")
    add("fn-07", "spike", "RETURN trim('  a b  ') AS a, ltrim('  a ') AS b, rtrim(' a  ') AS c", "supported", lambda g: [["  a b  ".strip(), "  a ".lstrip(), " a  ".rstrip()]], ["a", "b", "c"], payload=P)
    add("fn-08", "spike", "RETURN replace('banana', 'an', 'AN') AS a, replace('ab', '', '-') AS b, split('a,b,,c,', ',') AS c", "supported", lambda g: [["banana".replace("an", "AN"), "ab".replace("", "-"), "a,b,,c,".split(",")]], ["a", "b", "c"], payload=P, note="empty parts are kept")
    add("fn-09", "spike", "RETURN substring('hello', 1) AS a, substring('hello', 1, 3) AS b, substring('hello', 9) AS c, left('hello', 2) AS d, right('hello', 2) AS e, left('hi', 9) AS f", "supported", lambda g: [["hello"[1:], "hello"[1:4], "hello"[9:], "hello"[:2], "hello"[-2:], "hi"[:9]]], ["a", "b", "c", "d", "e", "f"], payload=P, note="positions start at 0")
    add("fn-10", "spike", "MATCH (n:Person) RETURN coalesce(n.nick, n.name, 'x') AS c", "supported", per_person(lambda p: [p.get("nick") or p["name"]]), ["c"], payload=P)
    add("fn-11", "spike", "RETURN toString(42) AS a, toString(-1.5) AS b, toString(2.0) AS c, toString(true) AS d, toString('s') AS e", "supported", lambda g: [[str(42), str(-1.5), str(2.0), "true", "s"]], ["a", "b", "c", "d", "e"], payload=P)
    add("fn-12", "spike", "MATCH (n:Person) OPTIONAL MATCH (n)-[r:FOLLOWS]->(m) RETURN n.name, type(r), id(m), toUpper(m.name), size(m.name)", "supported", follows_or_null, ["n.name", "type(r)", "id(m)", "toUpper(m.name)", "size(m.name)"], payload=P, note="a function of null is null")
    add("fn-13", "spike", "MATCH (a:Person)-[:KNOWS]->(b) RETURN a.name, size(collect(b.name)) AS n, toUpper(a.name) AS u", "supported", known_per_person, ["a.name", "n", "u"], payload=P, note="a function around an aggregate, beside a function that groups")
    add("fn-14", "spike", "MATCH (n:Person) RETURN collect(toLower(n.name)) AS l", "supported", lambda g: [[[p["name"].lower() for p in people(g)]]], ["l"], payload=P, note="a function inside an aggregate")
    add("fn-15", "spike", "MATCH (n:Person) RETURN coalesce(sum(n.missing), 0) + count(*) AS x", "supported", lambda g: [[0 + len(people(g))]], ["x"], payload=P)
    add("fn-16", "spike", "RETURN toUpper(null) AS a, size(null) AS b, replace('a', null, 'b') AS c, substring(null, 1) AS d, split(null, ',') AS e, coalesce(null, null) AS f", "supported", lambda g: [[None] * 6], ["a", "b", "c", "d", "e", "f"], payload=P)
    add("fn-17", "spike", "MATCH (n:Person) RETURN n.name ORDER BY size(n.name), toLower(n.name) DESC", "supported", lambda g: [[p["name"]] for p in sorted(sorted(people(g), key=lambda p: p["name"].lower(), reverse=True), key=lambda p: len(p["name"]))], ["n.name"], ordered=True, payload=P, note="functions as sort keys")
    add("fn-18", "spike", "RETURN toInteger('1') AS x", "rejected", error="unsupported", note="not in the function table")

    def older(g, limit):
        return [p["name"] for p in people(g) if p["age"] > limit]

    def younger(g, limit):
        return [p["name"] for p in people(g) if p["age"] < limit]

    def distinct(rows):
        out = []
        for row in rows:
            if row not in out:
                out.append(row)
        return out

    def known_or_followed(g):
        rows = [[g.nodes[e["target"]]["name"], "known"] for e in g.of_kind("KNOWS")]
        rows += [[g.nodes[e["target"]]["name"], "followed"] for e in g.of_kind("FOLLOWS")]
        return distinct(rows)

    BOTH = "MATCH (n:Person) WHERE n.age > 28 RETURN n.name AS x {} MATCH (n:Person) WHERE n.age < 35 RETURN n.name AS x"
    add("union-01", "spike", BOTH.format("UNION"), "supported", lambda g: distinct([[n] for n in older(g, 28) + younger(g, 35)]), ["x"], payload=P, note="Ann is in both parts and comes once")
    add("union-02", "spike", BOTH.format("UNION ALL"), "supported", lambda g: [[n] for n in older(g, 28) + younger(g, 35)], ["x"], payload=P, note="Ann comes twice")
    add("union-03", "spike", "RETURN 1 AS x UNION ALL RETURN 2 AS x UNION ALL RETURN 1 AS x", "supported", lambda g: [[1], [2], [1]], ["x"], payload=P, note="three parts")
    add("union-04", "spike", "MATCH (a)-[:KNOWS]->(b) RETURN b.name AS name, 'known' AS how UNION MATCH (a)-[:FOLLOWS]->(b) RETURN b.name AS name, 'followed' AS how", "supported", known_or_followed, ["name", "how"], payload=P, note="UNION also removes the duplicates within one part")
    add("union-05", "spike", "MATCH (n:Person) RETURN n.name AS x ORDER BY n.age DESC LIMIT 1 UNION ALL MATCH (n:Person) RETURN n.name AS x ORDER BY n.age LIMIT 1", "supported", lambda g: [[max(people(g), key=lambda p: p["age"])["name"]], [min(people(g), key=lambda p: p["age"])["name"]]], ["x"], payload=P, note="ORDER BY and LIMIT belong to their part")
    add("union-06", "spike", "MATCH ()-[r:KNOWS]->() RETURN count(r) AS c UNION ALL MATCH ()-[r:FOLLOWS]->() RETURN count(r) AS c", "supported", lambda g: [[len(g.of_kind("KNOWS"))], [len(g.of_kind("FOLLOWS"))]], ["c"], payload=P)
    add("union-07", "spike", "MATCH (n:Person) RETURN n.nick AS x UNION RETURN null AS x", "supported", lambda g: [[None]], ["x"], payload=P, note="null is a duplicate of null")

    # Maps, list comprehension, indexing, slicing, `+` on lists, and grouping expressions beside an aggregate
    # (2026-11-25). A map in a row is written `{"map": {...}}`, as nodes and edges are tagged.
    def mp(**entries):
        return {"map": entries}

    def knows_by_source(g):
        known = collections.defaultdict(list)
        for e in g.of_kind("KNOWS"):
            known[g.nodes[e["source"]]["name"]].append(g.nodes[e["target"]]["name"])
        return known

    def dee_paths(g):
        first = [e for e in g.edges if e["source"] == "dee" and e["kind"] in ("FOLLOWS", "KNOWS")]
        return [[g.nodes[b["target"]]["name"], [a["kind"], b["kind"]]] for a in first for b in g.of_kind("FOLLOWS", "KNOWS") if b["source"] == a["target"] and b["id"] != a["id"]]

    TEN = [10, 20, 30]
    FOUR = [1, 2, 3, 4]
    add("expr-01", "spike", "RETURN {a: [1, 2, {b: 'c'}], d: null} AS m", "supported", lambda g: [[mp(a=[1, 2, mp(b="c")], d=None)]], ["m"], payload=P)
    add("expr-02", "spike", "MATCH (n:Person) RETURN {name: n.name, adult: n.age >= 18} AS m", "supported", per_person(lambda p: [mp(name=p["name"], adult=p["age"] >= 18)]), ["m"], payload=P)
    add("expr-03", "spike", "MATCH (n:Person) WITH {who: n.name, ages: [n.age]} AS m RETURN m.who, m['ages'][0], m.nope", "supported", per_person(lambda p: [p["name"], p["age"], None]), ["m.who", "m['ages'][0]", "m.nope"], payload=P)
    add("expr-04", "spike", "RETURN [10, 20, 30][0] AS a, [10, 20, 30][-1] AS b, [10, 20, 30][3] AS c, [10, 20, 30][-4] AS d, [10, 20, 30][null] AS e", "supported", lambda g: [[TEN[0], TEN[-1], None, None, None]], ["a", "b", "c", "d", "e"], payload=P, note="outside the list is null")
    add("expr-05", "spike", "RETURN [1, 2, 3, 4][1..3] AS a, [1, 2, 3, 4][..-1] AS b, [1, 2, 3, 4][2..] AS c, [1, 2, 3, 4][3..1] AS d, [1, 2, 3, 4][-9..9] AS e, [1, 2][null..] AS f", "supported", lambda g: [[FOUR[1:3], FOUR[:-1], FOUR[2:], FOUR[3:1], FOUR[-9:9], None]], ["a", "b", "c", "d", "e", "f"], payload=P)
    add("expr-06", "spike", "RETURN [x IN [1, 2, 3, 4] WHERE x % 2 = 0 | x * x] AS a, [x IN [1, 2] | x + 1] AS b, [x IN [1, 2, 3] WHERE x > 1] AS c, [x IN [] | x] AS d, [x IN null | x] AS e", "supported", lambda g: [[[x * x for x in FOUR if x % 2 == 0], [2, 3], [2, 3], [], None]], ["a", "b", "c", "d", "e"], payload=P)
    add("expr-07", "spike", "MATCH (n:Person) WITH collect(n) AS people RETURN [p IN people WHERE p.age > 28 | p.name] AS older", "supported", lambda g: [[older(g, 28)]], ["older"], payload=P)
    add("expr-08", "spike", "MATCH (a:Person {name: 'Dee'})-[path:FOLLOWS|KNOWS*2]->(b) RETURN b.name, [r IN path | type(r)] AS types", "supported", dee_paths, ["b.name", "types"], payload=P, note="the edges of a path as a list")
    add("expr-09", "spike", "WITH [1, 2] AS l RETURN [x IN l | [y IN l WHERE y >= x | y * 10]] AS n", "supported", lambda g: [[[[y * 10 for y in [1, 2] if y >= x] for x in [1, 2]]]], ["n"], payload=P, note="a comprehension inside a comprehension reads the outer variable")
    add("expr-10", "spike", "RETURN [1] + [2, 3] AS a, [1] + 2 AS b, 0 + [1] AS c, [1] + null AS d", "supported", lambda g: [[[1, 2, 3], [1, 2], [0, 1], None]], ["a", "b", "c", "d"], payload=P)
    add("expr-11", "spike", "RETURN {a: 1} = {a: 1} AS a, {a: 1} = {a: 2} AS b, {a: null} = {a: null} AS c, {a: 1} = {b: 1} AS d", "supported", lambda g: [[True, False, None, False]], ["a", "b", "c", "d"], payload=P)
    add("expr-12", "spike", "MATCH (n:Person) RETURN DISTINCT {adult: n.age >= 18} AS m", "supported", lambda g: distinct([[mp(adult=p["age"] >= 18)] for p in people(g)]), ["m"], payload=P)
    add("expr-13", "spike", "MATCH (n:Person) RETURN n['na' + 'me'] AS name, n['nope'] AS none", "supported", per_person(lambda p: [p["name"], None]), ["name", "none"], payload=P)
    add("expr-14", "spike", "MATCH (a:Person)-[:KNOWS]->(b) RETURN a.name AS n, [a.name, count(*)] AS l", "supported", lambda g: [[k, [k, len(v)]] for k, v in knows_by_source(g).items()], ["n", "l"], payload=P, note="a grouping expression beside an aggregate keeps its value")
    add("expr-15", "spike", "MATCH (a:Person)-[:KNOWS]->(b) RETURN a.name AS n, a.name + ':' + toString(count(*)) AS s", "supported", lambda g: [[k, f"{k}:{len(v)}"] for k, v in knows_by_source(g).items()], ["n", "s"], payload=P)
    add("expr-16", "spike", "MATCH (a:Person)-[:KNOWS]->(b) RETURN a.name AS n, [x IN collect(b.name) WHERE x <> a.name | x + '<' + a.name] AS l", "supported", lambda g: [[k, [f"{x}<{k}" for x in v if x != k]] for k, v in knows_by_source(g).items()], ["n", "l"], payload=P, note="a comprehension over an aggregate that uses the grouping expression")
    return C


def build():
    graphs = {}
    lines = []
    for cid, graph, payload, query, status, ordered, columns, fn, error, note in cases():
        key = (graph, payload)
        if key not in graphs:
            graphs[key] = Graph(graph, payload)
        record = {"id": cid, "graph": graph}
        if payload:
            record["payload"] = payload
        record["query"] = query
        record["status"] = status
        if fn is not None:
            rows = fn(graphs[key])
            if not ordered:
                rows = sorted(rows, key=lambda r: json.dumps(r, sort_keys=True))
            record["ordered"] = ordered
            record["columns"] = columns
            record["rows"] = rows
        if error:
            record["error"] = error
        if note:
            record["note"] = note
        lines.append(json.dumps(record, ensure_ascii=False))
    return "\n".join(lines) + "\n"


def sums():
    out = []
    root = os.path.join(HERE, "graphs")
    for name in sorted(os.listdir(root)):
        if name.endswith(".json"):
            out.append(f"{hashlib.sha256(open(os.path.join(root, name), 'rb').read()).hexdigest()}  {name}")
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--summary", action="store_true")
    a = ap.parse_args()
    text = build()
    if a.write:
        open(OUT, "w").write(text)
        open(SUMS, "w").write(sums())
        print(f"wrote {OUT} and {SUMS}")
    if a.check:
        bad = []
        if not os.path.exists(OUT) or open(OUT).read() != text:
            bad.append("cases.jsonl differs from the oracle")
        if not os.path.exists(SUMS) or open(SUMS).read() != sums():
            bad.append("graphs/SHA256SUMS differs from the graph files")
        if bad:
            print("FAIL: " + "; ".join(bad))
            sys.exit(1)
        print("ok: cases.jsonl and graphs/SHA256SUMS match")
    if a.summary or not (a.write or a.check):
        c = collections.Counter()
        for line in text.splitlines():
            r = json.loads(line)
            c[(r["graph"], r["status"])] += 1
        for (graph, status), n in sorted(c.items()):
            print(f"{graph:16} {status:10} {n}")
        print(f"total {sum(c.values())}")


if __name__ == "__main__":
    main()
