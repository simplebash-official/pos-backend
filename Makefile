.PHONY: check check-atlas ci test

# Pre-commit gate: clippy + the whole suite against Atlas on a throwaway database.
check-atlas:
	bash scripts/check-atlas.sh

check:
	cargo fmt --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test

ci: check
