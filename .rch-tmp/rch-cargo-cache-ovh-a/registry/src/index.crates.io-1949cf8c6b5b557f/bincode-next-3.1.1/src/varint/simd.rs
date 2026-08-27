#![allow(unsafe_code)]
#![allow(clippy::cast_ptr_alignment)]
#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::__m128i;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::_mm_cmpgt_epi8;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::_mm_loadu_si128;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::_mm_movemask_epi8;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::_mm_set1_epi8;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::_mm_xor_si128;

/// Results of a SIMD varint scan.
pub struct VarintScan {
    /// The number of consecutive single-byte varints found.
    pub single_byte_count: usize,
}

/// Scans a buffer for consecutive single-byte varints (0..=250).
/// This returns the number of bytes that can be safely read as individual varints.
#[inline(always)]
pub fn scan_single_byte_varints(bytes: &[u8]) -> VarintScan {
    let mut count = 0;

    #[cfg(feature = "simd")]
    #[cfg(target_arch = "x86_64")]
    {
        if bytes.len() >= 16 {
            unsafe {
                while count + 16 <= bytes.len() {
                    let chunk = _mm_loadu_si128(bytes.as_ptr().add(count).cast::<__m128i>());
                    // Trick: x <= 250 (unsigned) is equivalent to:
                    // (x ^ 0x80) <= (250 ^ 0x80) (signed)
                    // 250 ^ 0x80 = 250 - 128 = 122
                    let shifted = _mm_xor_si128(chunk, _mm_set1_epi8(-128));
                    let mask = _mm_cmpgt_epi8(shifted, _mm_set1_epi8(122));
                    let movemask = _mm_movemask_epi8(mask);

                    if movemask != 0 {
                        // Found a byte > 250.
                        count += movemask.trailing_zeros() as usize;
                        return VarintScan {
                            single_byte_count: count,
                        };
                    }
                    count += 16;
                }
            }
        }
    }

    #[cfg(feature = "simd")]
    #[cfg(target_arch = "aarch64")]
    {
        if bytes.len() >= 16 {
            unsafe {
                let limit = vdupq_n_u8(250);
                while count + 16 <= bytes.len() {
                    let chunk = vld1q_u8(bytes.as_ptr().add(count));
                    let mask = vcgtq_u8(chunk, limit);

                    if vmaxvq_u8(mask) != 0 {
                        // Found a byte > 250.
                        // Fallback loop will find the exact count.
                        break;
                    }
                    count += 16;
                }
            }
        }
    }

    #[cfg(all(feature = "simd", feature = "hw-acceleration"))]
    #[cfg(any(target_arch = "riscv64", target_arch = "riscv32"))]
    {
        if bytes.len() >= 16 {
            unsafe {
                let limit = 250u8;
                while count < bytes.len() {
                    let remaining = bytes.len() - count;
                    if remaining < 16 {
                        break;
                    }
                    let vl: usize;
                    let first: isize;
                    let ptr = bytes.as_ptr().add(count);

                    core::arch::asm!(
                        "vsetvli {vl}, {remaining}, e8, m1, ta, ma",
                        "vle8.v v8, ({ptr})",
                        "vmsgtu.vx v0, v8, {limit}",
                        "vfirst.m {first}, v0",
                        remaining = in(reg) remaining,
                        ptr = in(reg) ptr,
                        limit = in(reg) limit,
                        vl = out(reg) vl,
                        first = out(reg) first,
                        out("v0") _,
                        out("v8") _,
                    );

                    if first >= 0 {
                        count += first as usize;
                        return VarintScan {
                            single_byte_count: count,
                        };
                    }
                    count += vl;
                }
            }
        }
    }

    // Fallback for remaining or non-x86
    while count < bytes.len() && bytes[count] <= 250 {
        count += 1;
    }

    VarintScan {
        single_byte_count: count,
    }
}
