//! Consistency tests for `vector_clock` (causal ordering) and `concurrency`
//! (optimistic concurrency control).
//!
//! Coverage:
//! - causality tracking across several actors exchanging messages
//! - merging divergent clocks (commutative, associative, idempotent, monotonic)
//! - conflict detection, both at the clock level and through the OCC conflict
//!   queue with field-level overlap
//! - concurrent operations: many providers racing on the same record version
//!   under each resolution strategy

use crate::concurrency::{
    compare_and_swap, get_pending_conflicts, get_record_conflicts, get_record_version,
    get_version_stamp, init_record_version, resolve_conflict, save_field_snapshot,
    set_resolution_strategy, ConflictStatus, FieldChange, ResolutionStrategy, UpdateOutcome,
    MAX_CONFLICT_QUEUE_SIZE,
};
use crate::vector_clock::{ClockOrdering, VectorClock, MAX_VECTOR_CLOCK_NODES};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, Address, Env, String, Vec};

#[contract]
struct MockContract;

/// Storage-backed OCC calls need a contract context.
fn with_contract<F: FnOnce() -> R, R>(env: &Env, f: F) -> R {
    let contract_id = env.register(MockContract, ());
    env.as_contract(&contract_id, f)
}

// ── Actor model helpers ─────────────────────────────────────────────────────

/// A process in a classic vector-clock message-passing model.
struct Actor {
    id: u32,
    clock: VectorClock,
}

impl Actor {
    fn new(env: &Env, id: u32) -> Self {
        Self {
            id,
            clock: VectorClock::new(env),
        }
    }

    /// A local event: tick own counter, return a snapshot of the event's clock.
    fn local(&mut self, env: &Env) -> VectorClock {
        self.clock.increment(env, self.id);
        self.clock.clone()
    }

    /// Sending is a local event whose clock travels with the message.
    fn send(&mut self, env: &Env) -> VectorClock {
        self.local(env)
    }

    /// Receiving merges the message clock, then ticks own counter.
    fn receive(&mut self, env: &Env, msg: &VectorClock) -> VectorClock {
        self.clock.merge(msg);
        self.local(env)
    }
}

fn clock(env: &Env, entries: &[(u32, u64)]) -> VectorClock {
    let mut c = VectorClock::new(env);
    for (node, value) in entries {
        c.entries.set(*node, *value);
    }
    c
}

fn happened_before(a: &VectorClock, b: &VectorClock) -> bool {
    a.compare(b) == ClockOrdering::Before
}

fn concurrent(a: &VectorClock, b: &VectorClock) -> bool {
    a.compare(b) == ClockOrdering::Concurrent
}

// ── Causality across multiple actors ────────────────────────────────────────

#[test]
fn causality_message_chain_across_three_actors() {
    let env = Env::default();
    let mut a = Actor::new(&env, 1);
    let mut b = Actor::new(&env, 2);
    let mut c = Actor::new(&env, 3);

    let a1 = a.send(&env); // A: {1:1}
    let b1 = b.receive(&env, &a1); // B: {1:1, 2:1}
    let b2 = b.send(&env); // B: {1:1, 2:2}
    let c1 = c.receive(&env, &b2); // C: {1:1, 2:2, 3:1}

    assert!(happened_before(&a1, &b1));
    assert!(happened_before(&b1, &b2));
    assert!(happened_before(&b2, &c1));
    // Transitivity: A's send precedes C's receive even with no direct message.
    assert!(happened_before(&a1, &c1));
    assert_eq!(c1.compare(&a1), ClockOrdering::After);

    assert_eq!(c1.get(1), 1);
    assert_eq!(c1.get(2), 2);
    assert_eq!(c1.get(3), 1);
}

#[test]
fn causality_independent_actors_are_concurrent_until_they_communicate() {
    let env = Env::default();
    let mut a = Actor::new(&env, 1);
    let mut b = Actor::new(&env, 2);
    let mut c = Actor::new(&env, 3);

    let a1 = a.local(&env);
    let b1 = b.local(&env);
    let c1 = c.local(&env);
    assert!(concurrent(&a1, &b1));
    assert!(concurrent(&b1, &c1));
    assert!(concurrent(&a1, &c1));

    // A tells C; C's next event now follows A's but stays concurrent with B's.
    let c2 = c.receive(&env, &a1);
    assert!(happened_before(&a1, &c2));
    assert!(happened_before(&c1, &c2));
    assert!(concurrent(&b1, &c2));
}

#[test]
fn causality_four_actor_fan_out_and_fan_in() {
    let env = Env::default();
    let mut hub = Actor::new(&env, 0);
    let mut spokes = [
        Actor::new(&env, 1),
        Actor::new(&env, 2),
        Actor::new(&env, 3),
    ];

    // Fan-out: the hub broadcasts the same message to every spoke.
    let broadcast = hub.send(&env);
    let mut spoke_events = alloc::vec::Vec::new();
    for s in spokes.iter_mut() {
        let ev = s.receive(&env, &broadcast);
        assert!(happened_before(&broadcast, &ev));
        spoke_events.push(s.send(&env));
    }

    // Spoke events are pairwise concurrent: none saw the others.
    for i in 0..spoke_events.len() {
        for j in (i + 1)..spoke_events.len() {
            assert!(concurrent(&spoke_events[i], &spoke_events[j]));
        }
    }

    // Fan-in: the hub collects all replies; its clock dominates every event.
    let mut last = broadcast.clone();
    for ev in spoke_events.iter() {
        last = hub.receive(&env, ev);
    }
    for ev in spoke_events.iter() {
        assert!(happened_before(ev, &last));
    }
    assert_eq!(last.get(0), 4); // one send + three receives
    for id in 1..=3u32 {
        assert_eq!(last.get(id), 2);
    }
}

#[test]
fn causality_ordering_is_antisymmetric_and_reflexively_equal() {
    let env = Env::default();
    let x = clock(&env, &[(1, 2), (2, 1)]);
    let y = clock(&env, &[(1, 3), (2, 1)]);
    let z = clock(&env, &[(1, 1), (2, 5)]);

    assert_eq!(x.compare(&x), ClockOrdering::Equal);
    assert_eq!(x.compare(&y), ClockOrdering::Before);
    assert_eq!(y.compare(&x), ClockOrdering::After);
    assert_eq!(x.compare(&z), ClockOrdering::Concurrent);
    assert_eq!(z.compare(&x), ClockOrdering::Concurrent);

    assert!(x.dominates(&x));
    assert!(y.dominates(&x));
    assert!(!x.dominates(&y));
    assert!(!x.dominates(&z) && !z.dominates(&x));
}

#[test]
fn causality_missing_entries_compare_as_zero() {
    let env = Env::default();
    // {1:1} vs {1:1, 2:1}: the extra node makes the right clock strictly later.
    let left = clock(&env, &[(1, 1)]);
    let right = clock(&env, &[(1, 1), (2, 1)]);
    assert_eq!(left.compare(&right), ClockOrdering::Before);
    assert_eq!(right.compare(&left), ClockOrdering::After);

    // An explicit zero entry is indistinguishable from an absent one.
    let explicit_zero = clock(&env, &[(1, 1), (2, 0)]);
    assert_eq!(left.compare(&explicit_zero), ClockOrdering::Equal);
    assert_eq!(explicit_zero.compare(&left), ClockOrdering::Equal);
}

// ── Merge of divergent clocks ───────────────────────────────────────────────

#[test]
fn merge_takes_component_wise_maximum() {
    let env = Env::default();
    let mut a = clock(&env, &[(1, 4), (2, 1), (3, 7)]);
    let b = clock(&env, &[(1, 2), (2, 6), (4, 3)]);
    assert!(concurrent(&a, &b));

    a.merge(&b);
    assert_eq!(a.get(1), 4);
    assert_eq!(a.get(2), 6);
    assert_eq!(a.get(3), 7);
    assert_eq!(a.get(4), 3);
    assert_eq!(a.entries.len(), 4);
}

#[test]
fn merge_is_commutative() {
    let env = Env::default();
    let a = clock(&env, &[(1, 4), (2, 1)]);
    let b = clock(&env, &[(2, 6), (3, 2)]);

    let mut ab = a.clone();
    ab.merge(&b);
    let mut ba = b.clone();
    ba.merge(&a);
    assert_eq!(ab.compare(&ba), ClockOrdering::Equal);
}

#[test]
fn merge_is_associative() {
    let env = Env::default();
    let a = clock(&env, &[(1, 4), (2, 1)]);
    let b = clock(&env, &[(2, 6), (3, 2)]);
    let c = clock(&env, &[(1, 5), (4, 9)]);

    let mut left = a.clone();
    left.merge(&b);
    left.merge(&c);

    let mut bc = b.clone();
    bc.merge(&c);
    let mut right = a.clone();
    right.merge(&bc);

    assert_eq!(left.compare(&right), ClockOrdering::Equal);
}

#[test]
fn merge_is_idempotent_and_identity_on_empty() {
    let env = Env::default();
    let a = clock(&env, &[(1, 4), (2, 1)]);

    let mut twice = a.clone();
    twice.merge(&a);
    assert_eq!(twice.compare(&a), ClockOrdering::Equal);

    let mut with_empty = a.clone();
    with_empty.merge(&VectorClock::new(&env));
    assert_eq!(with_empty.compare(&a), ClockOrdering::Equal);

    let mut empty = VectorClock::new(&env);
    empty.merge(&a);
    assert_eq!(empty.compare(&a), ClockOrdering::Equal);
}

#[test]
fn merge_never_moves_backwards() {
    let env = Env::default();
    let newer = clock(&env, &[(1, 10), (2, 10)]);
    let older = clock(&env, &[(1, 3), (2, 9)]);
    assert!(happened_before(&older, &newer));

    let mut merged = newer.clone();
    merged.merge(&older);
    assert_eq!(merged.compare(&newer), ClockOrdering::Equal);
}

#[test]
fn merge_of_divergent_replicas_resolves_to_common_successor() {
    let env = Env::default();
    // Two replicas fork from a shared base and diverge independently.
    let mut base = VectorClock::new(&env);
    base.increment(&env, 1);

    let mut replica_a = base.clone();
    replica_a.increment(&env, 1);
    replica_a.increment(&env, 1);

    let mut replica_b = base.clone();
    replica_b.increment(&env, 2);

    assert!(happened_before(&base, &replica_a));
    assert!(happened_before(&base, &replica_b));
    assert!(concurrent(&replica_a, &replica_b));

    let mut reconciled = replica_a.clone();
    reconciled.merge(&replica_b);
    assert!(reconciled.dominates(&replica_a));
    assert!(reconciled.dominates(&replica_b));

    // Recording the reconciliation as a new event makes it strictly later.
    reconciled.increment(&env, 3);
    assert!(happened_before(&replica_a, &reconciled));
    assert!(happened_before(&replica_b, &reconciled));
}

#[test]
fn merge_respects_node_cap_for_new_entries_from_increment_only() {
    let env = Env::default();
    // Fill one clock to the cap.
    let mut full = VectorClock::new(&env);
    for i in 0..MAX_VECTOR_CLOCK_NODES {
        full.increment(&env, i);
    }
    assert_eq!(full.entries.len(), MAX_VECTOR_CLOCK_NODES);
    // increment rejects a new node at the cap...
    assert_eq!(full.increment(&env, MAX_VECTOR_CLOCK_NODES + 1), 0);
    // ...while merge still keeps every observed entry, so no history is lost.
    let other = clock(&env, &[(MAX_VECTOR_CLOCK_NODES + 1, 1)]);
    full.merge(&other);
    assert_eq!(full.get(MAX_VECTOR_CLOCK_NODES + 1), 1);
    assert!(full.dominates(&other));
}

#[test]
fn increment_saturates_instead_of_overflowing() {
    let env = Env::default();
    let mut c = clock(&env, &[(1, u64::MAX)]);
    assert_eq!(c.increment(&env, 1), u64::MAX);
    assert_eq!(c.get(1), u64::MAX);
}

// ── Conflict detection ──────────────────────────────────────────────────────

#[test]
fn conflict_detected_between_concurrent_writers_is_symmetric() {
    let env = Env::default();
    let mut writer_a = Actor::new(&env, 1);
    let mut writer_b = Actor::new(&env, 2);
    let shared = writer_a.send(&env);
    writer_b.receive(&env, &shared);

    // Both edit after seeing the same base.
    let edit_a = writer_a.local(&env);
    let edit_b = writer_b.local(&env);

    assert!(concurrent(&edit_a, &edit_b));
    assert!(concurrent(&edit_b, &edit_a));
    assert!(!edit_a.dominates(&edit_b));
    assert!(!edit_b.dominates(&edit_a));
}

#[test]
fn conflict_queue_records_overlapping_fields_under_merge() {
    let env = Env::default();
    with_contract(&env, || {
        init_record_version(&env, 7, 1);
        set_resolution_strategy(&env, 7, &ResolutionStrategy::Merge);

        // The stored snapshot already reflects another provider's change to
        // `iop` (hash "iop_v2"), and `acuity` is still at "acuity_v1".
        let mut snapshot = Vec::new(&env);
        snapshot.push_back(field(&env, "iop", "iop_v1", "iop_v2"));
        snapshot.push_back(field(&env, "acuity", "acuity_v0", "acuity_v1"));
        save_field_snapshot(&env, 7, &snapshot);

        let provider_a = Address::generate(&env);
        let provider_b = Address::generate(&env);
        let empty = Vec::new(&env);
        // Provider A advances the record to v2.
        assert!(matches!(
            compare_and_swap(&env, 7, 1, 1, &provider_a, &empty),
            UpdateOutcome::Applied(_)
        ));

        // Provider B, still on v1, edits both fields based on stale `iop_v1`.
        let mut changes = Vec::new(&env);
        changes.push_back(field(&env, "iop", "iop_v1", "iop_b"));
        changes.push_back(field(&env, "acuity", "acuity_v1", "acuity_b"));
        let cid = match compare_and_swap(&env, 7, 1, 2, &provider_b, &changes) {
            UpdateOutcome::Conflicted(cid) => cid,
            other => panic!("expected Conflicted, got {:?}", other),
        };

        let conflicts = get_record_conflicts(&env, 7);
        assert_eq!(conflicts.len(), 1);
        let entry = conflicts.get(0).unwrap();
        assert_eq!(entry.conflict_id, cid);
        assert_eq!(entry.status, ConflictStatus::Pending);
        assert_eq!(entry.strategy, ResolutionStrategy::Merge);
        // Only `iop` truly conflicts; `acuity` matched the snapshot.
        assert_eq!(entry.conflicting_fields.len(), 1);
        assert_eq!(
            entry.conflicting_fields.get(0).unwrap(),
            String::from_str(&env, "iop")
        );
        // The proposed clock is a successor of the stored clock.
        assert_eq!(entry.clock_b.compare(&entry.clock_a), ClockOrdering::After);
        // The record itself was not changed by the conflicting write.
        assert_eq!(get_record_version(&env, 7), 2);
    });
}

#[test]
fn conflict_manual_review_queues_even_without_field_overlap() {
    let env = Env::default();
    with_contract(&env, || {
        init_record_version(&env, 3, 1);
        let provider = Address::generate(&env);
        let empty = Vec::new(&env);
        let _ = compare_and_swap(&env, 3, 1, 1, &provider, &empty);

        match compare_and_swap(&env, 3, 1, 2, &provider, &empty) {
            UpdateOutcome::Conflicted(_) => {}
            other => panic!("expected Conflicted, got {:?}", other),
        }
        let entry = get_record_conflicts(&env, 3).get(0).unwrap();
        assert_eq!(entry.strategy, ResolutionStrategy::ManualReview);
        assert_eq!(entry.conflicting_fields.len(), 1);
        assert_eq!(
            entry.conflicting_fields.get(0).unwrap(),
            String::from_str(&env, "*")
        );
    });
}

#[test]
fn conflict_resolution_is_scoped_and_single_use() {
    let env = Env::default();
    with_contract(&env, || {
        let provider = Address::generate(&env);
        let reviewer = Address::generate(&env);
        let empty = Vec::new(&env);

        // Two records, each with one stale write.
        for record in [10u64, 11] {
            init_record_version(&env, record, 1);
            let _ = compare_and_swap(&env, record, 1, 1, &provider, &empty);
            let _ = compare_and_swap(&env, record, 1, 2, &provider, &empty);
        }
        assert_eq!(get_pending_conflicts(&env).len(), 2);
        let cid = get_record_conflicts(&env, 10).get(0).unwrap().conflict_id;

        assert!(resolve_conflict(&env, cid, &reviewer));
        // Second resolution of the same conflict and unknown ids are no-ops.
        assert!(!resolve_conflict(&env, cid, &reviewer));
        assert!(!resolve_conflict(&env, 9_999, &reviewer));

        let pending = get_pending_conflicts(&env);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending.get(0).unwrap().record_id, 11);

        let resolved = get_record_conflicts(&env, 10).get(0).unwrap();
        assert_eq!(resolved.status, ConflictStatus::Resolved);
        assert_eq!(resolved.resolved_by, Some(reviewer));
    });
}

#[test]
fn conflict_queue_evicts_oldest_beyond_cap() {
    // Each write runs as its own invocation under the default network limits
    // (CPU, memory, and the 64 KiB per-entry size), as it would on-chain. A
    // cap whose full queue does not fit in one storage entry fails here.
    let env = Env::default();
    let contract_id = env.register(MockContract, ());
    let provider = Address::generate(&env);
    // A realistic three-field conflict, larger than the "*" placeholder.
    let mut changes = Vec::new(&env);
    for name in ["visual_acuity", "intraocular_pressure", "sphere_power"] {
        changes.push_back(field(
            &env,
            name,
            "stale_hash_value_0000",
            "new_hash_value_0000",
        ));
    }

    env.as_contract(&contract_id, || {
        init_record_version(&env, 1, 1);
        set_resolution_strategy(&env, 1, &ResolutionStrategy::Merge);
        let mut snapshot = Vec::new(&env);
        for name in ["visual_acuity", "intraocular_pressure", "sphere_power"] {
            snapshot.push_back(field(&env, name, "base", "current_hash_value_00"));
        }
        save_field_snapshot(&env, 1, &snapshot);
        let _ = compare_and_swap(&env, 1, 1, 1, &provider, &Vec::new(&env));
    });

    let total = MAX_CONFLICT_QUEUE_SIZE + 4;
    for _ in 0..total {
        env.cost_estimate().budget().reset_default();
        let outcome = env.as_contract(&contract_id, || {
            compare_and_swap(&env, 1, 1, 2, &provider, &changes)
        });
        assert!(matches!(outcome, UpdateOutcome::Conflicted(_)));
    }

    env.cost_estimate().budget().reset_default();
    env.as_contract(&contract_id, || {
        let queue = get_record_conflicts(&env, 1);
        assert_eq!(queue.len(), MAX_CONFLICT_QUEUE_SIZE);
        assert_eq!(queue.get(0).unwrap().conflicting_fields.len(), 3);
        // The four oldest ids (1..=4) were evicted; ids stay monotonic.
        assert_eq!(queue.get(0).unwrap().conflict_id, 5);
        assert_eq!(
            queue.get(MAX_CONFLICT_QUEUE_SIZE - 1).unwrap().conflict_id,
            u64::from(total)
        );
    });
}

fn field(env: &Env, name: &str, old_hash: &str, new_hash: &str) -> FieldChange {
    FieldChange {
        field_name: String::from_str(env, name),
        old_hash: String::from_str(env, old_hash),
        new_hash: String::from_str(env, new_hash),
    }
}

// ── Concurrent operations ───────────────────────────────────────────────────

/// `n` providers all read version 1 and then race to write it.
fn race(
    env: &Env,
    record_id: u64,
    strategy: ResolutionStrategy,
    n: u32,
) -> alloc::vec::Vec<UpdateOutcome> {
    init_record_version(env, record_id, 0);
    set_resolution_strategy(env, record_id, &strategy);
    let read_version = get_version_stamp(env, record_id).version;
    let empty = Vec::new(env);

    let mut outcomes = alloc::vec::Vec::new();
    for node in 1..=n {
        let provider = Address::generate(env);
        outcomes.push(compare_and_swap(
            env,
            record_id,
            read_version,
            node,
            &provider,
            &empty,
        ));
    }
    outcomes
}

#[test]
fn concurrent_writers_manual_review_first_wins_rest_queue() {
    let env = Env::default();
    with_contract(&env, || {
        let outcomes = race(&env, 1, ResolutionStrategy::ManualReview, 5);

        assert!(matches!(outcomes[0], UpdateOutcome::Applied(_)));
        let mut ids = alloc::vec::Vec::new();
        for o in outcomes.iter().skip(1) {
            match o {
                UpdateOutcome::Conflicted(cid) => ids.push(*cid),
                other => panic!("expected Conflicted, got {:?}", other),
            }
        }
        // Conflict ids are unique and increasing.
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(get_pending_conflicts(&env).len(), 4);

        // Only the winning write touched the record.
        let stamp = get_version_stamp(&env, 1);
        assert_eq!(stamp.version, 2);
        assert_eq!(stamp.clock.get(0), 1);
        assert_eq!(stamp.clock.get(1), 1);
        for node in 2..=5 {
            assert_eq!(stamp.clock.get(node), 0);
        }
    });
}

#[test]
fn concurrent_writers_last_writer_wins_all_apply_in_order() {
    let env = Env::default();
    with_contract(&env, || {
        let outcomes = race(&env, 2, ResolutionStrategy::LastWriterWins, 5);

        let mut prev: Option<VectorClock> = None;
        for (i, o) in outcomes.iter().enumerate() {
            let stamp = match o {
                UpdateOutcome::Applied(s) => s.clone(),
                other => panic!("expected Applied, got {:?}", other),
            };
            assert_eq!(stamp.version, i as u64 + 2);
            if let Some(p) = prev {
                // Each accepted write is a causal successor of the previous one.
                assert!(happened_before(&p, &stamp.clock));
            }
            prev = Some(stamp.clock);
        }
        assert!(get_pending_conflicts(&env).is_empty());

        // Every writer is reflected in the final clock exactly once.
        let final_clock = get_version_stamp(&env, 2).clock;
        for node in 0..=5 {
            assert_eq!(final_clock.get(node), 1);
        }
    });
}

#[test]
fn concurrent_writers_merge_disjoint_fields_all_merge() {
    let env = Env::default();
    with_contract(&env, || {
        init_record_version(&env, 4, 0);
        set_resolution_strategy(&env, 4, &ResolutionStrategy::Merge);
        let names = ["acuity", "iop", "sphere", "cylinder"];

        let mut base = Vec::new(&env);
        for n in names {
            base.push_back(field(&env, n, "v0", "v1"));
        }
        save_field_snapshot(&env, 4, &base);

        // Each provider edits a different field from the same base version.
        let mut merged = 0;
        for (i, n) in names.iter().enumerate() {
            let mut change = Vec::new(&env);
            change.push_back(field(&env, n, "v1", "v2"));
            let provider = Address::generate(&env);
            match compare_and_swap(&env, 4, 1, i as u32 + 1, &provider, &change) {
                UpdateOutcome::Applied(_) if i == 0 => {}
                UpdateOutcome::Merged(_) if i > 0 => merged += 1,
                other => panic!("write {} unexpected outcome {:?}", i, other),
            }
        }
        assert_eq!(merged, 3);
        assert!(get_pending_conflicts(&env).is_empty());
        assert_eq!(get_record_version(&env, 4), 5);
    });
}

#[test]
fn concurrent_writers_on_different_records_do_not_interfere() {
    let env = Env::default();
    with_contract(&env, || {
        let provider = Address::generate(&env);
        let empty = Vec::new(&env);
        for record in 100u64..105 {
            init_record_version(&env, record, 1);
        }
        // Every record sees exactly one write against its fresh version.
        for record in 100u64..105 {
            assert!(matches!(
                compare_and_swap(&env, record, 1, 2, &provider, &empty),
                UpdateOutcome::Applied(_)
            ));
        }
        for record in 100u64..105 {
            assert_eq!(get_record_version(&env, record), 2);
        }
        assert!(get_pending_conflicts(&env).is_empty());
    });
}

#[test]
fn concurrent_sequential_readers_never_conflict() {
    let env = Env::default();
    with_contract(&env, || {
        init_record_version(&env, 9, 1);
        let empty = Vec::new(&env);
        // Read-modify-write loop where each writer re-reads before writing.
        for round in 0..10u32 {
            let stamp = get_version_stamp(&env, 9);
            let provider = Address::generate(&env);
            match compare_and_swap(&env, 9, stamp.version, round % 3 + 1, &provider, &empty) {
                UpdateOutcome::Applied(next) => {
                    assert_eq!(next.version, stamp.version + 1);
                    assert!(happened_before(&stamp.clock, &next.clock));
                }
                other => panic!("round {} unexpected {:?}", round, other),
            }
        }
        assert_eq!(get_record_version(&env, 9), 11);
        assert!(get_pending_conflicts(&env).is_empty());
    });
}
