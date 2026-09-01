.PHONY: fmt test lint verify

fmt:
	cargo fmt --check

test:
	cargo test --all-targets --all-features --locked

lint:
	cargo clippy --all-targets --all-features --locked -- -D warnings

verify: fmt test lint
