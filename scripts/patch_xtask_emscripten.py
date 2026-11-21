#!/usr/bin/env python3
"""Drop the `allocate` runtime export that Emscripten >= 4 removed.

The pinned 0.22.0 xtask still requests it in EXPORTED_RUNTIME_METHODS and aborts
during JS finalization. The resulting wasm is otherwise complete.
"""

import pathlib
import sys

path = pathlib.Path(sys.argv[1])
source = path.read_text()
patched = source.replace('\\"allocate\\",', "")
if patched == source:
    print(f"{path}: no unsupported export entry to patch")
else:
    path.write_text(patched)
    print(f"{path}: dropped unsupported Emscripten `allocate` export")
