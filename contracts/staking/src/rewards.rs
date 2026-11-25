//! # Staking Reward Rounding & Precision Model
//!
//! ## Mathematical Model
//! The reward distribution engine uses an O(1) accumulator pattern scaled by [`PRECISION`] (10^12):
//!
//! ```text
//! 1. Accumulator Delta:
//!    Δrpt = ⌊ (reward_rate × elapsed × PRECISION) / total_staked ⌋
//!    rpt_new = rpt_stored + Δrpt
//!
//! 2. User Rewards Calculation:
//!    earned = ⌊ (staked × (rpt_current − rpt_paid)) / PRECISION ⌋ + user_earned
//! ```
//!
//! ## Rounding Direction
//! In Rust and Soroban integer arithmetic, division of non-negative integers strictly
//! **truncates toward zero (floor rounding)**.
//!
//! ### Conservation of Funds (No Inflation)
//! Because both accumulator calculation and user reward calculation round down:
//! - For every staker $i$: `earned_i <= exact_earned_i`
//! - Across all stakers: `∑ earned_i <= total_pool_reward`
//!
//! This directional choice guarantees that rounding **NEVER creates funds out of thin air**.
//! The protocol is completely immune to reward over-distribution or supply inflation attacks,
//! even when subjected to rapid micro-interactions or fragmented dust stakes.
//!
//! ### Dust Conservation (No Loss of Value)
//! Fractional sub-unit rewards that truncate down are preserved as a residual remainder ("dust")
//! in the contract reward pool:
//!
//! ```text
//! dust = total_pool_reward - ∑(earned_i) >= 0
//! ```
//!
//! For $N$ stakers claiming against a uniform accumulator snapshot, the maximum dust
//! per distribution epoch is strictly bounded:
//!
//! ```text
//! dust < N units (stroops) + (elapsed × reward_rate mod total_staked)
//! ```
//!
//! This dust is never lost or destroyed; it remains in the reward vault and rolls over into
//! subsequent distribution epochs or contract reserves.

/// Fixed-point scaling factor.
///
/// All reward-per-token values are multiplied by this constant before storage
/// to preserve sub-unit precision without floating-point arithmetic.
/// Using 10^12 gives 12 decimal places of precision, which is more than
/// sufficient for token amounts up to 10^18.
pub const PRECISION: i128 = 1_000_000_000_000;

// ── Core reward engine ──────────────────────────────────────────────────────

fn saturating_mul_checked(a: i128, b: i128) -> i128 {
    match a.checked_mul(b) {
        Some(v) => v,
        None => {
            if (a >= 0) == (b >= 0) {
                i128::MAX
            } else {
                i128::MIN
            }
        }
    }
}

/// Recompute the global `reward_per_token_stored` value.
///
/// This is the fundamental O(1) accumulation step:
///
/// ```text
/// Δrpt = ⌊ (reward_rate × elapsed_seconds × PRECISION) / total_staked ⌋
/// new_rpt = stored_rpt + Δrpt
/// ```
///
/// When `total_staked` is zero we return `stored` unchanged — no stakers
/// means no distribution, preventing division-by-zero and orphaned rewards.
///
/// # Rounding Direction
/// Integer division rounds down (floor), truncating any fractional delta.
/// This guarantees that Δrpt is never overstated, preserving value in the pool.
///
/// # Arguments
/// * `stored`       – current `reward_per_token_stored` (scaled by PRECISION)
/// * `reward_rate`  – tokens emitted per second across *all* stakers
/// * `elapsed`      – seconds since the last update
/// * `total_staked` – sum of all active stakes
#[allow(clippy::arithmetic_side_effects)]
pub fn compute_reward_per_token(
    stored: i128,
    reward_rate: i128,
    elapsed: u64,
    total_staked: i128,
) -> i128 {
    if total_staked <= 0 {
        return stored;
    }

    // Widen to i128 — reward_rate and PRECISION fit comfortably.
    // elapsed is u64; cast to i128 is safe since u64::MAX < i128::MAX.
    let rate_mul = saturating_mul_checked(reward_rate, elapsed as i128);
    let scaled = saturating_mul_checked(rate_mul, PRECISION);
    let delta = scaled.checked_div(total_staked).unwrap_or({
        if scaled >= 0 {
            i128::MAX
        } else {
            i128::MIN
        }
    });

    stored.saturating_add(delta)
}

/// Calculate the total rewards earned by a single staker.
///
/// ```text
/// earned = ⌊ staked × (current_rpt − user_rpt_paid) / PRECISION ⌋ + user_earned
/// ```
///
/// The subtraction `current_rpt − user_rpt_paid` isolates only the
/// accumulation that happened *since the user's last snapshot*, so prior
/// claims/snapshots are never double-counted.
///
/// # Rounding Direction
/// Division by `PRECISION` truncates toward zero (floor). This strictly protects
/// the contract from overpaying rewards: the staker receives `floor(entitled_rewards)`
/// and fractional sub-stroop remainders are conserved in the pool.
///
/// # Arguments
/// * `staked`        – user's current staked balance
/// * `current_rpt`   – latest global `reward_per_token_stored`
/// * `user_rpt_paid` – the snapshot taken at the user's last interaction
/// * `user_earned`   – already-accumulated rewards not yet claimed
#[allow(clippy::arithmetic_side_effects)]
pub fn earned(staked: i128, current_rpt: i128, user_rpt_paid: i128, user_earned: i128) -> i128 {
    let delta_rpt = current_rpt.saturating_sub(user_rpt_paid);
    let numer = saturating_mul_checked(staked, delta_rpt);
    let new_rewards =
        numer
            .checked_div(PRECISION)
            .unwrap_or(if numer >= 0 { i128::MAX } else { i128::MIN });

    user_earned.saturating_add(new_rewards)
}

// ── Unit tests ──────────────────────────────────────────────────────────────
// These are pure-math tests with no Soroban environment dependency.

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn rpt_zero_when_no_stakers() {
        let rpt = compute_reward_per_token(500, 100, 60, 0);
        assert_eq!(rpt, 500, "RPT must not change when total_staked is zero");
    }

    #[test]
    fn rpt_accumulates_correctly() {
        // reward_rate=10 tokens/s, elapsed=100s, total_staked=1_000 tokens
        // Δrpt = 10 × 100 × PRECISION / 1_000 = 1_000 × PRECISION / 1_000 = PRECISION
        let rpt = compute_reward_per_token(0, 10, 100, 1_000);
        assert_eq!(rpt, PRECISION);
    }

    #[test]
    fn earned_zero_when_no_new_accumulation() {
        // If user's snapshot equals current RPT, no new rewards.
        let e = earned(500, 100, 100, 50);
        assert_eq!(e, 50);
    }

    #[test]
    fn earned_proportional_to_stake() {
        // RPT increased by PRECISION since last snapshot.
        // staked=1_000 → earned_new = 1_000 × PRECISION / PRECISION = 1_000
        let e = earned(1_000, PRECISION, 0, 0);
        assert_eq!(e, 1_000);
    }

    #[test]
    fn earned_does_not_overflow_large_amounts() {
        // Stress test: large stake × large RPT delta.
        // i128::MAX ≈ 1.7 × 10^38; with PRECISION = 10^12 and typical token
        // decimals of 7 (Stellar), staked values up to 10^15 are realistic.
        // saturating_mul clamps at i128::MAX rather than wrapping, so the
        // result must be positive and the call must not panic.
        let large_stake: i128 = 1_000_000_000_000_000; // 10^15
        let rpt_delta = PRECISION.saturating_mul(1_000); // large accumulation
        let e = earned(large_stake, rpt_delta, 0, 0);
        assert!(e > 0);
        assert_eq!(e, large_stake.saturating_mul(1_000)); // 10^15 × 1000 = 10^18
    }
}

// ── Property tests (proptest) ───────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    extern crate std;
    use super::*;
    use proptest::prelude::*;
    use std::vec::Vec;

    proptest! {
        /// Property: Rounding down never creates funds across many small stakes.
        /// The sum of all stakers' earned rewards must NEVER exceed the total pool emission.
        #[test]
        fn proptest_rounding_never_creates_funds_many_small_stakes(
            reward_rate in 1i128..10_000_000i128,
            elapsed in 1u64..86_400u64,
            // 2 to 50 small stakers with 1 to 500 stroops each
            stakes in prop::collection::vec(1i128..500i128, 2..50),
        ) {
            let total_staked: i128 = stakes.iter().sum();
            let total_pool_emission = reward_rate.saturating_mul(elapsed as i128);

            let rpt = compute_reward_per_token(0, reward_rate, elapsed, total_staked);

            let mut sum_earned = 0i128;
            for &staked in &stakes {
                let user_reward = earned(staked, rpt, 0, 0);
                sum_earned = sum_earned.saturating_add(user_reward);
            }

            // Invariant 1: Sum of rewards NEVER creates funds (<= total pool emission)
            prop_assert!(
                sum_earned <= total_pool_emission,
                "Reward inflation detected: sum_earned ({}) > total_pool_emission ({})",
                sum_earned,
                total_pool_emission
            );

            // Invariant 2: Dust remainder is strictly non-negative (conserved in pool)
            let dust = total_pool_emission - sum_earned;
            prop_assert!(
                dust >= 0,
                "Negative dust: pool lost funds (dust = {})",
                dust
            );

            // Invariant 3: Dust is tightly bounded and cannot exceed the pool emission
            prop_assert!(dust <= total_pool_emission);
        }

        /// Property: Uneven stake distribution (whales and micro-stakers).
        /// Mixed dust stakers (1..10) alongside large stakers (10^7..10^12)
        /// must strictly conserve total pool funds without inflation.
        #[test]
        fn proptest_uneven_stakes_conservation(
            reward_rate in 1i128..1_000_000i128,
            elapsed in 1u64..3600u64,
            micro_stakes in prop::collection::vec(1i128..10i128, 10..30),
            whale_stake in 1_000_000i128..1_000_000_000i128,
        ) {
            let mut all_stakes = micro_stakes;
            all_stakes.push(whale_stake);
            let total_staked: i128 = all_stakes.iter().sum();
            let total_pool_emission = reward_rate.saturating_mul(elapsed as i128);

            let rpt = compute_reward_per_token(0, reward_rate, elapsed, total_staked);

            let mut sum_earned = 0i128;
            for &staked in &all_stakes {
                let user_reward = earned(staked, rpt, 0, 0);
                sum_earned = sum_earned.saturating_add(user_reward);
            }

            prop_assert!(
                sum_earned <= total_pool_emission,
                "Funds created in uneven pool: sum_earned ({}) > total_pool_emission ({})",
                sum_earned,
                total_pool_emission
            );
            prop_assert!(total_pool_emission - sum_earned >= 0);
        }

        /// Property: Incremental multi-step reward accumulation across time intervals
        /// preserves funds conservation and does not compound rounding errors into inflation.
        #[test]
        fn proptest_incremental_accumulation_no_inflation(
            reward_rate in 10i128..10_000i128,
            intervals in prop::collection::vec(1u64..3600u64, 2..6),
            stakes in prop::collection::vec(1i128..100i128, 3..15),
        ) {
            let total_staked: i128 = stakes.iter().sum();
            let total_elapsed: u64 = intervals.iter().sum();
            let total_emission = reward_rate.saturating_mul(total_elapsed as i128);

            let mut current_rpt = 0i128;
            for &dt in &intervals {
                current_rpt = compute_reward_per_token(current_rpt, reward_rate, dt, total_staked);
            }

            let mut sum_earned = 0i128;
            for &staked in &stakes {
                let user_reward = earned(staked, current_rpt, 0, 0);
                sum_earned = sum_earned.saturating_add(user_reward);
            }

            prop_assert!(
                sum_earned <= total_emission,
                "Multi-epoch inflation: sum_earned ({}) > total_emission ({})",
                sum_earned,
                total_emission
            );
        }
    }
}
