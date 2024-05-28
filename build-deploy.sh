cargo build-bpf &&
solana program deploy --program-id keypair2.json -v --max-len 1000000 ./target/deploy/solana_escrow.so
