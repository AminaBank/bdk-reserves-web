.PHONY: build serve test test-unit test-e2e fmt clippy clean

PORT ?= 8087

# Build the static site into dist/.
build:
	./build.sh

# Serve dist/ locally. Any static file server will do, since dist/ is just
# files. The wasm module needs http rather than file://.
serve: build
	@echo "Serving http://localhost:$(PORT)"
	@cd dist && python3 -m http.server $(PORT)

test: test-unit test-e2e

# Verification logic, on the host, no network.
test-unit:
	cargo test

# The built wasm module driven through its real JS API against a mock Esplora.
test-e2e: build
	@rm -rf target/e2e
	@$$(command -v wasm-bindgen || echo .tools/bin/wasm-bindgen) \
		--target nodejs --no-typescript --out-dir target/e2e \
		target/wasm32-unknown-unknown/release/bdk_reserves_web.wasm
	node tests/e2e.cjs

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-targets -- -D warnings

clean:
	cargo clean
	rm -rf dist .tools
