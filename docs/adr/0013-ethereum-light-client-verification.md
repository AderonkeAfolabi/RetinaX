# ADR-0013: Ethereum Light-Client Verification on Soroban

- **Status**: Proposed
- **Date**: 2026-11-18
- **Issue**: #469

## Context

RetinaX already uses a **permissioned relay + Merkle inclusion** model for cross-chain messages (ADR-0008, `contracts/cross_chain/`). Product and security discussions ask whether **Ethereum headers and receipts** can be verified **on Soroban** so clinical/bridge payloads depend less on trusting a small relayer set.

Constraints:

- Soroban has strict **CPU, memory, and storage** budgets per transaction.
- Full Ethereum consensus (all validators, full sync committees over time) is far larger than a typical Soroban invocation.
- We already invest in **ZK verification** patterns (ADR-0007); any path should reuse that mindset where possible.
- This is a **research spike**: choose a direction, not ship production mainnet bridging.

## Decision

**Recommended near-term path: hybrid optimistic relay with optional ZK acceleration; defer native Ethereum light-client consensus verification on Soroban.**

1. **Do not** implement a full Ethereum light client (sync committee + header chain) inside Soroban in the next implementation phase.
2. **Keep** ADR-0008’s relay + Merkle (or receipt trie) proofs as the operational baseline.
3. **Add** an explicit **challenge / fraud-proof window** (optimistic) for high-value messages, or require **ZK proofs of header/receipt validity** generated **off-chain** and verified with a small on-chain verifier (align with ADR-0007).
4. Revisit a **partial** light-client (e.g. only recent sync-committee aggregate checks) only after measured gas/CPU prototypes prove feasibility.

## Alternatives compared

### A — Native light client on Soroban

Verify Ethereum consensus artifacts (headers, sync committee signatures / aggregates) inside the contract.

| Pros | Cons |
|------|------|
| Strongest trust minimization on-chain | Very high CPU/memory; may exceed Soroban limits |
| No external prover dependency | Complex upgrades when Ethereum consensus changes |
| | Large stored state (committee keys, periods) |

**Budget (order-of-magnitude):** high — likely multi–CPU-limit risk per header batch; storage grows with committee updates. **Not recommended** as first ship.

### B — Optimistic verification

Relayers post headers/receipts; watchers challenge within a window; wrong claims can be slashed or messages frozen.

| Pros | Cons |
|------|------|
| Cheap happy path on Soroban | Latency (challenge window) |
| Fits existing relayer ops | Needs watchers + economic security |
| | Weaker than pure cryptographic finality on L1 |

**Budget:** low–medium on success path; dispute path needs careful metering. **Good interim** for non-instant clinical metadata if policy allows delay.

### C — ZK proofs of Ethereum state (recommended companion)

Off-chain prover shows “this receipt/header is valid under Ethereum consensus”; Soroban verifies a **succinct proof**.

| Pros | Cons |
|------|------|
| Small on-chain footprint | Prover infra cost and latency |
| Aligns with existing ZK direction | Circuit maintenance when Ethereum upgrades |
| Stronger than pure multisig relay | Prover liveness / decentralization |

**Budget:** verification cost bounded by the verifier circuit (design target: fit one Soroban tx); prove cost is off-chain (USD/ops, not ledger CPU). **Best long-term trust story** if ZK stack is already planned.

### D — Status quo only (permissioned relay + Merkle)

| Pros | Cons |
|------|------|
| Already designed (ADR-0008) | Trust in relayer set |
| Lowest implementation risk | Does not meet “verify Ethereum headers on Soroban” literally |

Acceptable **baseline**; this ADR layers **B and/or C** on top rather than replacing it overnight.

## Cost / budget estimate (spike-level)

| Approach | On-chain CPU/memory | Storage | Off-chain cost | Engineering effort |
|----------|---------------------|---------|----------------|--------------------|
| Native light client | Very high | High (committees) | Low | Very high |
| Optimistic | Low (post) / med (dispute) | Med (bonds, windows) | Med (watchers) | Medium |
| ZK verify | Low–med (fixed verifier) | Low | High (proving) | High |
| Relay + Merkle only | Low | Low | Low | Done / incremental |

Numbers are **relative**. A follow-up issue should run a **prototype measurement** (sample header batch or sample ZK verify) under `soroban` budget limits before any “light client” milestone is committed.

## Recommendation

1. **Document** that full Ethereum light-client consensus verification on Soroban is **out of scope** until prototypes clear resource limits.
2. **Product path:** strengthen ADR-0008 with **receipt/header commitment fields** and optional **optimistic challenge** for sensitive shares.
3. **Security path:** prefer **ZK proofs of inclusion/validity** verified on Soroban when the ZK verifier is production-ready (ADR-0007).
4. **Non-goal of this spike:** shipping a production Ethereum sync-committee client in `contracts/cross_chain/`.

## Consequences

- **Positive:** Clear decision avoids unbounded contract scope; aligns with ZK and relay work already in-repo.
- **Positive:** Clinical bridge can keep moving on relay + proofs without blocking on Ethereum consensus-in-WASM.
- **Negative:** Users still trust relays and/or provers until ZK/optimistic layers ship.
- **Negative:** “True” trustless Ethereum verification remains a later phase.

## Implementation notes / follow-ups

- No mandatory production contract change in this spike PR unless maintainers request a **stub** interface (e.g. `verify_eth_receipt_proof` placeholder that returns `Unsupported`).
- Follow-up issues (suggested):
  - Measure Soroban CPU for a minimal header-hash / Merkle receipt verification only (no consensus).
  - Spike ZK circuit for “receipt in block + block in ancestry” with on-chain verify hook.
  - Spec optimistic challenge window parameters for clinical_share messages.
- References: ADR-0007, ADR-0008, `contracts/cross_chain/`, `docs/api/cross_chain.md`, `docs/ZK_INTEGRATION.md`.
