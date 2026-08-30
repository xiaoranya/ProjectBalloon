#!/usr/bin/env python3
"""Compare legacy OpenAPI operations with the Axum router without extra packages."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# (legacy, rust, exact, missing-after-renames, rust-only). Every redesigned
# legacy operation is registered in docs/api/renamed-routes.yaml, so nothing
# is left unaccounted in `missing`.
EXPECTED = (147, 218, 108, 0, 110)


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


def renamed_operations() -> set[tuple[str, str]]:
    """Legacy routes deliberately redesigned per docs/api/renamed-routes.yaml."""
    renamed: set[tuple[str, str]] = set()
    for line in (ROOT / "docs/api/renamed-routes.yaml").read_text().splitlines():
        match = re.match(r"^\s*-\s*legacy:\s*(\S+)\s+(/api/\S+)\s*$", line)
        if match:
            renamed.add((match.group(1).upper(), normalize(match.group(2))))
    return renamed


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
    # Routes are registered inside each feature module's routes() function and
    # merged by the root router, so scan the whole API source tree.
    for source_file in sorted((ROOT / "apps/api/src").rglob("*.rs")):
        for call in route_calls(source_file.read_text()):
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
    renamed = renamed_operations()
    exact = legacy & rust
    missing = legacy - rust
    unmatched = renamed - missing
    remaining = missing - renamed
    extensions = rust - legacy
    actual = (len(legacy), len(rust), len(exact), len(remaining), len(extensions))
    print(
        "legacy={} rust={} exact={} missing-after-renames={} rust-only={} registered-renames={}".format(
            *actual, len(renamed)
        )
    )
    for method, path in sorted(remaining, key=lambda item: (item[1], item[0])):
        print(f"{method:6} {path}")
    if args.check:
        if unmatched:
            print(
                "renamed-routes.yaml lists operations that are not redesigned-and-missing: "
                f"{sorted(unmatched)}"
            )
            return 1
        if actual != EXPECTED:
            print(f"compatibility snapshot drifted: expected={EXPECTED} actual={actual}")
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
