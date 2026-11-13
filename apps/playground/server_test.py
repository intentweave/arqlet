import importlib.util
import json
from pathlib import Path
import threading
import unittest
import urllib.request
import urllib.error
from http.server import ThreadingHTTPServer

ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location("demo_server",ROOT/"server.py")
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class StaticTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server=ThreadingHTTPServer(("127.0.0.1",0),module.Handler)
        cls.thread=threading.Thread(target=cls.server.serve_forever,daemon=True)
        cls.thread.start()
        cls.base=f"http://127.0.0.1:{cls.server.server_port}"
    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown();cls.server.server_close();cls.thread.join()
    def test_packages_are_served(self):
        with urllib.request.urlopen(f"{self.base}/packages/viewer/svg.js",timeout=10) as response:
            self.assertEqual(response.status,200)
            self.assertIn(b"export",response.read())
    def test_local_examples_are_served_only_from_their_folder(self):
        import tempfile
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/"examples";root.mkdir()
            (root/"tiny.json").write_text('{"nodes":[{"id":"a"}],"edges":[]}')
            (root/"tiny.meta.json").write_text('{"title":"Tiny","overview":true}')
            (Path(folder)/"secret.json").write_text("{}")
            module.LOCAL.update(root=root.resolve(),index=module.local_index(root.resolve()))
            try:
                with urllib.request.urlopen(f"{self.base}/local/index.json",timeout=10) as response:
                    index=json.loads(response.read())
                self.assertEqual([(e["name"],e["url"],e["nodes"],e["overview"]) for e in index],[("local-tiny","/local/tiny.json",1,True)])
                with urllib.request.urlopen(f"{self.base}/local/tiny.json",timeout=10) as response:
                    self.assertEqual(json.loads(response.read())["nodes"][0]["id"],"a")
                for path in ["/local/../secret.json","/local/%2e%2e/secret.json","/local/%2e%2e%2fsecret.json","/local/tiny.meta.json","/local/missing.json"]:
                    with self.subTest(path=path),self.assertRaises(urllib.error.HTTPError) as caught:
                        urllib.request.urlopen(f"{self.base}{path}",timeout=10)
                    self.assertEqual(caught.exception.code,404)
            finally:
                module.LOCAL.update(root=None,index=[])
    def test_packages_do_not_escape_their_root(self):
        for path in ["/packages/../apps/playground/server.py","/packages/%2e%2e/apps/playground/server.py","/packages/viewer/"]:
            with self.subTest(path=path),self.assertRaises(urllib.error.HTTPError) as caught:
                urllib.request.urlopen(f"{self.base}{path}",timeout=10)
            self.assertEqual(caught.exception.code,404)

class BackendTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server=ThreadingHTTPServer(("127.0.0.1",0),module.Handler)
        cls.thread=threading.Thread(target=cls.server.serve_forever,daemon=True)
        cls.thread.start()
        cls.url=f"http://127.0.0.1:{cls.server.server_port}/api/op"
    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown();cls.server.server_close();cls.thread.join()
    def request(self,payload,origin=None):
        headers={"Content-Type":"application/json"}
        if origin: headers["Origin"]=origin
        request=urllib.request.Request(self.url,json.dumps(payload).encode(),headers)
        return urllib.request.urlopen(request,timeout=20)
    def test_native_refinement(self):
        snapshot=json.loads((ROOT/"generated/snapshot.json").read_text())
        with self.request({"op":"refine","snapshot":snapshot,"expectedRevision":snapshot["revision"],"expectedLayoutVersion":snapshot.get("layoutVersion",0)}) as response:
            result=json.load(response)["snapshot"]
        self.assertEqual(result["graph"],snapshot["graph"])
        self.assertEqual(result["revision"],snapshot["revision"])
        self.assertEqual(result["layoutVersion"],snapshot.get("layoutVersion",0)+1)
    def test_threaded_refinement_matches_serial(self):
        snapshot=json.loads((ROOT/"generated/snapshot.json").read_text())
        base={"op":"refine","snapshot":snapshot,"expectedRevision":snapshot["revision"],"expectedLayoutVersion":snapshot.get("layoutVersion",0)}
        with self.request(base) as response:serial=json.load(response)
        with self.request({**base,"threads":4}) as response:threaded=json.load(response)
        self.assertEqual(threaded,serial)
    def test_edit_operation_runs_at_backend(self):
        snapshot=json.loads((ROOT/"generated/snapshot.json").read_text())
        source=snapshot["graph"]["nodes"][0]["id"]
        with self.request({"op":"add","snapshot":snapshot,"expectedRevision":snapshot["revision"],"sourceId":source,"label":"Backend node"}) as response:
            result=json.load(response)["snapshot"]
        self.assertEqual(result["revision"],snapshot["revision"]+1)
    def test_the_allowlist_covers_every_operation_the_dispatcher_knows(self):
        import re
        # The operations the typed protocol reads (ADR 0019): every op name its parser matches.
        source=(ROOT/"../../crates/runtime/src/protocol.rs").read_text()
        start=source.index("pub fn from_value(")
        body=source[start:source.index("pub fn to_value(",start)]
        known=set(re.findall(r'Some\("([A-Za-z]+)"\)',body))|set(re.findall(r'"([a-z][A-Za-z]+)"\s*(?:=>|\|)',body))
        known.add("refineProgressive")
        self.assertTrue({"move","intent","restore","import"}<=known,known)
        self.assertEqual(sorted(known-module.OPERATIONS),[],"operations the dispatcher knows but the backend refuses")
    def session(self,payload):
        request=urllib.request.Request(self.url.replace("/api/op","/api/session"),json.dumps(payload).encode(),{"Content-Type":"application/json"})
        try:
            with urllib.request.urlopen(request,timeout=30) as response:return response.status,json.load(response)
        except urllib.error.HTTPError as error:return error.code,json.load(error)
    def test_a_backend_session_keeps_the_view(self):
        graph=json.loads((ROOT/"../../examples/fixtures/small.json").read_text())
        status,opened=self.session({"session":"open"})
        self.assertEqual(status,200);key=opened["session"]
        self.assertEqual(len(key),32,"a random key, not the process's number")
        status,imported=self.session({"session":key,"request":{"op":"import","graph":graph,"limit":100}})
        self.assertEqual(status,200);self.assertIn("scene",imported);self.assertNotIn("snapshot",imported)
        # The change alone: no snapshot travels.
        status,toggled=self.session({"session":key,"request":{"op":"toggle","nodeId":"root"}})
        self.assertEqual((status,toggled["collapsedIds"]),(200,["root"]))
        status,stale=self.session({"session":key,"request":{"op":"toggle","nodeId":"root","expectedLayoutVersion":0}})
        self.assertEqual((status,stale["error"]),(400,"Stale view request"))
        status,exported=self.session({"session":key,"request":{"op":"export"}})
        self.assertEqual(exported["snapshot"]["collapsedIds"],["root"])
        status,refused=self.session({"session":key,"request":{"op":"rm -rf"}})
        self.assertEqual(status,400)
        self.assertEqual(self.session({"session":key,"close":True})[0],200)
        self.assertEqual(self.session({"session":key,"request":{"op":"export"}}),(404,{"ok":False,"error":"Unknown session"}))
        self.assertEqual(self.session({"session":"guessed","request":{"op":"export"}})[0],404)
    def test_cross_origin_is_rejected(self):
        with self.assertRaises(urllib.error.HTTPError) as error:self.request({"op":"refine"},"https://example.org")
        self.assertEqual(error.exception.code,403)
    def test_unsupported_operation(self):
        with self.assertRaises(urllib.error.HTTPError) as error:self.request({"op":"shell"})
        self.assertEqual(error.exception.code,400)

if __name__=='__main__':unittest.main()
