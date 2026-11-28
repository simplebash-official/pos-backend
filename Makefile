.PHONY: check check-atlas ci test

# Pre-commit gate: clippy + the whole suite against Atlas on a throwaway database.
check-atlas:
	bash scripts/check-atlas.sh

check:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo check -p pos-calc-wasm --target wasm32-unknown-unknown
	bash scripts/test-docker-mongo.sh

ci: check
