//! Unsigned n-bit arithmetic, implemented as digital algorithms rather than host ALU calls.
pub fn mask(width: u8) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

pub fn full_adder(a: bool, b: bool, carry: bool) -> (bool, bool) {
    (a ^ b ^ carry, (a & b) | ((a ^ b) & carry))
}

pub fn add(a: u64, b: u64, width: u8, carry_in: bool) -> (u64, bool) {
    let mut result = 0;
    let mut carry = carry_in;
    for bit in 0..width {
        let (sum, next) = full_adder((a >> bit) & 1 != 0, (b >> bit) & 1 != 0, carry);
        result |= u64::from(sum) << bit;
        carry = next;
    }
    (result, carry)
}

/// A + NOT(B) + 1; second output means unsigned borrow.
pub fn sub(a: u64, b: u64, width: u8) -> (u64, bool) {
    let (result, carry) = add(a, !b & mask(width), width, true);
    (result, !carry)
}

/// Shift-and-add, retaining a double-width product to detect overflow.
pub fn mul(a: u64, b: u64, width: u8) -> (u64, bool) {
    let a = u128::from(a & mask(width));
    let b = b & mask(width);
    let mut product = 0u128;
    for bit in 0..width {
        if b & (1 << bit) != 0 {
            product += a << bit;
        }
    }
    ((product as u64) & mask(width), product >> width != 0)
}

/// Restoring division. Returns quotient and remainder; None is division by zero.
pub fn div(a: u64, b: u64, width: u8) -> Option<(u64, u64)> {
    let a = a & mask(width);
    let b = u128::from(b & mask(width));
    if b == 0 {
        return None;
    }
    let mut quotient = 0u64;
    let mut remainder = 0u128;
    for bit in (0..width).rev() {
        remainder = (remainder << 1) | u128::from((a >> bit) & 1);
        if remainder >= b {
            remainder -= b;
            quotient |= 1 << bit;
        }
    }
    Some((quotient, remainder as u64))
}
