.DEFAULT_GOAL := build

build:
	cargo build --offline --release --target wasm32-unknown-unknown
	mkdir -p out
	cp target/wasm32-unknown-unknown/release/atrium_mailbox.wasm out/plugin.wasm

clean:
	cargo clean
	rm -rf out

.PHONY: build clean
