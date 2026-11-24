# ZK Prover SDK (`sdk/zk_prover`)

The `zk_prover` SDK provides client-side zero-knowledge proof generation designed for browser and edge environments using WebAssembly.

## Overview

- **Crate**: `sdk/zk_prover`
- **Target**: `wasm32-unknown-unknown`
- **Specification**: Enables generating mock Groth16 and BN254 non-degenerate proofs matching `zk_verifier` Soroban contract verification logic.

## Compiling to WASM

```bash
cargo build -p zk_prover --target wasm32-unknown-unknown --release
```

## Browser JavaScript Integration

See the complete guide and code samples in [`sdk/zk_prover/README.md`](../../sdk/zk_prover/README.md).
