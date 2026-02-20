//! FFI bindings to C entropy calculator.
//!
//! Demonstrates calling C code from Rust via FFI.
//! The C implementation lives in `c_src/entropy.c` and is compiled
//! by the `cc` crate in `build.rs`.

unsafe extern "C" {
    fn compute_shannon_entropy(data: *const u8, len: usize) -> f64;
}

/// Compute Shannon entropy of the given byte slice.
///
/// Returns a value between 0.0 (completely uniform, e.g. all zeros)
/// and 8.0 (maximally random, e.g. encrypted data).
///
/// Values above ~7.5 strongly suggest encrypted/compressed content —
/// a key ransomware detection signal.
pub fn shannon_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    // SAFETY: `data.as_ptr()` is valid for `data.len()` bytes, and
    // `compute_shannon_entropy` only reads within that range.
    unsafe { compute_shannon_entropy(data.as_ptr(), data.len()) }
}

/// Quick classification of entropy values for threat detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyLevel {
    Low,        // 0.0 - 4.0: structured data, text
    Normal,     // 4.0 - 6.5: typical documents, binaries
    High,       // 6.5 - 7.5: compressed data
    VeryHigh,   // 7.5 - 8.0: encrypted / random (ransomware indicator)
}

impl EntropyLevel {
    pub fn classify(entropy: f64) -> Self {
        match entropy {
            e if e < 4.0 => Self::Low,
            e if e < 6.5 => Self::Normal,
            e if e < 7.5 => Self::High,
            _ => Self::VeryHigh,
        }
    }

    pub fn is_suspicious(&self) -> bool {
        matches!(self, Self::VeryHigh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_data_has_zero_entropy() {
        assert_eq!(shannon_entropy(&[]), 0.0);
    }

    #[test]
    fn uniform_data_has_zero_entropy() {
        let data = vec![0u8; 1024];
        assert!(shannon_entropy(&data) < 0.001);
    }

    #[test]
    fn repeated_pattern_has_low_entropy() {
        let data: Vec<u8> = (0..1024).map(|i| (i % 4) as u8).collect();
        let e = shannon_entropy(&data);
        assert!(e > 1.9 && e < 2.1, "expected ~2.0 for 4-symbol pattern, got {e}");
    }

    #[test]
    fn text_has_moderate_entropy() {
        let data = b"The quick brown fox jumps over the lazy dog. \
                     This is a test of entropy calculation in Rust.";
        let e = shannon_entropy(data);
        assert!(e > 3.5 && e < 5.5, "expected moderate entropy for text, got {e}");
    }

    #[test]
    fn classify_entropy_levels() {
        assert_eq!(EntropyLevel::classify(1.0), EntropyLevel::Low);
        assert_eq!(EntropyLevel::classify(5.0), EntropyLevel::Normal);
        assert_eq!(EntropyLevel::classify(7.0), EntropyLevel::High);
        assert_eq!(EntropyLevel::classify(7.8), EntropyLevel::VeryHigh);
        assert!(EntropyLevel::VeryHigh.is_suspicious());
        assert!(!EntropyLevel::Normal.is_suspicious());
    }
}
