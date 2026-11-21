#!/usr/bin/env python3
"""Drop the `allocate` runtime export that Emscripten >= 4 removed.

The pinned 0.22.0 xtask still requests `allocate` in EXPORTED_RUNTIME_METHODS and aborts
during Emscripten JS finalization even though the wasm itself is complete. This script
removes that single entry from the upstream checkout used to build the provider.

Usage:
    python3 scripts/patch_xtask_emscripten.py .provider-src/dear-imgui-rs/xtask/src/main.rs

`--check` reports whether the patch is needed and does not write anything.
Exit codes: 0 = compatible or patched, 1 = --check found the incompatible entry.
"""

from __future__ import annotations

import argparse
import pathlib
import sys


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Remove the Emscripten `allocate` export from the pinned dear-imgui-rs xtask."
    )
    parser.add_argument("xtask_main", type=pathlib.Path, help="path to xtask/src/main.rs")
    parser.add_argument(
        "--check",
        action="store_true",
        help="do not modify the file; exit 1 if the patch is still needed",
    )
    args = parser.parse_args()

    path = args.xtask_main
    if not path.is_file():
        print(f"patch_xtask_emscripten: missing file {path}", file=sys.stderr)
        return 2

    source = path.read_text()
    patched = source.replace('\\"allocate\\",', "")
    if patched == source:
        print(f"{path}: compatible (no `allocate` export to remove)")
        return 0
    if args.check:
        print(f"{path}: still requests the removed `allocate` export")
        return 1
    path.write_text(patched)
    print(f"{path}: dropped unsupported Emscripten `allocate` export")
    return 0


if __name__ == "__main__":
    sys.exit(main())
