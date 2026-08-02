.PHONY: build test lint fuzz clean deploy-testnet oracle-build oracle-test all

build:
	cargo build --target wasm32-unknown-unknown --release

test:
	cargo test

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

fuzz:
	cargo fuzz run fuzz_buy_ticket -- -max_total_time=300
	cargo fuzz run fuzz_finalize_raffle -- -max_total_time=300

deploy-testnet:
	./scripts/deploy-testnet.sh

clean:
	cargo clean

oracle-build:
	cd oracle && npm ci && npm run build

oracle-test:
	cd oracle && npm test

all: lint test build
