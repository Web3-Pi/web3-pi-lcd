#!/usr/bin/env python3
"""Fail CI/release builds if compromised CA handling returns to the lockfile."""
import pathlib
import tomllib

lock = tomllib.loads((pathlib.Path(__file__).resolve().parents[1] / "Cargo.lock").read_text())
minimum = {"webpki-roots": (1, 0, 10), "rustls-webpki": (0, 103, 16)}
for name, wanted in minimum.items():
    versions = [p["version"] for p in lock["package"] if p["name"] == name]
    if not versions or any("-" in v or tuple(map(int, v.split("."))) < wanted for v in versions):
        raise SystemExit(f"Unsafe or unreviewed {name}: {versions}")
    print(f"{name}: {', '.join(versions)}")
