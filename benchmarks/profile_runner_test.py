"""Tests for the pure parts of profile_runner.py. Run: python3 -m unittest benchmarks/profile_runner_test.py"""
import hashlib
import json
import os
import tempfile
import subprocess
import sys
from pathlib import Path
import unittest
from unittest.mock import patch

import profile_runner as pr


class PeakBytesTest(unittest.TestCase):
    def test_macos_time_reports_bytes(self):
        stderr = "real 0.01\n  1234567  maximum resident set size\n"
        self.assertEqual(pr.peak_bytes(stderr), 1234567)

    def test_gnu_time_reports_kilobytes(self):
        stderr = "Maximum resident set size (kbytes): 2048\n"
        self.assertEqual(pr.peak_bytes(stderr), 2048 * 1024)

    def test_missing_report_is_none(self):
        self.assertIsNone(pr.peak_bytes("no report here"))


class DrawingTest(unittest.TestCase):
    def snapshot(self):
        return {
            "graph": {
                "nodes": [
                    {"id": "box", "parentId": None},
                    {"id": "leaf", "parentId": "box"},
                    {"id": "other", "parentId": None},
                ],
                "edges": [
                    {"id": "e1", "source": "leaf", "target": "other"},
                    {"id": "e2", "source": "leaf", "target": None},
                    {"id": "e3", "source": "leaf", "target": "other"},
                ],
            },
            "boxes": {
                "box": {"x": 100, "y": 50, "width": 300, "height": 200},
                "leaf": {"x": 10, "y": 20, "width": 180, "height": 54},
                "other": {"x": 600, "y": 0, "width": 180, "height": 54},
            },
            "routes": [
                {"id": "e1", "points": [{"x": 1, "y": 1}, {"x": 2, "y": 2}]},
                {"id": "e3", "points": [{"x": 3, "y": 3}], "bundle_of": "e1"},
            ],
        }

    def test_nodes_are_absolute_and_containers_flagged(self):
        drawing = pr.drawing_of(self.snapshot())
        nodes = {n["id"]: n for n in drawing["nodes"]}
        self.assertTrue(nodes["box"]["container"])
        self.assertFalse(nodes["leaf"]["container"])
        self.assertEqual((nodes["leaf"]["x"], nodes["leaf"]["y"]), (110, 70))

    def test_unrouted_and_bundled_edges_are_left_out(self):
        drawing = pr.drawing_of(self.snapshot())
        self.assertEqual([e["id"] for e in drawing["edges"]], ["e1"])


class RunnerSafetyTest(unittest.TestCase):
    def test_snapshot_hash_detects_geometry_despite_same_metrics(self):
        self.assertNotEqual(pr.snapshot_hash({"x": 1}), pr.snapshot_hash({"x": 2}))
        self.assertEqual(pr.snapshot_hash({"a": 1,"b": 2}), pr.snapshot_hash({"b": 2,"a": 1}))

    def test_missing_route_is_not_silently_ignored(self):
        snapshot = DrawingTest().snapshot()
        snapshot["routes"] = snapshot["routes"][1:]
        with self.assertRaisesRegex(RuntimeError, "Missing"):
            pr.drawing_of(snapshot)

    def test_timeout_stops_grandchild(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "survived"
            child = "import time,pathlib; time.sleep(0.6); pathlib.Path(%r).write_text('alive')" % str(marker)
            parent = "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c',%r]); time.sleep(10)" % child
            with self.assertRaises(subprocess.TimeoutExpired):
                pr.run_process([sys.executable, "-c", parent], "", 0.2)
            import time
            time.sleep(0.7)
            self.assertFalse(marker.exists())


class BudgetTest(unittest.TestCase):
    def test_refine_gets_remaining_budget_and_reports_interruption(self):
        snapshot = {"revision": 0, "layoutVersion": 0, "optimization": {"interrupted": True}}
        requests = []
        def call(request, timeout):
            requests.append(request)
            return {"ok": True, "snapshot": snapshot}, 20, None
        with patch.object(pr, "call", side_effect=call), patch.object(pr, "measure", return_value=({}, 0, None)):
            result = pr.run_once({}, 10, {"options": {}, "refine": True, "budgetMs": 45}, 1)
        self.assertEqual(requests[-1]["budgetMs"], 5)
        self.assertTrue(result["interrupted"])


class FrozenInputTest(unittest.TestCase):
    def test_sha256_matches_hashlib_and_detects_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "graph.json")
            content = b'{"nodes": [], "edges": []}'
            with open(path, "wb") as f:
                f.write(content)
            self.assertEqual(pr.sha256(path), hashlib.sha256(content).hexdigest())
            with open(path, "ab") as f:
                f.write(b"\n")
            self.assertNotEqual(pr.sha256(path), hashlib.sha256(content).hexdigest())


if __name__ == "__main__":
    unittest.main()
