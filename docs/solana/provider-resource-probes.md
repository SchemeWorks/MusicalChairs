# Native Solana provider/resource probe

This is a host-side compatibility and packing probe for SOL-30. It exercises the pinned
ORAO Callback SDK through its real Anchor types, derives the registered-client and request
PDAs, serializes the request state, and builds unsigned legacy/v0 packets. It does not
contain a native production program, an IDL, a signer, an RPC endpoint, or a transaction
submission path.

The probe uses Anchor `0.32.1` and the supplied public `orao-solana-vrf-cb 0.4.0` archive
with `default-features = false, features = ["cpi"]`. The archive is vendored unchanged at
`solana/probes/provider-resources/vendor/orao-solana-vrf-cb`. Its archive SHA-256 is
`a036bfe6b101f9583797e4923d8c8fe838604a735ed843b4959b7238f649d9a7`. The sorted full-file
SHA-256 manifest has 57 entries and digest
`8855e1e63707fde1e5dc9ea4dc0adaeb54994314f9b5d03c586943f2c703346c`; reproduce it with:

```sh
find solana/probes/provider-resources/vendor/orao-solana-vrf-cb -type f -print0 \
  | sort -z | xargs -0 shasum -a 256
```

The checked source is the public crate archive; no SDK file was patched. Cargo resolution is
locked in `solana/probes/provider-resources/Cargo.lock` and uses Rust `1.92.0`.

The generated CPI account type produces seven request accounts in the SDK order: payer
signer/writable, registered state signer, client writable, network state writable, treasury
writable, request writable, and system program. The probe calls Anchor's generated
`ToAccountMetas` implementation and compares it with this order. `typecheck_request_cpi`
also compiles the exact ORAO `cpi::request` call with the registered state PDA as the signer
account; no CPI is invoked in the host test.

The ORAO SDK's source constants give these account allocation sizes without inventing field
offsets:

| account | allocated bytes | actual serialized bytes in this no-callback fixture |
| --- | ---: | ---: |
| registered `Client` | 123 | 114 |
| pending `RequestAccount` | 759 | 88 |
| fulfilled `RequestAccount` | 147 | 147 |

The difference for `Client` and pending `RequestAccount` is reserved account space for the
SDK's maximum response/callback shape; the serialized fixture contains no callback and no
responses. The probe records the SDK's `Rent::default()` result under explicit
`sdk_default_*` fields; that host SDK fixture uses a 6,960 lamport-per-byte rent coefficient.
It also records a separately observed October 12, 2026 public rent snapshot fit,
`5080*(bytes+128)`, under `observed_oct12_*` fields for operating estimates. Those values are
copied from the saved receipt at
`docs/coordination/evidence/2026-10-12-native-solana-costs.json`; this CLI performs no RPC
request and makes no current-price claim. The evidence JSON carries the 300,000-lamport ORAO
request-fee input only; it contains no invented provider-creation fee and does not read a user
balance or claim a receipt.

The fulfilled ORAO result is carried through the SDK as a full 64-byte randomness value. The
probe records `provider_randomness_output_bytes: 64` and validates the decoded value without
truncating it to 32 bytes; any downstream mapping remains outside this evidence-only crate.

Fulfillment validation also checks the decoded request client, seed, and bump against the
canonical SDK binding. Pending requests, wrong owners, malformed bytes, wrong clients, wrong
seeds, wrong bumps, and forged request fields are rejected. The request `slot` is retained as
an SDK-decoded field but is intentionally not compared: this fixture has no authoritative
creation receipt or slot input.

The packet probe uses Solana SDK `2.3.1` legacy and v0 serialization, default signatures
(`Signature::default()`), a fixed fee payer distinct from the fixed accounts, eight fixed
program accounts, one Backer PDA per row, and one snapshot page per 16 snapshot rows. The
16-row page size is a packing-fixture assumption for this probe, not an approved native page
capacity. It
serializes and deserializes the actual `VersionedTransaction` before checking that every
signature is the default zero value, and a test with a crafted nonzero signature is rejected.
It reports packet bytes and loaded account counts for chunk capacities 1/4/8/16/32 and snapshot
sizes 0/1/16/17/100/256. An ALT v0
variant is measured separately and contains only the row/page keys in the synthetic lookup
account. These are representative packing measurements, not the Musical Chairs instruction
schema or compute-unit estimates.

The current proof limits recorded by the probe are 1,232 serialized packet bytes and 64
loaded account locks. In the representative 256-row snapshot, legacy capacity 8 is 1,229
bytes with 34 loaded accounts; capacities 16 and 32 exceed the packet byte limit (1,493 and
2,021 bytes). The v0 ALT variant is 507/513/521/537/569 bytes for capacities 1/4/8/16/32,
with 27/30/34/42/58 loaded accounts. ALT compression reduces bytes but does not remove the
loaded-account lock bound. These figures do not establish CU limits, SBF behavior, validator
latency, finality, rent receipts, provider liveness, or production readiness.

Run the probe and tests from the standalone workspace root:

```sh
cd solana/probes/provider-resources
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt -- --check
cargo run --quiet
```

The latter prints the reproducible JSON snapshot at
`docs/solana/evidence/provider-resource-probes.json`. `anchor`, `solana`,
`cargo-build-sbf`, and `surfpool` were absent on the host during this pass, so SBF/validator/
Surfpool evidence remains explicitly unsupported. No deployment, wallet interaction,
signature, broadcast, funds movement, or live provider read was attempted.

The standalone `.cargo/config.toml` registers the upstream `anchor-debug` cfg value for this
consumer, so the untouched vendored ORAO derive macros remain byte-for-byte unchanged while
the all-targets `-D warnings` Clippy check passes. The local probe uses the pinned
`solana-system-interface 2.0.0` program identifier conversion rather than the deprecated
`solana_sdk::system_program` module.
