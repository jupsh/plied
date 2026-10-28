#!/usr/bin/env python3
"""Contract tests for the native companion; fixture vectors are not semantic evidence."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ARGS = None


class RelatedTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        subprocess.run([ARGS.plied, "-C", str(self.root), "init"], check=True, capture_output=True)
        config = self.root / "plied.toml"
        config.write_text(config.read_text().replace('include = ["plied/**/*.md"]',
            'include = ["notes/**/*.md"]\n').replace('sealed = []', 'sealed = ["private"]') +
            '\n[[sources]]\nname = "notes"\nglob = "notes/**/*.md"\nid = "^E[0-9]+$"\n')
        self.write("E001", "integer equation and its assumptions", "notes/allowed.md")
        self.write("E002", "PRIVATE_SENTINEL", "notes/private/secret.md")
        self.write("E003", "OUTSIDE_ALLOWLIST", "elsewhere/hidden.md")
        self.worker = self.root / "worker.py"
        self.worker.write_text('''import json,sys,pathlib
d=json.load(sys.stdin)
pathlib.Path(__file__).with_name("received.json").write_text(json.dumps(d))
print(json.dumps({"model":{"name":"fixture"},"query":[1,0],"documents":[[1,0] for _ in d["documents"]]}))
''')

    def write(self, rid, body, name):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"---\nid: {rid}\ntitle: {rid}\n---\n{body}\n")

    def run_related(self, *args, expected=0):
        proc = subprocess.run([ARGS.related, "-C", str(self.root), *args], capture_output=True, text=True)
        self.assertEqual(proc.returncode, expected, proc.stdout + proc.stderr)
        return json.loads(proc.stdout)

    def hybrid(self, *args, expected=0):
        return self.run_related("--embedder", sys.executable, "--embed-arg", str(self.worker),
                                *args, "unfamiliar paraphrase", expected=expected)

    def test_only_searchable_records_reach_the_embedder(self):
        out = self.hybrid()
        received = (self.root / "received.json").read_text()
        self.assertNotIn("PRIVATE_SENTINEL", received)
        self.assertNotIn("OUTSIDE_ALLOWLIST", received)
        self.assertEqual([p["id"] for p in out["candidates"]], ["E001"])
        passage = out["candidates"][0]
        self.assertEqual(passage["applicability"], "unassessed")
        self.assertEqual(len(passage["revision"]), 64)
        self.assertIn(passage["excerpt"], (self.root / passage["path"]).read_text())

    def test_policy_change_during_embedding_refuses_the_result(self):
        self.worker.write_text(self.worker.read_text().replace('print(json.dumps',
            'p=pathlib.Path(__file__).with_name("plied.toml"); p.write_text(p.read_text().replace("private", "allowed"))\nprint(json.dumps'))
        out = self.hybrid(expected=1)
        self.assertIn("policy changed", out["error"])
        self.assertNotIn("candidates", out)

    def test_changed_record_is_not_returned_with_an_old_excerpt(self):
        self.worker.write_text(self.worker.read_text().replace('print(json.dumps',
            'pathlib.Path(__file__).with_name("notes").joinpath("allowed.md").write_text("changed")\nprint(json.dumps'))
        out = self.hybrid()
        self.assertEqual(out["candidates"], [])
        self.assertTrue(out["changed_candidates_omitted"])

    def test_zero_limit_does_not_launch_an_embedder(self):
        out = self.run_related("-n", "0", "anything")
        self.assertEqual(out["candidates"], [])
        self.assertFalse((self.root / "received.json").exists())

    def test_generated_history_is_omitted_from_embedding_text(self):
        self.write("E001", "integer equation\n\n## History\nHISTORY_SENTINEL", "notes/allowed.md")
        self.hybrid()
        self.assertNotIn("HISTORY_SENTINEL", (self.root / "received.json").read_text())

    def test_retrieval_preserves_record_and_configuration_bytes(self):
        before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file() and p.suffix in [".md", ".toml"]}
        self.hybrid()
        self.assertEqual(before, {p: p.read_bytes() for p in before})

    def test_budget_is_explicit_instead_of_truncating_the_corpus(self):
        self.write("E004", "more words " * 300, "notes/more.md")
        out = self.run_related("--mode", "lexical", "--max-passages", "1", "words", expected=1)
        self.assertIn("budget", out["error"])

    def test_missing_seal_file_refuses_before_embedding(self):
        p = self.root / "plied.toml"
        p.write_text(p.read_text().replace('[access]', '[access]\nsealed_file = "missing"'))
        self.hybrid(expected=1)
        self.assertFalse((self.root / "received.json").exists())

    @unittest.skipIf(sys.platform == "win32", "symlink permissions vary on Windows")
    def test_allowlisted_symlink_to_sealed_file_does_not_escape_policy(self):
        (self.root / "notes/alias.md").symlink_to(self.root / "notes/private/secret.md")
        out = self.hybrid()
        self.assertNotIn("PRIVATE_SENTINEL", (self.root / "received.json").read_text())
        self.assertEqual([p["id"] for p in out["candidates"]], ["E001"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--plied", required=True)
    parser.add_argument("--related", required=True)
    ARGS, extra = parser.parse_known_args()
    unittest.main(argv=[sys.argv[0]] + extra)
