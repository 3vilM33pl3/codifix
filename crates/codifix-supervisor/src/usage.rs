//! Provider-reported counters, not a price schedule or proof of provider identity.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
}

impl ProviderUsage {
    pub fn validate(&self) -> bool {
        self.input_tokens.checked_add(self.output_tokens) == Some(self.total_tokens)
            && self
                .cached_tokens
                .checked_add(self.cache_write_tokens)
                .is_some_and(|n| n <= self.input_tokens)
            && self.reasoning_tokens <= self.output_tokens
    }

    /// Fixed storage order. Never reconstruct unknown legacy counters as zero.
    pub fn counters(self) -> [u64; 6] {
        [
            self.input_tokens,
            self.output_tokens,
            self.total_tokens,
            self.cached_tokens,
            self.cache_write_tokens,
            self.reasoning_tokens,
        ]
    }

    pub fn from_counters(values: [u64; 6]) -> Option<Self> {
        let [
            input_tokens,
            output_tokens,
            total_tokens,
            cached_tokens,
            cache_write_tokens,
            reasoning_tokens,
        ] = values;
        let usage = Self {
            input_tokens,
            output_tokens,
            total_tokens,
            cached_tokens,
            cache_write_tokens,
            reasoning_tokens,
        };
        usage.validate().then_some(usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counters_are_checked_without_overflow_or_double_counting() {
        for values in [
            [100, 20, 120, 10, 20, 5],
            [0; 6],
            [u64::MAX, 0, u64::MAX, 0, 0, 0],
        ] {
            assert_eq!(
                ProviderUsage::from_counters(values)
                    .expect("valid")
                    .counters(),
                values
            );
        }
        for values in [
            [100, 20, 121, 0, 0, 0],
            [100, 20, 120, 90, 20, 0],
            [100, 20, 120, 0, 0, 21],
            [u64::MAX, 1, 0, 0, 0, 0],
            [u64::MAX, 0, u64::MAX, u64::MAX, 1, 0],
        ] {
            assert!(ProviderUsage::from_counters(values).is_none());
        }
    }
}
