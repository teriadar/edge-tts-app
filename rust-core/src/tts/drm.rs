use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::DateTime;
use rand::RngCore;

use crate::tts::constants::TRUSTED_CLIENT_TOKEN;
use crate::tts::exceptions::{EdgeTtsError, Result};

pub const WIN_EPOCH: f64 = 11_644_473_600.0;
pub const S_TO_NS: f64 = 1e9;

const SHA256_INITIAL: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];

const SHA256_STATE: [u32; 8] = [
    0xFD9FBEA2, 0x91243CE7, 0xBE499094, 0xCD2E1E0A, 0x6C006F49, 0x04BD35AB, 0xFEB0A7E4, 0x03EFFBD2,
];

const SHA256_SCHEDULE: [u32; 14] = [
    0xEB24BF9E, 0x1558691F, 0xA0C09EE2, 0x0CFA32BA, 0xA622EE8C, 0xD1DD9491, 0xC0DD25A0, 0xF9E70AB6,
    0xF0C31573, 0x7058CE4E, 0x7188E606, 0x00E5AB33, 0xD258B060, 0x72766564,
];

const SHA256_ROUND_CONSTANTS: [u32; 64] = [
    0x428A2F98, 0x71374491, 0xB5C0FBCF, 0xE9B5DBA5, 0x3956C25B, 0x59F111F1, 0x923F82A4, 0xAB1C5ED5,
    0xD807AA98, 0x12835B01, 0x243185BE, 0x550C7DC3, 0x72BE5D74, 0x80DEB1FE, 0x9BDC06A7, 0xC19BF174,
    0xE49B69C1, 0xEFBE4786, 0x0FC19DC6, 0x240CA1CC, 0x2DE92C6F, 0x4A7484AA, 0x5CB0A9DC, 0x76F988DA,
    0x983E5152, 0xA831C66D, 0xB00327C8, 0xBF597FC7, 0xC6E00BF3, 0xD5A79147, 0x06CA6351, 0x14292967,
    0x27B70A85, 0x2E1B2138, 0x4D2C6DFC, 0x53380D13, 0x650A7354, 0x766A0ABB, 0x81C2C92E, 0x92722C85,
    0xA2BFE8A1, 0xA81A664B, 0xC24B8B70, 0xC76C51A3, 0xD192E819, 0xD6990624, 0xF40E3585, 0x106AA070,
    0x19A4C116, 0x1E376C08, 0x2748774C, 0x34B0BCB5, 0x391C0CB3, 0x4ED8AA4A, 0x5B9CCA4F, 0x682E6FF3,
    0x748F82EE, 0x78A5636F, 0x84C87814, 0x8CC70208, 0x90BEFFFA, 0xA4506CEB, 0xBEF9A3F7, 0xC67178F2,
];

const SHA256_ADJUSTMENT: u32 = 0xC9DC06A7;

const SHA256_BLOCK_CONSTANTS: [u32; 64] = {
    let mut arr = SHA256_ROUND_CONSTANTS;
    arr[14] = SHA256_ADJUSTMENT;
    arr
};

const SHA256_SIGMA0_ADJUSTMENT: u32 = 0x059C0B80;
const SHA256_SIGMA1_ADJUSTMENT: u32 = 0x000B92C0;

const CS64_COEFFICIENTS: [(u32, u32); 14] = [
    (0xE82E91B1, 0xB32700D0),
    (0x44DE24C6, 0x88DFF51D),
    (0x3BEB65BC, 0x29514038),
    (0xC89C5933, 0xB328CBDF),
    (0x8D1EC211, 0xD405B4D5),
    (0x314EF5D5, 0x26E55F3C),
    (0xE92491B1, 0x607801C6),
    (0xF0A5B9D5, 0x82B2C1FB),
    (0x06064CF3, 0xFA51A3DA),
    (0xDA79FA60, 0x207CC464),
    (0xBED20560, 0x3C3C74F3),
    (0x89F9C9F8, 0xD2D14B61),
    (0x35FA38C6, 0xBB04A2C1),
    (0xCB49E3C6, 0xC5626167),
];

const CS64_FACTOR: u32 = 0x82893933;
const CS64_ADJUSTMENT0: u32 = 0xF2F90B1A;
const CS64_ADJUSTMENT1: u32 = 0x0B1A0000;

static CLOCK_SKEW_BITS: AtomicU64 = AtomicU64::new(0);

#[inline]
fn rol16(value: u32) -> u32 {
    value.rotate_left(16)
}

fn words_le4_even(data: &[u8]) -> Vec<u32> {
    if data.is_empty() {
        return Vec::new();
    }
    let rem = data.len() % 4;
    let pad_len = if rem == 0 { 0 } else { 4 - rem };
    let mut padded = Vec::with_capacity(data.len() + pad_len);
    padded.extend_from_slice(data);
    padded.resize(data.len() + pad_len, 0u8);

    let mut words: Vec<u32> = padded
        .as_chunks::<4>().0.iter()
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    if words.len() % 2 == 1 {
        words.push(0);
    }
    words
}

#[inline]
fn sigma0(value: u32) -> u32 {
    value.rotate_right(7) ^ value.rotate_right(18) ^ (value >> 3)
}

#[inline]
fn sigma1(value: u32) -> u32 {
    value.rotate_right(17) ^ value.rotate_right(19) ^ (value >> 10)
}

fn sha256_rounds(state: [u32; 8], words: &[u32; 64], first_round: usize) -> [u32; 8] {
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
    let constants = if first_round == 14 {
        &SHA256_BLOCK_CONSTANTS
    } else {
        &SHA256_ROUND_CONSTANTS
    };

    for index in first_round..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = g ^ (e & (f ^ g));
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(constants[index])
            .wrapping_add(words[index]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) | (c & (a | b));
        let t2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }

    [a, b, c, d, e, f, g, h]
}

fn expand_sha256_schedule(words: &mut [u32; 64], first_word: usize) {
    for index in first_word..64 {
        let s1 = sigma1(words[index - 2]);
        let s0 = sigma0(words[index - 15]);
        words[index] = s1
            .wrapping_add(words[index - 7])
            .wrapping_add(s0)
            .wrapping_add(words[index - 16]);
    }
}

fn sha256_init(data: &[u8]) -> [u32; 8] {
    let mut words = [0u32; 64];
    words[14] = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32);
    words[15] = u32::from_be_bytes([data[3], data[4], data[5], data[6]]);

    for index in 16..30 {
        let mut value = SHA256_SCHEDULE[index - 16];
        if index - 2 >= 14 {
            let s1 = if index == 16 {
                sigma1(words[index - 2]) ^ SHA256_SIGMA1_ADJUSTMENT
            } else {
                sigma1(words[index - 2])
            };
            value = value.wrapping_add(s1);
        }
        if index - 7 >= 14 {
            value = value.wrapping_add(words[index - 7]);
        }
        if index - 15 >= 14 {
            let s0 = if index == 29 {
                sigma0(words[index - 15]) ^ SHA256_SIGMA0_ADJUSTMENT
            } else {
                sigma0(words[index - 15])
            };
            value = value.wrapping_add(s0);
        }
        words[index] = value;
    }

    words[30] = sigma1(words[28])
        .wrapping_add(words[23])
        .wrapping_add(sigma0(words[15]))
        .wrapping_add(words[14])
        .wrapping_add(SHA256_ADJUSTMENT)
        .wrapping_sub(SHA256_ROUND_CONSTANTS[14]);

    expand_sha256_schedule(&mut words, 31);
    let working = sha256_rounds(SHA256_STATE, &words, 14);

    let mut out = [0u32; 8];
    for i in 0..8 {
        out[i] = SHA256_INITIAL[i].wrapping_add(working[i]);
    }
    out
}

pub fn sha256_digest(data: &[u8]) -> [u8; 32] {
    assert!(
        data.len() >= 7,
        "Token salt must contain at least seven bytes"
    );
    let length = 57 + data.len();
    let pad_zeros = (55isize - (length as isize)).rem_euclid(64) as usize;

    let mut padded = Vec::with_capacity(data.len() - 7 + 1 + pad_zeros + 8);
    padded.extend_from_slice(&data[7..]);
    padded.push(0x80);
    padded.resize(padded.len() + pad_zeros, 0u8);
    padded.extend_from_slice(&((length as u64) * 8).to_be_bytes());

    let mut state = sha256_init(&data[..7]);
    for chunk in padded.as_chunks::<64>().0 {
        let mut words = [0u32; 64];
        for (i, w) in chunk.as_chunks::<4>().0.iter().enumerate() {
            words[i] = u32::from_be_bytes([w[0], w[1], w[2], w[3]]);
        }
        expand_sha256_schedule(&mut words, 16);
        let working = sha256_rounds(state, &words, 0);
        for i in 0..8 {
            state[i] = state[i].wrapping_add(working[i]);
        }
    }

    let mut digest = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        digest[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

pub fn cs64_digest(data: &[u8], digest: &[u8; 32]) -> (u32, u32) {
    let mut key_a = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
    let mut key_b = u32::from_le_bytes([digest[4], digest[5], digest[6], digest[7]]);
    key_a |= 1;
    key_b |= 1;

    let factors = [
        key_a.wrapping_mul(CS64_FACTOR),
        key_b.wrapping_mul(CS64_FACTOR),
    ];

    let mut terms: Vec<u32> = CS64_COEFFICIENTS
        .iter()
        .enumerate()
        .map(|(idx, &(coeff, _))| coeff.wrapping_mul(factors[idx % 2]))
        .collect();

    let mut offsets: Vec<u32> = CS64_COEFFICIENTS
        .iter()
        .map(|&(_, offset)| offset)
        .collect();

    let mut prefixed = Vec::with_capacity(1 + data.len());
    prefixed.push(0u8);
    prefixed.extend_from_slice(data);

    for (index, word) in words_le4_even(&prefixed).into_iter().enumerate() {
        let k = if index % 2 == 0 { key_a } else { key_b };
        let mut term = (word as u64).wrapping_mul(k as u64);
        let mult = if index % 2 == 0 {
            0xEF0569FBu64
        } else {
            0xC31713DBu64
        };
        let mut offset = (rol16(word) as u64).wrapping_mul(mult);
        if index == 0 {
            term = term.wrapping_add((CS64_ADJUSTMENT0 as u64).wrapping_mul(factors[0] as u64));
            offset = offset.wrapping_add(CS64_ADJUSTMENT1 as u64);
        }
        terms.push(term as u32);
        offsets.push(offset as u32);
    }

    let mut swap_acc = 0u32;
    let mut swap_last = 0u32;
    let mut state = 0u32;
    let mut acc = 0u32;

    for index in (0..terms.len()).step_by(2) {
        let x = terms[index];
        let y = terms[index + 1];

        let t = rol16(offsets[index].wrapping_add(x));
        let q = t.wrapping_mul(0x76F50F34);
        swap_acc = swap_acc.wrapping_add(rol16(q).wrapping_mul(0xC3EFEA97).wrapping_add(q));

        let t2 = rol16(offsets[index + 1].wrapping_add(y));
        let q2 = t2.wrapping_mul(0x1881CE3C);
        swap_last = rol16(q2).wrapping_mul(0x35BD1EC9).wrapping_add(q2);
        swap_acc = swap_acc.wrapping_add(swap_last);

        state = state.wrapping_mul(key_a).wrapping_add(x);
        state = rol16(state).wrapping_mul(0xCF98B111);
        state = rol16(state).wrapping_mul(0x87085B9F);
        state = rol16(state).wrapping_mul(0x12CEB96D);
        state = rol16(state).wrapping_mul(0x257E1D83);
        acc = acc.wrapping_add(state);

        let mut t3 = state.wrapping_mul(key_b).wrapping_add(y);
        t3 = rol16(t3).wrapping_mul(0xA27416F5);
        t3 = rol16(t3).wrapping_mul(0xD38396FF);
        state = rol16(rol16(t3).wrapping_mul(0x7C932B89)).wrapping_mul(0xBFA49F69);
        acc = acc.wrapping_add(state);
    }

    (swap_last ^ state, swap_acc ^ acc)
}

fn hex_upper(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut s, "{:02X}", b);
    }
    s
}

pub fn token_hash(data: &[u8]) -> (String, String) {
    let digest = sha256_digest(data);
    let (w0, w1) = cs64_digest(data, &digest);
    let mut cs64_bytes = [0u8; 8];
    cs64_bytes[..4].copy_from_slice(&w0.to_le_bytes());
    cs64_bytes[4..].copy_from_slice(&w1.to_le_bytes());
    (hex_upper(&cs64_bytes), hex_upper(&digest))
}

pub fn generate_sec_ms_gec_v2(unix_timestamp: Option<f64>) -> String {
    let mut ticks = unix_timestamp.unwrap_or_else(Drm::get_unix_timestamp);
    ticks += WIN_EPOCH;
    ticks -= ticks % 300.0;
    ticks *= S_TO_NS / 100.0;
    let str_to_hash = format!("{:.0}{}", ticks, TRUSTED_CLIENT_TOKEN);
    let (cs64, sha256) = token_hash(str_to_hash.as_bytes());
    format!("{}-{}", cs64, sha256)
}

pub struct Drm;

impl Drm {

    pub fn clock_skew_seconds() -> f64 {
        f64::from_bits(CLOCK_SKEW_BITS.load(Ordering::Relaxed))
    }

    pub fn adj_clock_skew_seconds(skew_seconds: f64) {
        let mut current = CLOCK_SKEW_BITS.load(Ordering::Relaxed);
        loop {
            let next = (f64::from_bits(current) + skew_seconds).to_bits();
            match CLOCK_SKEW_BITS.compare_exchange_weak(
                current,
                next,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn get_unix_timestamp() -> f64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        now + Self::clock_skew_seconds()
    }

    pub fn parse_rfc2616_date(date: &str) -> Option<f64> {
        DateTime::parse_from_rfc2822(date)
            .ok()
            .map(|dt| dt.timestamp() as f64 + (dt.timestamp_subsec_nanos() as f64) / 1e9)
    }

    pub fn handle_client_response_error(server_date_header: Option<&str>) -> Result<()> {
        let server_date = server_date_header.ok_or_else(|| {
            EdgeTtsError::SkewAdjustmentError("No server date in headers.".to_string())
        })?;
        let server_date_parsed = Self::parse_rfc2616_date(server_date).ok_or_else(|| {
            EdgeTtsError::SkewAdjustmentError(format!(
                "Failed to parse server date: {}",
                server_date
            ))
        })?;
        let client_date = Self::get_unix_timestamp();
        Self::adj_clock_skew_seconds(server_date_parsed - client_date);
        Ok(())
    }

    pub fn generate_sec_ms_gec() -> String {
        generate_sec_ms_gec_v2(None)
    }

    pub fn generate_muid() -> String {
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        hex_upper(&bytes)
    }

    pub fn headers_with_muid(headers: &HashMap<String, String>) -> HashMap<String, String> {
        let mut combined = headers.clone();
        assert!(!combined.contains_key("Cookie"));
        combined.insert(
            "Cookie".to_string(),
            format!("muid={};", Self::generate_muid()),
        );
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_sec_ms_gec_v2_matches_python_reference() {
        assert_eq!(
            generate_sec_ms_gec_v2(Some(1_700_000_000.0)),
            "2CD23A2CA0B2595A-F3C846C370E05F9CC83ACC5F995C5F5D87A4D0F14CE8936A0F902E64099E38A2"
        );
        assert_eq!(
            generate_sec_ms_gec_v2(Some(1_729_110_000.0)),
            "D9E0488BC4950B78-5548A1A263CBF97997518B9BC924F084E576FF530FBEE79AF1CDA23A05387F6D"
        );
    }

    #[test]
    fn test_parse_rfc2616_date() {
        let ts = Drm::parse_rfc2616_date("Wed, 16 Oct 2024 20:00:00 GMT");
        assert!(ts.is_some());
        assert!((ts.unwrap() - 1_729_108_800.0).abs() < 1.0);
    }
}
