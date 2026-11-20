CARGO ?= cargo
WASM_BINDGEN ?= wasm-bindgen
PYTHON ?= python3

PORT ?= 8088
WASM := target/wasm32-unknown-unknown/release/dear_imgui_demo.wasm

.PHONY: native web serve

native:
	$(CARGO) run

web: $(WASM)
	$(WASM_BINDGEN) --target web --out-dir web/pkg --out-name dear_imgui_demo $(WASM)

$(WASM): Cargo.toml src/lib.rs
	$(CARGO) build --lib --release --target wasm32-unknown-unknown --features web

serve:
	$(PYTHON) -m http.server $(PORT) -d web
