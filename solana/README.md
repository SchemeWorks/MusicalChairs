# Native SOL host model

This directory contains the accepted native SOL economic reference model and
local provider/resource probes. It is host-side Rust proof code; it does not
claim SVM execution, deployed program behavior, wallet authority, signing,
RPC/oracle liveness, or custody of funds.

From the repository root, run the economic workspace checks with the pinned host toolchain:

```bash
cargo +1.92.0 fmt --manifest-path solana/Cargo.toml --all -- --check
cargo +1.92.0 test --manifest-path solana/Cargo.toml --workspace --locked
cargo +1.92.0 clippy --manifest-path solana/Cargo.toml --workspace --all-targets --locked -- -D warnings
```

Run the standalone provider probe checks:

```bash
cd solana/probes/provider-resources
cargo fmt --manifest-path Cargo.toml --all -- --check
cargo test --manifest-path Cargo.toml --locked
cargo clippy --manifest-path Cargo.toml --all-targets --locked -- -D warnings
```

The pinned host toolchain is Rust 1.92.0. Deployment, live transaction
signing/broadcast, key configuration, and funds remain outside this scope.
