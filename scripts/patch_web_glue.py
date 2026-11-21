#!/usr/bin/env python3
"""Patch wasm-bindgen output so the main module and imgui-sys-v1 share one memory.

The provider (Emscripten build of cimgui) imports `env.memory`. wasm-bindgen does not
expose `--import-memory`, so the generated module is converted with a WAT round trip:

* its defined memory becomes `(import "env" "memory" ...)`, re-exported as `memory`;
* the generated init code passes `globalThis.__imgui_shared_memory` as `env.memory`.

Run with: patch_web_glue.py <module_bg.wasm> <module.js>
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

MEMORY_IMPORT = '(import "env" "memory" (memory (;0;) 256 4096))'
MEMORY_EXPORT = '(export "memory" (memory 0))'
JS_MARKER = "const imports = __wbg_get_imports();"
JS_INJECT = """
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
        if any(MEMORY_IMPORT.split(" 256")[0] in line for line in lines):
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
        fail(f"{js}: marker {JS_MARKER!r} not found")
    js.write_text(code.replace(JS_MARKER, JS_MARKER + JS_INJECT))
    print(f"{js}: injected shared memory")


def main() -> None:
    if len(sys.argv) != 3:
        fail("usage: patch_web_glue.py <module_bg.wasm> <module.js>")
    wasm = pathlib.Path(sys.argv[1])
    js = pathlib.Path(sys.argv[2])
    for path in (wasm, js):
        if not path.is_file():
            fail(f"missing input file {path}")
    patch_wasm(wasm)
    patch_js(js)


if __name__ == "__main__":
    main()
