use crate::cpen442coin;
use std::time::{Duration, Instant};

pub struct Timer {
    start : Instant,
    period : Duration
}

impl Timer {
    pub fn new(period : Duration) -> Self {
        Timer {
            start : Instant::now(),
            period
        }
    }

    pub fn check_and_reset(&mut self) -> bool {
        if self.start.elapsed() > self.period {
            self.start = Instant::now();
            true
        } else {
            false
        }
    }

    pub fn check_and_reset_rt(&mut self) -> bool {
        if self.start.elapsed() > self.period {
            self.start += self.period;
            true
        } else {
            false
        }
    }

    //pub fn reset(&mut self) {
    //    self.start = Instant::now();
    //}
}

/// Check if the hash starts
/// for n zeroes difficulty. (n is in number of hex chars).
#[inline(always)]
pub fn hash_starts_n_zeroes(hash : &[u8], n : u64) -> bool {
    assert_eq!(hash.len(), cpen442coin::MD5_HASH_LEN);
    let n = n as usize;

    for i in 0..n / 2 {
        //println!("has_starts_n_zeroes hash[{}] = {}", i, hash[i]);
        if hash[i] != 0 {
            return false;
        }
    }

    if n % 2 == 1 && (hash[n / 2] & 0xF0) != 0 {
        //println!("has_starts_n_zeroes hash[{}] = {}", n / 2 + 1, hash[n/2 + 1]);
        return false;
    }

    true
}

#[inline(always)]
pub fn hex_starts_n_zeroes(hex : &str, n : u64) -> bool {
    assert_eq!(hex.len(), cpen442coin::MD5_HASH_HEX_LEN);


    for c in hex[0..n as usize].chars() {
        if c != '0' {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_hash_word2_mask_ok() {


    }

    #[test]
    fn test_hash_starts_n_zeros_ok() {
        let hash = hex::decode("000000000002330fd125c706950f913b").unwrap();

        println!("Hash: {:?}", hash);

        assert!(hash_starts_n_zeroes(&hash, 11));
    }

}
