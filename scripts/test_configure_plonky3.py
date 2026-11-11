"""Test optional local Plonky3 configuration without modifying the repository."""

import runpy
import shutil
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
REVISION = "0d803737b681774186cd283b1fd2d8e493655c82"
COMPONENTS = ("field", "goldilocks", "matrix", "maybe-rayon", "symmetric", "commit", "merkle-tree")


class ConfigurePlonky3Tests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / "scripts").mkdir()
        self.script = self.root / "scripts/configure_plonky3.py"
        shutil.copy(ROOT / "scripts/configure_plonky3.py", self.script)
        shutil.copy(ROOT / "Cargo.toml", self.root / "Cargo.toml")
        self.checkout = self.root / "Plonky3"
        for name in COMPONENTS:
            component = self.checkout / name
            component.mkdir(parents=True)
            (component / "Cargo.toml").write_text("")

    def configure(self, revision=REVISION, status=""):
        with mock.patch.object(sys, "argv", [str(self.script), str(self.checkout)]), \
             mock.patch("subprocess.check_output", side_effect=[revision + "\n", status]):
            runpy.run_path(str(self.script), run_name="__main__")

    def test_git_dependencies_become_local_and_reconfiguration_is_idempotent(self):
        self.configure()
        manifest = self.root / "Cargo.toml"
        first = manifest.read_text()
        deps = tomllib.loads(first)["dependencies"]
        for name in COMPONENTS:
            self.assertEqual(deps["p3-" + name]["path"], str(self.checkout / name))
        self.assertEqual(deps["p3-maybe-rayon"]["features"], ["parallel"])
        self.configure()
        self.assertEqual(manifest.read_text(), first)

    def test_wrong_revision_and_dirty_checkout_leave_manifest_unchanged(self):
        manifest = self.root / "Cargo.toml"
        before = manifest.read_text()
        for revision, status in [("wrong", ""), (REVISION, " M field/src/lib.rs")]:
            with self.assertRaises(SystemExit):
                self.configure(revision, status)
            self.assertEqual(manifest.read_text(), before)


if __name__ == "__main__":
    unittest.main()
