CARGO ?= cargo
WASM_BINDGEN ?= wasm-bindgen
PYTHON ?= python3

PORT ?= 8088
WASM := target/wasm32-unknown-unknown/release/dear_imgui_demo.wasm
PKG := web/pkg

.PHONY: native web provider serve check

native:
	$(CARGO) run

web: $(WASM)
	@test -f web/provider/imgui-sys-v1-wrapper.js || { echo "missing provider: run 'make provider' first" >&2; exit 1; }
	$(WASM_BINDGEN) --target web --out-dir $(PKG) --out-name dear_imgui_demo $(WASM)
	$(PYTHON) scripts/patch_web_glue.py $(PKG)/dear_imgui_demo_bg.wasm $(PKG)/dear_imgui_demo.js

$(WASM): Cargo.toml src/lib.rs
	$(CARGO) build --lib --release --target wasm32-unknown-unknown --features web

provider:
	./scripts/build-provider.sh

check:
	$(PYTHON) scripts/check_web_bundle.py

serve:
	$(PYTHON) -m http.server $(PORT) -d web
