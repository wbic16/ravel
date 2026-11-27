//! Ed25519 signature verification, in-tree.
//!
//! The daemon's authorization gate (see `crate::tessera`, a port of the
//! tessera crate's gate surface) checks Ed25519 tokens. tessera did that
//! through `ed25519-dalek`, whose tree cannot stay: curve25519-dalek's
//! proc-macro pulls a second `syn` major version into a tree that
//! otherwise carries one, and ashfall requires duplicate-free trees.
//! So the verification equation is implemented here, from RFC 8032 and
//! the field arithmetic of the curve, with `verify_strict` semantics
//! matched to ed25519-dalek 2.2.0 exactly:
//!
//! 1. the signature's `s` must be canonical (`s < L`, the group order);
//! 2. `R` (from the signature) and `A` (the verifying key) must both
//!    decompress, and neither may be a small-order point;
//! 3. with `k = SHA-512(R ‖ A ‖ M) mod L`, the check is the cofactorless
//!    equation `[s]B == R + [k]A`, compared on canonical encodings.
//!
//! Field elements are `[u64; 4]` integers mod `p = 2^255 − 19`, reduced
//! by folding at 2^256 (where `2^256 ≡ 38 (mod p)`). Nothing here is
//! constant-time; verification inputs (tokens, keys, signatures) are
//! public values, and this code never touches a secret.

use sha2::{Digest, Sha512};

/// A field element mod p, always kept fully reduced (`< p`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Fe([u64; 4]);

const P: [u64; 4] = [
    0xffffffffffffffed,
    0xffffffffffffffff,
    0xffffffffffffffff,
    0x7fffffffffffffff,
];

/// The group order L = 2^252 + 27742317777372353535851937790883648493.
const L: [u64; 4] = [
    0x5812631a5cf5d3ed,
    0x14def9dea2f79cd6,
    0x0000000000000000,
    0x1000000000000000,
];

fn cmp256(a: &[u64; 4], b: &[u64; 4]) -> std::cmp::Ordering {
    for i in (0..4).rev() {
        if a[i] != b[i] {
            return a[i].cmp(&b[i]);
        }
    }
    std::cmp::Ordering::Equal
}

/// `a + b` for values `< 2^256`, as a 5-limb result.
fn add_raw(a: &[u64; 4], b: &[u64; 4]) -> [u64; 5] {
    let mut out = [0u64; 5];
    let mut carry = 0u128;
    for i in 0..4 {
        let t = a[i] as u128 + b[i] as u128 + carry;
        out[i] = t as u64;
        carry = t >> 64;
    }
    out[4] = carry as u64;
    out
}

/// Reduce a 5-limb value (< 2^320, in practice < 2^262) mod p.
fn reduce5(mut v: [u64; 5]) -> Fe {
    // Fold everything above bit 255: 2^255 ≡ 19, so a limb at position
    // 4 (bits 256..) contributes limb * 38 at the bottom, and bit 255
    // itself contributes 19.
    loop {
        let hi = v[4];
        let bit255 = (v[3] >> 63) as u64;
        if hi == 0 && bit255 == 0 {
            break;
        }
        v[3] &= 0x7fffffffffffffff;
        v[4] = 0;
        let add = hi as u128 * 38 + bit255 as u128 * 19;
        let mut carry = add;
        for limb in v.iter_mut().take(4) {
            let t = *limb as u128 + carry;
            *limb = t as u64;
            carry = t >> 64;
        }
        v[4] = carry as u64;
    }
    let mut out = [v[0], v[1], v[2], v[3]];
    // Conditional subtractions: out < 2^255 + small, so a few passes of
    // "subtract p if >= p" fully reduce it.
    while cmp256(&out, &P) != std::cmp::Ordering::Less {
        out = sub_raw(&out, &P);
    }
    Fe(out)
}

/// `a - b` assuming `a >= b`.
fn sub_raw(a: &[u64; 4], b: &[u64; 4]) -> [u64; 4] {
    let mut out = [0u64; 4];
    let mut borrow = 0i128;
    for i in 0..4 {
        let t = a[i] as i128 - b[i] as i128 - borrow;
        if t < 0 {
            out[i] = (t + (1i128 << 64)) as u64;
            borrow = 1;
        } else {
            out[i] = t as u64;
            borrow = 0;
        }
    }
    out
}

impl Fe {
    fn zero() -> Fe {
        Fe([0; 4])
    }
    fn one() -> Fe {
        Fe([1, 0, 0, 0])
    }
    fn from_u64(x: u64) -> Fe {
        Fe([x, 0, 0, 0])
    }
    /// Load 32 little-endian bytes with the top bit masked off (a
    /// compressed point's y, or a field element mod 2^255). The value is
    /// `< 2^255 < 2p`, so one conditional subtraction reduces it.
    fn from_bytes_masked(b: &[u8; 32]) -> Fe {
        let mut v = [0u64; 4];
        for i in 0..4 {
            v[i] = u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
        }
        v[3] &= 0x7fffffffffffffff;
        if cmp256(&v, &P) != std::cmp::Ordering::Less {
            v = sub_raw(&v, &P);
        }
        Fe(v)
    }
    fn to_bytes(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..4 {
            out[i * 8..i * 8 + 8].copy_from_slice(&self.0[i].to_le_bytes());
        }
        out
    }
    fn add(&self, other: &Fe) -> Fe {
        reduce5(add_raw(&self.0, &other.0))
    }
    fn sub(&self, other: &Fe) -> Fe {
        // (a + 2p) - b stays non-negative and reduces to a - b.
        let two_p = add_raw(&P, &P);
        let mut a = [0u64; 5];
        a[..4].copy_from_slice(&self.0);
        let sum = {
            let mut s = [0u64; 5];
            let mut carry = 0u128;
            for i in 0..5 {
                let t = a[i] as u128 + two_p[i] as u128 + carry;
                s[i] = t as u64;
                carry = t >> 64;
            }
            s
        };
        let mut diff = [0u64; 5];
        let mut borrow = 0i128;
        for i in 0..5 {
            let b = if i < 4 { other.0[i] } else { 0 };
            let t = sum[i] as i128 - b as i128 - borrow;
            if t < 0 {
                diff[i] = (t + (1i128 << 64)) as u64;
                borrow = 1;
            } else {
                diff[i] = t as u64;
                borrow = 0;
            }
        }
        reduce5(diff)
    }
    fn neg(&self) -> Fe {
        Fe::zero().sub(self)
    }
    fn mul(&self, other: &Fe) -> Fe {
        // Schoolbook 4x4 into 8 limbs, then fold at 2^256 twice
        // (2^256 ≡ 38): hi parts shrink by 38 each fold.
        let mut t = [0u64; 8];
        for i in 0..4 {
            let mut carry = 0u128;
            for j in 0..4 {
                let cur = t[i + j] as u128 + self.0[i] as u128 * other.0[j] as u128 + carry;
                t[i + j] = cur as u64;
                carry = cur >> 64;
            }
            t[i + 4] = carry as u64;
        }
        // value = lo + hi * 2^256 ≡ lo + hi * 38
        let mut v = [0u64; 5];
        let mut carry = 0u128;
        for i in 0..4 {
            let cur = t[i] as u128 + t[i + 4] as u128 * 38 + carry;
            v[i] = cur as u64;
            carry = cur >> 64;
        }
        v[4] = carry as u64;
        reduce5(v)
    }
    fn sqr(&self) -> Fe {
        self.mul(self)
    }
    /// `self^e` for a 256-bit exponent given as 4 limbs (little-endian).
    fn pow(&self, e: &[u64; 4]) -> Fe {
        let mut result = Fe::one();
        let mut started = false;
        for i in (0..256).rev() {
            if started {
                result = result.sqr();
            }
            let bit = (e[i / 64] >> (i % 64)) & 1;
            if bit == 1 {
                if started {
                    result = result.mul(self);
                } else {
                    result = *self;
                    started = true;
                }
            }
        }
        if !started {
            return Fe::one();
        }
        result
    }
    fn invert(&self) -> Fe {
        // p - 2 = 2^255 - 21.
        const PM2: [u64; 4] = [
            0xffffffffffffffeb,
            0xffffffffffffffff,
            0xffffffffffffffff,
            0x7fffffffffffffff,
        ];
        self.pow(&PM2)
    }
    fn is_zero(&self) -> bool {
        self.0 == [0; 4]
    }
    fn is_odd(&self) -> bool {
        self.0[0] & 1 == 1
    }
}

/// The curve constant d = -121665/121666 mod p.
fn d() -> Fe {
    use std::sync::OnceLock;
    static D: OnceLock<Fe> = OnceLock::new();
    *D.get_or_init(|| Fe::from_u64(121665).neg().mul(&Fe::from_u64(121666).invert()))
}

/// sqrt(-1) mod p = 2^((p-1)/4).
fn sqrt_m1() -> Fe {
    use std::sync::OnceLock;
    static I: OnceLock<Fe> = OnceLock::new();
    *I.get_or_init(|| {
        // (p-1)/4 = 2^253 - 5.
        const E: [u64; 4] = [
            0xfffffffffffffffb,
            0xffffffffffffffff,
            0xffffffffffffffff,
            0x1fffffffffffffff,
        ];
        Fe::from_u64(2).pow(&E)
    })
}

/// A point in extended coordinates (X, Y, Z, T) with x = X/Z, y = Y/Z,
/// T = XY/Z. Curve: -x² + y² = 1 + d x² y².
#[derive(Clone, Copy)]
struct Point {
    x: Fe,
    y: Fe,
    z: Fe,
    t: Fe,
}

impl Point {
    fn identity() -> Point {
        Point {
            x: Fe::zero(),
            y: Fe::one(),
            z: Fe::one(),
            t: Fe::zero(),
        }
    }
    /// Hisil–Wong–Carter–Dawson addition (add-2008-hwcd, a = -1).
    fn add(&self, other: &Point) -> Point {
        let a = self.y.sub(&self.x).mul(&other.y.sub(&other.x));
        let b = self.y.add(&self.x).mul(&other.y.add(&other.x));
        let c = self.t.mul(&other.t).mul(&d()).mul(&Fe::from_u64(2));
        let dd = self.z.mul(&other.z).mul(&Fe::from_u64(2));
        let e = b.sub(&a);
        let f = dd.sub(&c);
        let g = dd.add(&c);
        let h = b.add(&a);
        Point {
            x: e.mul(&f),
            y: g.mul(&h),
            t: e.mul(&h),
            z: f.mul(&g),
        }
    }
    /// Doubling (dbl-2008-hwcd, a = -1).
    fn double(&self) -> Point {
        let a = self.x.sqr();
        let b = self.y.sqr();
        let c = self.z.sqr().mul(&Fe::from_u64(2));
        let dd = a.neg();
        let e = self.x.add(&self.y).sqr().sub(&a).sub(&b);
        let g = dd.add(&b);
        let f = g.sub(&c);
        let h = dd.sub(&b);
        Point {
            x: e.mul(&f),
            y: g.mul(&h),
            t: e.mul(&h),
            z: f.mul(&g),
        }
    }
    fn neg(&self) -> Point {
        Point {
            x: self.x.neg(),
            y: self.y,
            z: self.z,
            t: self.t.neg(),
        }
    }
    /// Scalar multiplication by a 256-bit scalar (little-endian limbs),
    /// double-and-add from the top bit. Not constant-time; the scalars
    /// here are hashes and signature values, both public.
    fn mul_scalar(&self, s: &[u64; 4]) -> Point {
        let mut r = Point::identity();
        for i in (0..256).rev() {
            r = r.double();
            if (s[i / 64] >> (i % 64)) & 1 == 1 {
                r = r.add(self);
            }
        }
        r
    }
    fn mul_by_cofactor(&self) -> Point {
        self.double().double().double()
    }
    fn is_small_order(&self) -> bool {
        self.mul_by_cofactor().encode() == Point::identity().encode()
    }
    /// The canonical compressed encoding: y's bytes, top bit = x odd.
    fn encode(&self) -> [u8; 32] {
        let zinv = self.z.invert();
        let x = self.x.mul(&zinv);
        let y = self.y.mul(&zinv);
        let mut out = y.to_bytes();
        if x.is_odd() {
            out[31] |= 0x80;
        }
        out
    }
    /// Decompress a canonical-or-not encoding, as curve25519-dalek
    /// does: y is read mod p (non-canonical y reduces), x is recovered
    /// per RFC 8032 §5.1.3, and an x of zero with the sign bit set is
    /// refused.
    fn decompress(bytes: &[u8; 32]) -> Option<Point> {
        let sign = bytes[31] >> 7 == 1;
        let y = Fe::from_bytes_masked(bytes);
        let yy = y.sqr();
        let u = yy.sub(&Fe::one());
        let v = d().mul(&yy).add(&Fe::one());
        let xx = u.mul(&v.invert());
        // x = xx^((p+3)/8); correct by sqrt(-1) if needed.
        const E: [u64; 4] = [
            0xfffffffffffffffe,
            0xffffffffffffffff,
            0xffffffffffffffff,
            0x0fffffffffffffff,
        ];
        let mut x = xx.pow(&E);
        if x.sqr() == xx.neg() {
            x = x.mul(&sqrt_m1());
        }
        if x.sqr() != xx {
            return None;
        }
        if x.is_zero() && sign {
            return None;
        }
        if x.is_odd() != sign {
            x = x.neg();
        }
        Some(Point {
            x,
            y,
            z: Fe::one(),
            t: x.mul(&y),
        })
    }
}

fn basepoint() -> Point {
    // The canonical compressed basepoint (RFC 8032 §5.1).
    const B: [u8; 32] = [
        0x58, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66,
    ];
    Point::decompress(&B).expect("the basepoint encoding is valid")
}

fn le_limbs(b: &[u8]) -> [u64; 4] {
    let mut v = [0u64; 4];
    for i in 0..4 {
        v[i] = u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
    }
    v
}

/// A 512-bit hash output reduced mod L, by bitwise long division
/// (512 rounds of shift-and-conditionally-subtract; verification is
/// not a hot path, and clarity beats speed here).
fn reduce_wide_mod_l(hash: &[u8; 64]) -> [u64; 4] {
    // Remainder in 5 limbs, always < 2L after each step.
    let mut rem = [0u64; 5];
    let l5 = [L[0], L[1], L[2], L[3], 0];
    for i in (0..512).rev() {
        // rem <<= 1, bring in bit i of the hash.
        let mut carry = (hash[i / 8] >> (i % 8)) & 1;
        for limb in rem.iter_mut() {
            let next = (*limb >> 63) as u8;
            *limb = (*limb << 1) | carry as u64;
            carry = next;
        }
        // If rem >= L, subtract L.
        let mut ge = true;
        for j in (0..5).rev() {
            if rem[j] != l5[j] {
                ge = rem[j] > l5[j];
                break;
            }
        }
        if ge {
            let mut borrow = 0i128;
            for j in 0..5 {
                let t = rem[j] as i128 - l5[j] as i128 - borrow;
                if t < 0 {
                    rem[j] = (t + (1i128 << 64)) as u64;
                    borrow = 1;
                } else {
                    rem[j] = t as u64;
                    borrow = 0;
                }
            }
        }
    }
    [rem[0], rem[1], rem[2], rem[3]]
}

/// Verify an Ed25519 signature with ed25519-dalek's `verify_strict`
/// semantics (see the module header for the exact rules).
///
/// `key` is the 32-byte verifying-key encoding, `signature` the 64-byte
/// signature (R ‖ s). Returns `Ok(())` only for a signature dalek's
/// `verify_strict` would accept.
pub fn verify_strict(key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> Result<(), ()> {
    let (r_bytes, s_bytes) = signature.split_at(32);
    let r_enc: &[u8; 32] = r_bytes.try_into().unwrap();
    let s = le_limbs(s_bytes);
    // 1. s must be canonical: s < L.
    if cmp256(&s, &L) != std::cmp::Ordering::Less {
        return Err(());
    }
    // 2. Both points decompress, and neither is small-order.
    let a_point = Point::decompress(key).ok_or(())?;
    let r_point = Point::decompress(r_enc).ok_or(())?;
    if r_point.is_small_order() || a_point.is_small_order() {
        return Err(());
    }
    // 3. k = SHA-512(R ‖ A ‖ M) mod L; check [s]B − [k]A encodes to R.
    let mut h = Sha512::new();
    h.update(r_enc);
    h.update(key);
    h.update(message);
    let digest: [u8; 64] = h.finalize().into();
    let k = reduce_wide_mod_l(&digest);
    let expected = basepoint()
        .mul_scalar(&s)
        .add(&a_point.mul_scalar(&k).neg());
    if expected.encode() == *r_enc {
        Ok(())
    } else {
        Err(())
    }
}

/// Whether a 32-byte encoding is a usable verifying key at all (it
/// decompresses) — the check ed25519-dalek's
/// `VerifyingKey::from_bytes` performs.
pub fn key_is_usable(key: &[u8; 32]) -> bool {
    Point::decompress(key).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// RFC 8032 §7.1 test vectors 1–2 (empty and 1-byte messages):
    /// a conforming signature must verify.
    #[test]
    fn rfc8032_vectors_verify() {
        let cases = [
            (
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
                "",
                "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
                 5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            ),
            (
                "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
                "72",
                "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da\
                 085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
            ),
        ];
        for (pk, msg, sig) in &cases {
            let pk: [u8; 32] = hex(pk).try_into().unwrap();
            let sig: [u8; 64] = hex(&sig.replace([' ', '\n'], "")).try_into().unwrap();
            assert!(verify_strict(&pk, &hex(msg), &sig).is_ok());
        }
        let _ = cases;
    }

    #[test]
    fn tampered_signatures_fail() {
        let pk: [u8; 32] = hex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
            .try_into()
            .unwrap();
        let sig: [u8; 64] = hex(
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
             5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
        )
        .try_into()
        .unwrap();
        // Wrong message.
        assert!(verify_strict(&pk, b"x", &sig).is_err());
        // Flipped bit in R and in s.
        let mut bad = sig;
        bad[0] ^= 1;
        assert!(verify_strict(&pk, b"", &bad).is_err());
        let mut bad = sig;
        bad[40] ^= 1;
        assert!(verify_strict(&pk, b"", &bad).is_err());
        // s = L (non-canonical) must fail even though it is s mod L = 0-ish.
        let mut bad = sig;
        bad[32..].copy_from_slice(&{
            let mut b = [0u8; 32];
            for i in 0..4 {
                b[i * 8..i * 8 + 8].copy_from_slice(&L[i].to_le_bytes());
            }
            b
        });
        assert!(verify_strict(&pk, b"", &bad).is_err());
    }

    #[test]
    fn small_order_keys_fail() {
        // The identity encoding (y = 1) as a key: a signature over it
        // must be refused as small-order whatever s claims.
        let mut identity = [0u8; 32];
        identity[0] = 1;
        let sig = [0u8; 64];
        assert!(verify_strict(&identity, b"", &sig).is_err());
        assert!(key_is_usable(&identity));
    }

    #[test]
    fn field_and_group_sanity() {
        // d matches its known decimal-derived encoding: compress the
        // basepoint back to its canonical bytes, and [L]B == identity.
        assert_eq!(basepoint().encode()[..], {
            const B: [u8; 32] = [
                0x58, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
                0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
                0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
            ];
            B
        }[..]);
        assert_eq!(
            basepoint().mul_scalar(&L).encode(),
            Point::identity().encode()
        );
        // (p-1) as a field element is -1: (-1)^2 == 1, -1 + 1 == 0.
        let neg1 = Fe::one().neg();
        assert_eq!(neg1.sqr(), Fe::one());
        assert_eq!(neg1.add(&Fe::one()), Fe::zero());
    }
}
