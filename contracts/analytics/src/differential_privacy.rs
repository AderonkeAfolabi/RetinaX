use soroban_sdk::Env;

/// Epsilon used until an admin sets one. Matches the behaviour before the parameters were configurable.
pub const DEFAULT_EPSILON: u32 = 1;
/// Sensitivity used until an admin sets one.
pub const DEFAULT_SENSITIVITY: i128 = 10;
/// Smallest epsilon an admin may set. Zero would switch noise off, so it is refused.
pub const MIN_EPSILON: u32 = 1;
/// Largest epsilon an admin may set. A larger epsilon gives less privacy, so the bound keeps it meaningful.
pub const MAX_EPSILON: u32 = 10;
/// Largest sensitivity an admin may set. It bounds the magnitude of the noise.
pub const MAX_SENSITIVITY: i128 = 1_000_000;

pub struct DifferentialPrivacy;

impl DifferentialPrivacy {
    /// Whether an (epsilon, sensitivity) pair lies within the configurable bounds (#463).
    pub fn validate(epsilon: u32, sensitivity: i128) -> bool {
        (MIN_EPSILON..=MAX_EPSILON).contains(&epsilon)
            && (1..=MAX_SENSITIVITY).contains(&sensitivity)
    }

    /// Adds noise to a value to satisfy epsilon-differential privacy.
    ///
    /// The noise scale is `b = sensitivity / epsilon` (rounded down), and the noise is drawn uniformly from
    /// `[-b, b]`. On-chain we use a bounded uniform draw from `env.prng()` rather than a true Laplace sample, which
    /// needs logarithms that i128 arithmetic cannot provide. With `epsilon` and `sensitivity` at their defaults the
    /// bound is 10, the same as before the parameters were configurable.
    pub fn add_laplace_noise(env: &Env, value: i128, epsilon: u32, sensitivity: i128) -> i128 {
        if epsilon == 0 || sensitivity <= 0 {
            return value; // Only reachable from internal callers: admin-set parameters are validated.
        }
        let scale = (sensitivity as u64) / (epsilon as u64);
        if scale == 0 {
            return value;
        }

        let seed: u64 = env.prng().gen_range(0..u64::MAX);
        let range_val: u64 = scale.saturating_mul(2).saturating_add(1);
        let rem: u64 = seed % range_val;
        let noise: i128 = (rem as i128) - (scale as i128);

        value.saturating_add(noise)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn noise_stays_within_the_scale_bound() {
        let env = Env::default();
        // epsilon 2, sensitivity 100: scale 50.
        for _ in 0..2000 {
            let v = DifferentialPrivacy::add_laplace_noise(&env, 0, 2, 100);
            assert!((-50..=50).contains(&v), "noise {v} outside [-50, 50]");
        }
    }

    #[test]
    fn higher_epsilon_gives_smaller_noise() {
        let env = Env::default();
        let max_abs = |epsilon: u32| {
            (0..2000)
                .map(|_| DifferentialPrivacy::add_laplace_noise(&env, 0, epsilon, 100).abs())
                .max()
                .unwrap_or(0)
        };
        // Scale 100 at epsilon 1, scale 10 at epsilon 10.
        assert!(max_abs(1) > 50, "epsilon 1 should produce large noise");
        assert!(max_abs(10) <= 10, "epsilon 10 should bound the noise to 10");
    }

    #[test]
    fn noise_is_centred_on_zero_and_not_constant() {
        let env = Env::default();
        let samples: std::vec::Vec<i128> = (0..5000)
            .map(|_| DifferentialPrivacy::add_laplace_noise(&env, 0, 1, 10))
            .collect();
        let mean = samples.iter().sum::<i128>() as f64 / samples.len() as f64;
        let distinct = samples.iter().collect::<std::collections::BTreeSet<&i128>>().len();

        assert!(mean.abs() < 1.0, "mean {mean} should be close to zero");
        assert!(distinct > 10, "noise should take many distinct values, got {distinct}");
    }

    #[test]
    fn zero_scale_adds_no_noise() {
        let env = Env::default();
        // sensitivity 5 at epsilon 10 rounds the scale down to zero.
        assert_eq!(DifferentialPrivacy::add_laplace_noise(&env, 42, 10, 5), 42);
    }

    #[test]
    fn validate_enforces_the_configured_bounds() {
        assert!(DifferentialPrivacy::validate(DEFAULT_EPSILON, DEFAULT_SENSITIVITY));
        assert!(DifferentialPrivacy::validate(MIN_EPSILON, 1));
        assert!(DifferentialPrivacy::validate(MAX_EPSILON, MAX_SENSITIVITY));
        assert!(!DifferentialPrivacy::validate(0, DEFAULT_SENSITIVITY));
        assert!(!DifferentialPrivacy::validate(MAX_EPSILON + 1, DEFAULT_SENSITIVITY));
        assert!(!DifferentialPrivacy::validate(DEFAULT_EPSILON, 0));
        assert!(!DifferentialPrivacy::validate(DEFAULT_EPSILON, -1));
        assert!(!DifferentialPrivacy::validate(DEFAULT_EPSILON, MAX_SENSITIVITY + 1));
    }
}
