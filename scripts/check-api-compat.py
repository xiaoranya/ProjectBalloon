#!/usr/bin/env python3
"""Compare legacy OpenAPI operations with the Axum router without extra packages."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = (147, 205, 108, 39, 97)


def normalize(path: str) -> str:
    return re.sub(r"\{[^}]+}", "{}", path)


def legacy_operations() -> set[tuple[str, str]]:
    operations: set[tuple[str, str]] = set()
    path: str | None = None
    for line in (ROOT / "docs/api/openapi.yaml").read_text().splitlines():
        match = re.match(r"^  (/api/[^:]+):$", line)
        if match:
            path = match.group(1)
            continue
        match = re.match(r"^    (get|post|put|patch|delete):$", line)
        if path and match:
            operations.add((match.group(1).upper(), normalize(path)))
    return operations


def route_calls(source: str) -> list[str]:
    calls: list[str] = []
    cursor = 0
    while (start := source.find(".route(", cursor)) >= 0:
        index = start + len(".route(")
        depth = 1
        quoted = False
        escaped = False
        while index < len(source) and depth:
            character = source[index]
            if quoted:
                if escaped:
                    escaped = False
                elif character == "\\":
                    escaped = True
                elif character == '"':
                    quoted = False
            elif character == '"':
                quoted = True
            elif character == "(":
                depth += 1
            elif character == ")":
                depth -= 1
            index += 1
        calls.append(source[start:index])
        cursor = index
    return calls


def rust_operations() -> set[tuple[str, str]]:
    operations: set[tuple[str, str]] = set()
    source = (ROOT / "apps/api/src/lib.rs").read_text()
    for call in route_calls(source):
        path_match = re.search(r'\.route\(\s*"([^"]+)"', call)
        if not path_match:
            continue
        path = normalize(path_match.group(1))
        for method in re.findall(r"\b(get|post|put|patch|delete)\s*\(", call):
            operations.add((method.upper(), path))
    return operations


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail when the documented snapshot drifts")
    args = parser.parse_args()
    legacy = legacy_operations()
    rust = rust_operations()
    exact = legacy & rust
    missing = legacy - rust
    extensions = rust - legacy
    actual = (len(legacy), len(rust), len(exact), len(missing), len(extensions))
    print(
        "legacy={} rust={} exact={} redesigned-or-missing={} rust-only={}".format(*actual)
    )
    for method, path in sorted(missing, key=lambda item: (item[1], item[0])):
        print(f"{method:6} {path}")
    if args.check and actual != EXPECTED:
        print(f"compatibility snapshot drifted: expected={EXPECTED} actual={actual}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
