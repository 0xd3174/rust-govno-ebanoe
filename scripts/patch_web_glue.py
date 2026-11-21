#!/usr/bin/env python3
"""Patch wasm-bindgen output so the app and the imgui-sys-v1 provider share one memory.

The provider (an Emscripten build of cimgui) imports `env.memory`. wasm-bindgen does not
expose `--import-memory`, so this script performs two small, idempotent edits:

1. It converts the module's own memory into `(import "env" "memory" ...)` and re-exports
   it as `memory`, using a WAT round trip through `wasm-tools`.
2. It injects code into the generated JS that passes `globalThis.__imgui_shared_memory`
   (created by web/index.html) as `env.memory` before the module is instantiated.

Usage:
    python3 scripts/patch_web_glue.py web/pkg/dear_imgui_demo_bg.wasm \
        web/pkg/dear_imgui_demo.js

Requires `wasm-tools` on PATH (provided by `nix develop`).
"""

from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys
import tempfile

# The declared limits are wide enough for the provider (max 32768 pages). The
# memory object actually handed to the module is created in web/index.html.
MEMORY_IMPORT = '(import "env" "memory" (memory (;0;) 256 32768))'
MEMORY_EXPORT = '(export "memory" (memory 0))'
JS_MARKER = "const imports = __wbg_get_imports();"
JS_INJECT = """
    // Injected by scripts/patch_web_glue.py: share one linear memory with imgui-sys-v1.
    const __shared_mem = globalThis.__imgui_shared_memory
        || new WebAssembly.Memory({ initial: 2048, maximum: 4096 });
    if (!imports.env) imports.env = {};
    if (!imports.env.memory) imports.env.memory = __shared_mem;
"""


def fail(message: str) -> None:
    sys.exit(f"patch_web_glue: {message}")


def patch_wasm(wasm: pathlib.Path) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        wat = pathlib.Path(tmp) / "module.wat"
        patched = pathlib.Path(tmp) / "module.patched.wat"
        subprocess.run(["wasm-tools", "print", str(wasm), "-o", str(wat)], check=True)

        lines = wat.read_text().splitlines()
        if any(line.strip().startswith('(import "env" "memory"') for line in lines):
            print(f"{wasm}: already imports env.memory")
            return

        module_at = next(
            (i for i, line in enumerate(lines) if line.strip().startswith("(module")), None
        )
        memory_at = next(
            (i for i, line in enumerate(lines) if line.strip().startswith("(memory")), None
        )
        if module_at is None:
            fail(f"{wasm}: no (module ...) header in WAT")
        if memory_at is None:
            fail(f"{wasm}: no defined (memory ...) in WAT")

        del lines[memory_at]
        lines = [line for line in lines if line.strip() != MEMORY_EXPORT]
        lines[module_at + 1 : module_at + 1] = [f"  {MEMORY_IMPORT}", f"  {MEMORY_EXPORT}"]
        patched.write_text("\n".join(lines) + "\n")

        subprocess.run(["wasm-tools", "parse", str(patched), "-o", str(wasm)], check=True)
        print(f"{wasm}: patched to import env.memory")


def patch_js(js: pathlib.Path) -> None:
    code = js.read_text()
    if "__shared_mem" in code:
        print(f"{js}: already injects shared memory")
        return
    if JS_MARKER not in code:
        fail(
            f"{js}: marker {JS_MARKER!r} not found. "
            "The wasm-bindgen glue changed; inspect the file and update JS_MARKER."
        )
    js.write_text(code.replace(JS_MARKER, JS_MARKER + JS_INJECT))
    print(f"{js}: injected shared memory")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Make the wasm-bindgen module import the imgui-sys-v1 shared memory."
    )
    parser.add_argument("wasm", type=pathlib.Path, help="wasm-bindgen *_bg.wasm file")
    parser.add_argument("js", type=pathlib.Path, help="wasm-bindgen *.js glue file")
    args = parser.parse_args()

    for path in (args.wasm, args.js):
        if not path.is_file():
            fail(f"missing input file {path}")
    try:
        patch_wasm(args.wasm)
        patch_js(args.js)
    except subprocess.CalledProcessError as error:
        fail(f"wasm-tools failed: {error}")


if __name__ == "__main__":
    main()
