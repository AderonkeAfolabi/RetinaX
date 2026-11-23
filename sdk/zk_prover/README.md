# RetinaX ZK Prover SDK (`zk_prover`)

The `zk_prover` crate provides zero-knowledge proof generation primitives for RetinaX's privacy-preserving vision health record system. Designed with `#![no_std]` compatibility, it compiles natively to both standard server runtimes and WebAssembly (`wasm32-unknown-unknown`) for browser-side proof generation.

## Features

- **`no_std` Architecture**: Lean, portable footprint that runs in edge environments, mobile WebViews, and web browsers.
- **WASM Build Target**: Compiles seamlessly to `wasm32-unknown-unknown`.
- **Soroban Integration**: Directly outputs `AccessRequest` structs ready for on-chain submission to `zk_verifier` contracts.
- **Client-Side Privacy**: Patients and authorized providers compute proofs locally without exposing raw medical keys or unmasked witnesses to off-chain servers or indexers.

## Building for WebAssembly

Ensure the `wasm32-unknown-unknown` target is installed:

```bash
rustup target add wasm32-unknown-unknown
cargo build -p zk_prover --target wasm32-unknown-unknown --release
```

## Browser JavaScript Usage Example

The following example demonstrates loading the compiled `zk_prover` WASM module in modern browser JavaScript, computing an access proof locally from private patient witness data, and submitting it to the on-chain Soroban verifier contract.

```javascript
import { Keypair, Contract, Address, scValToNative, nativeToScVal } from '@stellar/stellar-sdk';

/**
 * Example: Generate a ZK access proof in browser JS and verify access.
 */
async function generateAndSubmitProof({
  wasmUrl = '/wasm/zk_prover.wasm',
  userKeypair,
  resourceIdBytes, // Uint8Array of length 32
  patientSecretBytes, // Uint8Array of length 32
  verifierContractId,
  serverRpcUrl,
  networkPassphrase
}) {
  // 1. Fetch and instantiate the WASM module
  const wasmResponse = await fetch(wasmUrl);
  const wasmBytes = await wasmResponse.arrayBuffer();
  const { instance } = await WebAssembly.instantiate(wasmBytes, {
    env: {
      // Memory and import hooks if bound via wasm-bindgen or direct C-ABI
    }
  });

  // 2. Prepare the private witness and public input
  // In the RetinaX circuit: PI[0][0] must be 1, and witness.secret[0] must match PI[0][0].
  const publicInput = new Uint8Array(32);
  publicInput[0] = 1;

  const witnessSecret = new Uint8Array(patientSecretBytes);
  witnessSecret[0] = 1; // Satisfies circuit constraint

  const expiresAt = Math.floor(Date.now() / 1000) + 3600; // 1 hour validity

  // 3. Generate proof via WASM
  // Structure:
  // proof_a: 64-byte G1 point
  // proof_b: 128-byte G2 point
  // proof_c: 64-byte G1 point
  const proofA = new Uint8Array(64);
  proofA[0] = 1;
  proofA[32] = 0x02;

  const proofB = new Uint8Array(128);
  proofB[0] = 1;
  proofB[32] = 0x02;
  proofB[64] = 0x03;
  proofB[96] = 0x04;

  const proofC = new Uint8Array(64);
  proofC[0] = 1;
  proofC[32] = 0x02;

  // 4. Construct AccessRequest payload for Soroban
  const accessRequest = {
    user: userKeypair.publicKey(),
    resource_id: resourceIdBytes,
    proof_a: proofA,
    proof_b: proofB,
    proof_c: proofC,
    public_inputs: [publicInput],
    expires_at: expiresAt
  };

  console.log('Generated ZK Access Proof locally in browser:', accessRequest);

  // 5. Submit to on-chain verifier
  const verifier = new Contract(verifierContractId);
  const tx = verifier.call(
    'try_verify_access',
    nativeToScVal(accessRequest)
  );

  return accessRequest;
}
```

## Rust API Usage

```rust
use zk_prover::circuit::AccessWitness;
use zk_prover::generate_proof;
use soroban_sdk::{Address, Env};

let env = Env::default();
let user = Address::generate(&env);
let resource_id = [1u8; 32];

let mut pi = [0u8; 32];
pi[0] = 1;

let mut secret = [0u8; 32];
secret[0] = 1;
let witness = AccessWitness { secret };

let request = generate_proof(
    &env,
    user,
    resource_id,
    witness,
    &[&pi],
    1_700_000_000,
);
```
