# State Channel Design: Forced Settlement & Dispute Window

This document defines the dispute resolution mechanism, dispute window lifecycle, and forced settlement protocol in `contracts/state_channel`.

---

## 1. Context & Motivation

State channels allow high-frequency interactions (such as streaming medical telemetry, rapid examination micro-updates, or micropayments) between a patient and a provider without writing every transition to the Stellar ledger.

In normal operation, both parties conclude the channel via `cooperative_close`, which settles balances immediately with mutual digital signatures. However, if a counterparty stalls, disconnects, or refuses to cooperate, the protocol must provide a deterministic **forced settlement** path so that funds and clinical updates are **never locked indefinitely**.

---

## 2. Dispute Window & Protocol Lifecycle

```
[Open] 
  |
  | unilateral_close(closer) by patient OR provider
  v
[Disputed]  (challenge_end = now + dispute_window)
  |
  |--- During dispute window (now < challenge_end):
  |      * settle() is REJECTED with ChallengePeriodActive.
  |      * Counterparty may call submit_fraud_proof(higher_nonce, balance, sig)
  |        -> If nonce > current_nonce: overrides stale state.
  |        -> If nonce <= current_nonce: penalized with capacity deduction.
  |
  v (now >= challenge_end)
[Dispute Window Expired]
  |
  | settle() called by EITHER party (or any relayer)
  v
[Closed] -> Final balances and record updates committed to ledger.
```

### Who Can Start Forced Settlement?
- Either channel participant (`closer == channel.patient` or `closer == channel.provider`) can initiate forced settlement by calling `unilateral_close`.
- Third parties cannot initiate unilateral close (enforced via `CommonError::AccessDenied`).

### What is the Dispute Window?
- The dispute window defines the duration during which the non-closing counterparty can defend against a stale state submission.
- **Configurability**:
  - Global default: `86,400` seconds (1 day), configurable by admin via `set_dispute_window(admin, window_secs)`.
  - Per-channel override: Channels opened via `open_channel_with_timeout(patient, provider, capacity, dispute_window)` store a custom dispute window.
- During the dispute window, any invocation of `settle` is rejected with `CommonError::ChallengePeriodActive`.

### What Does a Fraud Proof Do During the Window?
- If the closing party submitted a stale state (e.g. earlier nonce), the honest counterparty calls `submit_fraud_proof` within the dispute window.
- If `nonce > channel.nonce`: the contract replaces the stale state with the submitted higher nonce and balance.
- If an invalid or older state (`nonce <= channel.nonce`) is submitted as a dispute, the submitter is assessed a fraud penalty (`FRD_PEN`) deducted from their stake.
- After `challenge_end`, calls to `submit_fraud_proof` are rejected with `CommonError::InvalidChannelState`.

### Guaranteed Liveness (Funds Never Locked Forever)
- Once `env.ledger().timestamp() >= challenge_end`, the channel is eligible for final settlement.
- Either participant (or an automated keeper) calls `settle(env, channel_id)`.
- The contract updates status to `ChannelStatus::Closed`, triggers settlement batch processing, and finalizes balances on-chain.
- **Result**: Even if a counterparty vanishes permanently, the channel unlocks and settles upon dispute window expiry.
