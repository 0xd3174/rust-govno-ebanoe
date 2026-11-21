#!/usr/bin/env python3
"""Check that the browser bundle is complete and wired for the imgui-sys-v1 provider.

Run after `make web` (and `make provider` the first time):

    python3 scripts/check_web_bundle.py

It verifies the generated files, the import map, the shared-memory injection, and that
the wasm module imports `env.memory`. Exit code 0 means all checks passed.
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
INDEX = ROOT / "web" / "index.html"
GLUE_JS = ROOT / "web" / "pkg" / "dear_imgui_demo.js"
GLUE_WASM = ROOT / "web" / "pkg" / "dear_imgui_demo_bg.wasm"
PROVIDER = ROOT / "web" / "provider"
PROVIDER_FILES = [
    PROVIDER / "imgui-sys-v1.js",
    PROVIDER / "imgui-sys-v1.wasm",
    PROVIDER / "imgui-sys-v1-wrapper.js",
]

failures: list[str] = []


def check(condition: bool, label: str, detail: str = "") -> None:
    if condition:
        print(f"ok   {label}")
        return
    message = f"FAIL {label}" + (f" ({detail})" if detail else "")
    print(message)
    failures.append(message)


def main() -> int:
    check(INDEX.is_file(), "web/index.html exists", "run `make web`")
    check(GLUE_JS.is_file(), "wasm-bindgen JS glue exists", "run `make web`")
    check(GLUE_WASM.is_file(), "wasm-bindgen wasm exists", "run `make web`")
    for path in PROVIDER_FILES:
        check(path.is_file(), f"{path.relative_to(ROOT)} exists", "run `make provider`")

    if INDEX.is_file():
        html = INDEX.read_text()
        check('type="importmap"' in html, "index.html has an import map")
        check('"imgui-sys-v1"' in html, "import map maps imgui-sys-v1")
        check("__imgui_shared_memory" in html, "index.html creates the shared memory")

    if GLUE_JS.is_file():
        js = GLUE_JS.read_text()
        check('from "imgui-sys-v1"' in js, "glue imports the provider module")
        check("__shared_mem" in js, "glue injects the shared memory", "run `make web`")
        check(
            "const imports = __wbg_get_imports();" in js,
            "glue still has the expected init marker",
            "wasm-bindgen changed its output; update scripts/patch_web_glue.py",
        )

    if GLUE_WASM.is_file():
        with tempfile.TemporaryDirectory() as tmp:
            wat = pathlib.Path(tmp) / "module.wat"
            try:
                subprocess.run(
                    ["wasm-tools", "print", str(GLUE_WASM), "-o", str(wat)],
                    check=True,
                    capture_output=True,
                )
            except FileNotFoundError:
                check(False, "wasm-tools is available", "run this inside `nix develop`")
            except subprocess.CalledProcessError as error:
                check(False, "the wasm module can be printed", str(error))
            else:
                text = wat.read_text()
                check(
                    '(import "env" "memory"' in text,
                    "wasm imports env.memory",
                    "run scripts/patch_web_glue.py",
                )

    if failures:
        print(f"\n{len(failures)} check(s) failed.")
        return 1
    print("\nAll checks passed. Serve the directory and open the page over HTTP.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
