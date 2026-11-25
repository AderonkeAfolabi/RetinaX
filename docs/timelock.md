# Timelock Contract Guide

This guide details the architecture, transaction lifecycle, authorization model, error conditions, and API reference for the RetinaX Timelock Contract located in `contracts/timelock/src/lib.rs`.

---

## 1. Overview & Governance Fit

The Timelock contract acts as a critical security buffer in the RetinaX decentralized governance architecture. When the DAO Governor passes a proposal, the resulting operations are not executed immediately on-chain. Instead, they are queued into the Timelock.

```
+------------------+         +--------------------+         +-----------------------+
|  Governor DAO    | ------> |  Timelock Contract | ------> |  Target Smart         |
|  Proposal Passed | (queue) |  Mandatory Delay   | (exec)  |  Contracts (e.g.      |
+------------------+         +--------------------+         |  Treasury, Upgrades)  |
                                      |                     +-----------------------+
                                      | (veto/cancel)
                                      v
                             +--------------------+
                             | Admin / Guardian   |
                             +--------------------+
```

### Why a Timelock is Required
1. **Notice Period**: Network participants, node operators, and users have time to review scheduled operations before they take effect.
2. **Guardian Intervention**: A designated security guardian or administrator can veto or cancel unexpected, malicious, or erroneous actions before execution.
3. **Emergency Prioritization**: In verified security emergencies, the administrator can mark a queued transaction with `priority = true` to allow execution before the standard delay expires.

---

## 2. Transaction Lifecycle

A transaction queued in the Timelock transitions through distinct states:

```
                  +-------------------+
                  |   Uninitialized   |
                  +-------------------+
                            |  initialize()
                            v
                  +-------------------+
                  |    Ready/Idle     |
                  +-------------------+
                            |  queue_tx() [delay >= min_delay]
                            v
                  +-------------------+
                  |      Queued       | <-----------------------------+
                  +-------------------+                               |
                     /      |      \                                  |
    prioritize_tx() /       |       \ cancel_tx() / veto_tx()         |
                   v        |        v                                |
     +---------------+      |      +-------------------+              |
     |   Prioritized |      |      |     Cancelled     | (removed)    |
     +---------------+      |      +-------------------+              |
            |               |                                         |
            |               | now >= execute_after                    |
            \               /                                         |
             v             v                                          |
         +--------------------+                                       |
         |    execute_tx()    | --------------------------------------+
         +--------------------+ (removed from queue, executed)
```

1. **Queueing (`queue_tx`)**:
   - The administrator queues an operation identifier (`BytesN<32>`), supplying an execution delay.
   - The contract verifies that `delay >= min_delay`.
   - The execution unlock timestamp is computed as `execute_after = env.ledger().timestamp() + delay`.
   - The transaction struct `Tx { id, execute_after, priority: false }` is appended to the queue, and a `TX_QUED` event is published.
2. **Timelock Delay**:
   - The transaction remains queued while `now < execute_after`.
   - If an unauthorized party or admin attempts execution before `execute_after`, the call fails with `TimelockError::TooEarly`.
3. **Emergency Prioritization (`prioritize_tx`)**:
   - The administrator can flag a queued transaction as prioritized (`priority = true`).
   - Prioritized transactions bypass the `now < execute_after` constraint during execution. A `TX_PRIO` event is emitted.
4. **Execution (`execute_tx`)**:
   - Once `now >= execute_after` (or `priority == true`), the administrator calls `execute_tx`.
   - The transaction is removed from the queue, and a `TX_EXEC` event is published.
5. **Cancellation or Veto (`cancel_tx` / `veto_tx`)**:
   - Either the administrator or the designated guardian can cancel or veto a queued transaction at any time prior to execution.
   - The transaction is purged from the queue, emitting `TX_CNCL` or `TX_VETO`.

---

## 3. Roles and Authorization

The Timelock contract enforces strict role boundaries:

| Role | Stored Key | Capabilities | Enforcement |
| :--- | :--- | :--- | :--- |
| **Admin** | `ADMIN` (Instance) | Queue, prioritize, execute, cancel, veto, set guardian | `caller.require_auth()` + `caller == admin` |
| **Guardian** | `GUARD` (Instance) | Cancel, veto queued transactions | `caller.require_auth()` + `caller == admin \|\| caller == guardian` |
| **Public / Any** | N/A | Read-only getters (`get_admin`, `get_min_delay`, `get_queue`, `get_tx`, `get_guardian`) | None (read-only queries) |

---

## 4. Error Reference (`TimelockError`)

The `TimelockError` enum defines 7 canonical error variants with numeric codes 1 through 7:

| Error Variant | Code | Trigger Condition |
| :--- | :---: | :--- |
| **`AlreadyInitialized`** | `1` | `initialize` is invoked when `ADMIN` is already present in instance storage. |
| **`NotInitialized`** | `2` | Any state-dependent or getter function is called before `initialize` has been executed. |
| **`Unauthorized`** | `3` | Caller fails role authorization checks (`caller != admin`, or caller is neither `admin` nor `guardian`). |
| **`TxAlreadyQueued`** | `4` | `queue_tx` is called with a transaction `id` that already exists in the queue. |
| **`TxNotFound`** | `5` | `get_tx`, `prioritize_tx`, `execute_tx`, `cancel_tx`, or `veto_tx` targets a transaction `id` not in the queue. |
| **`TooEarly`** | `6` | `execute_tx` is invoked when `now < execute_after` and `priority` is `false`. |
| **`DelayTooShort`** | `7` | `queue_tx` is called with a `delay` parameter less than the contract's configured `min_delay`. |

---

## 5. Public Functions Reference

### Initialization & Configuration

#### `initialize(env: Env, admin: Address, min_delay: u64) -> Result<(), TimelockError>`
Initializes the contract with an administrator address and minimum delay in seconds.
- **Authorization**: `admin.require_auth()`
- **Errors**: `AlreadyInitialized`

#### `set_guardian(env: Env, caller: Address, guardian: Address) -> Result<(), TimelockError>`
Sets or updates the security guardian address.
- **Authorization**: `caller.require_auth()`, requires `caller == admin`
- **Errors**: `NotInitialized`, `Unauthorized`

### Query Methods

#### `get_admin(env: Env) -> Result<Address, TimelockError>`
Returns the configured admin address.
- **Errors**: `NotInitialized`

#### `get_guardian(env: Env) -> Result<Address, TimelockError>`
Returns the active guardian address.
- **Errors**: `NotInitialized`

#### `get_min_delay(env: Env) -> Result<u64, TimelockError>`
Returns the minimum execution delay in seconds.
- **Errors**: `NotInitialized`

#### `get_queue(env: Env) -> Result<Vec<Tx>, TimelockError>`
Returns the entire list of currently queued transactions.
- **Errors**: `NotInitialized`

#### `get_tx(env: Env, id: BytesN<32>) -> Result<Tx, TimelockError>`
Returns the `Tx` entry matching the given identifier.
- **Errors**: `NotInitialized`, `TxNotFound`

### Transaction Management

#### `queue_tx(env: Env, caller: Address, id: BytesN<32>, delay: u64) -> Result<u64, TimelockError>`
Queues a new transaction with a specified delay.
- **Authorization**: `caller.require_auth()`, requires `caller == admin`
- **Errors**: `NotInitialized`, `Unauthorized`, `DelayTooShort`, `TxAlreadyQueued`
- **Returns**: The computed `execute_after` timestamp (`now + delay`).
- **Events**: Publishes `("TX_QUED", id, caller)` with data `execute_after`.

#### `prioritize_tx(env: Env, caller: Address, id: BytesN<32>) -> Result<(), TimelockError>`
Flags a queued transaction as prioritized to bypass timestamp delays.
- **Authorization**: `caller.require_auth()`, requires `caller == admin`
- **Errors**: `NotInitialized`, `Unauthorized`, `TxNotFound`
- **Events**: Publishes `("TX_PRIO", id, caller)`.

#### `execute_tx(env: Env, caller: Address, id: BytesN<32>) -> Result<(), TimelockError>`
Executes and dequeues a transaction after its timelock delay has elapsed (or if prioritized).
- **Authorization**: `caller.require_auth()`, requires `caller == admin`
- **Errors**: `NotInitialized`, `Unauthorized`, `TxNotFound`, `TooEarly`
- **Events**: Publishes `("TX_EXEC", id, caller)` with data `now`.

#### `cancel_tx(env: Env, caller: Address, id: BytesN<32>) -> Result<(), TimelockError>`
Cancels and removes a transaction from the queue.
- **Authorization**: `caller.require_auth()`, requires `caller == admin || caller == guardian`
- **Errors**: `NotInitialized`, `Unauthorized`, `TxNotFound`
- **Events**: Publishes `("TX_CNCL", id, caller)`.

#### `veto_tx(env: Env, caller: Address, id: BytesN<32>) -> Result<(), TimelockError>`
Vetoes and removes a transaction from the queue (alias for guardian cancellation).
- **Authorization**: `caller.require_auth()`, requires `caller == admin || caller == guardian`
- **Errors**: `NotInitialized`, `Unauthorized`, `TxNotFound`
- **Events**: Publishes `("TX_VETO", id, caller)`.

---

## 6. Worked Example: Queue, Wait, Execute

Below is a complete Soroban test demonstrating the end-to-end flow:

```rust
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};
use timelock::{TimelockContract, TimelockContractClient, TimelockError};

#[test]
fn test_timelock_lifecycle_example() {
    let env = Env::default();
    env.mock_all_auths();

    // 1. Setup contract and actors
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let min_delay = 86_400; // 1 day delay

    // Initialize timelock
    client.initialize(&admin, &min_delay);
    assert_eq!(client.get_min_delay(), 86_400);

    // 2. Queue a transaction
    let tx_id = BytesN::from_array(&env, &[7u8; 32]);
    let delay = 100_000; // > min_delay
    let execute_after = client.queue_tx(&admin, &tx_id, &delay);
    assert_eq!(execute_after, env.ledger().timestamp() + delay);

    // 3. Attempt execution before delay expires -> Fails with TooEarly
    let err = client.try_execute_tx(&admin, &tx_id);
    assert_eq!(err, Err(Ok(TimelockError::TooEarly)));

    // 4. Advance ledger time past execute_after
    env.ledger().set_timestamp(execute_after + 1);

    // 5. Execute transaction -> Successfully dequeued
    client.execute_tx(&admin, &tx_id);

    // Verify transaction is no longer in the queue
    let queue = client.get_queue();
    assert_eq!(queue.len(), 0);
}
```
