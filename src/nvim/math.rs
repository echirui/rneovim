/// Neovim's xpopcount re-implemented in idiomatic Rust.
pub fn xpopcount(x: u64) -> u32 {
    x.count_ones()
}

/// Neovim's xctz re-implemented in idiomatic Rust.
pub fn xctz(x: u64) -> u32 {
    if x == 0 {
        (8 * std::mem::size_of::<u64>()) as u32
    } else {
        x.trailing_zeros()
    }
}

/// Neovim's trim_to_int re-implemented in idiomatic Rust.
pub fn trim_to_int(x: i64) -> i32 {
    x.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// For overflow detection, add a digit safely to an int value.
/// Returns Ok(()) on success, Err(()) on overflow.
pub fn vim_append_digit_int(value: &mut i32, digit: i32) -> Result<(), ()> {
    match value.checked_mul(10).and_then(|v| v.checked_add(digit)) {
        Some(new_value) => {
            *value = new_value;
            Ok(())
        }
        None => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xpopcount_basic() {
        assert_eq!(xpopcount(0b1111), 4);
    }

    #[test]
    fn test_xpopcount_zero() {
        assert_eq!(xpopcount(0b0), 0);
    }

    #[test]
    fn test_xctz_basic() {
        assert_eq!(xctz(0b1000), 3);
    }

    #[test]
    fn test_xctz_one() {
        assert_eq!(xctz(0b1), 0);
    }

    #[test]
    fn test_xctz_zero() {
        assert_eq!(xctz(0b0), 64);
    }

    #[test]
    fn test_trim_to_int_max() {
        assert_eq!(trim_to_int(i64::MAX), i32::MAX);
    }

    #[test]
    fn test_trim_to_int_min() {
        assert_eq!(trim_to_int(i64::MIN), i32::MIN);
    }

    #[test]
    fn test_trim_to_int_normal() {
        assert_eq!(trim_to_int(42), 42);
    }

    #[test]
    fn test_vim_append_digit_int_basic() {
        let mut val = 10;
        assert!(vim_append_digit_int(&mut val, 5).is_ok());
        assert_eq!(val, 105);
    }

    #[test]
    fn test_vim_append_digit_int_limit() {
        let mut val = 214748364; // (i32::MAX - 7) / 10
        assert!(vim_append_digit_int(&mut val, 7).is_ok());
        assert_eq!(val, i32::MAX);
    }

    #[test]
    fn test_vim_append_digit_int_overflow() {
        let mut val = 214748364; 
        assert!(vim_append_digit_int(&mut val, 8).is_err());
    }

    #[test]
    fn test_vim_append_digit_int_max_overflow() {
        let mut val = i32::MAX;
        assert!(vim_append_digit_int(&mut val, 0).is_err());
    }
}
