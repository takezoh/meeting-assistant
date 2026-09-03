//! Retry classification and backoff: 1 s, 4 s, 16 s, 64 s, 256 s, at most five attempts.

pub const BACKOFF_SCHEDULE_MS: [u64; 5] = [1_000, 4_000, 16_000, 64_000, 256_000];
pub const MAX_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    Retryable,
    Permanent,
}

/// Delay after `attempts` failed attempts, or `None` when the step is exhausted.
pub fn backoff_ms(attempts: u32) -> Option<u64> {
    if attempts == 0 || attempts >= MAX_ATTEMPTS {
        return None;
    }
    BACKOFF_SCHEDULE_MS.get(attempts as usize - 1).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_is_exponential_and_capped() {
        assert_eq!(backoff_ms(1), Some(1_000));
        assert_eq!(backoff_ms(2), Some(4_000));
        assert_eq!(backoff_ms(4), Some(64_000));
        assert_eq!(backoff_ms(5), None);
    }
}
