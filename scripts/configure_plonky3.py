#!/usr/bin/env python3
"""Point local dependencies at an existing pinned Plonky3 checkout; fetch nothing."""
import json
import pathlib
import re
import subprocess
import sys
import tomllib

EXPECTED = "0d803737b681774186cd283b1fd2d8e493655c82"
if len(sys.argv) != 2:
    raise SystemExit("usage: configure_plonky3.py /path/to/Plonky3")
checkout = pathlib.Path(sys.argv[1]).resolve()
revision = subprocess.check_output(
    ["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True
).strip()
if revision != EXPECTED:
    raise SystemExit(f"wrong Plonky3 revision: {revision}; need {EXPECTED}")
if subprocess.check_output(
    ["git", "-C", str(checkout), "status", "--porcelain"], text=True
).strip():
    raise SystemExit("Plonky3 checkout must be clean")
manifest = pathlib.Path(__file__).resolve().parents[1] / "Cargo.toml"
original = manifest.read_text()
text = original
for name, dependency in tomllib.loads(original)["dependencies"].items():
    if name.startswith("p3-") and isinstance(dependency, dict) and "path" in dependency:
        component = pathlib.Path(dependency["path"]).name
        target = checkout / component
        if not (target / "Cargo.toml").is_file():
            raise SystemExit(f"missing dependency {component}")
        pattern = rf'(?m)^({re.escape(name)}\s*=\s*\{{\s*path\s*=\s*)"(?:[^"\\]|\\.)*"'
        text, count = re.subn(pattern, lambda m: m[1] + json.dumps(str(target)), text)
        if count != 1:
            raise SystemExit(f"expected one path entry for {name}")
tomllib.loads(text)
manifest.write_text(text)
print(f"Configured Plonky3 {revision} at {checkout}")
