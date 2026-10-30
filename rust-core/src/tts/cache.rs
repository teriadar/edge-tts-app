use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::ffi::SynthesizedChunk;
use crate::tts::exceptions::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtsCacheStats {
    pub memory_entries: usize,
    pub disk_entries: usize,
    pub disk_bytes: u64,
}

const FIPS_SHA256_INIT: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];

const FIPS_SHA256_K: [u32; 64] = [
    0x428A2F98, 0x71374491, 0xB5C0FBCF, 0xE9B5DBA5, 0x3956C25B, 0x59F111F1, 0x923F82A4, 0xAB1C5ED5,
    0xD807AA98, 0x12835B01, 0x243185BE, 0x550C7DC3, 0x72BE5D74, 0x80DEB1FE, 0x9BDC06A7, 0xC19BF174,
    0xE49B69C1, 0xEFBE4786, 0x0FC19DC6, 0x240CA1CC, 0x2DE92C6F, 0x4A7484AA, 0x5CB0A9DC, 0x76F988DA,
    0x983E5152, 0xA831C66D, 0xB00327C8, 0xBF597FC7, 0xC6E00BF3, 0xD5A79147, 0x06CA6351, 0x14292967,
    0x27B70A85, 0x2E1B2138, 0x4D2C6DFC, 0x53380D13, 0x650A7354, 0x766A0ABB, 0x81C2C92E, 0x92722C85,
    0xA2BFE8A1, 0xA81A664B, 0xC24B8B70, 0xC76C51A3, 0xD192E819, 0xD6990624, 0xF40E3585, 0x106AA070,
    0x19A4C116, 0x1E376C08, 0x2748774C, 0x34B0BCB5, 0x391C0CB3, 0x4ED8AA4A, 0x5B9CCA4F, 0x682E6FF3,
    0x748F82EE, 0x78A5636F, 0x84C87814, 0x8CC70208, 0x90BEFFFA, 0xA4506CEB, 0xBEF9A3F7, 0xC67178F2,
];

pub fn sha256_hex(data: &[u8]) -> String {
    let bit_len = (data.len() as u64) * 8;
    let rem = (data.len() + 1) % 64;
    let pad_zeros = if rem <= 56 { 56 - rem } else { 120 - rem };

    let mut padded = Vec::with_capacity(data.len() + 1 + pad_zeros + 8);
    padded.extend_from_slice(data);
    padded.push(0x80);
    padded.resize(padded.len() + pad_zeros, 0u8);
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = FIPS_SHA256_INIT;

    for chunk in padded.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word_bytes) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes([word_bytes[0], word_bytes[1], word_bytes[2], word_bytes[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(FIPS_SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut hex = String::with_capacity(64);
    for word in state {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{:08x}", word);
    }
    hex
}

#[derive(Debug, Clone)]
pub struct TtsCache {
    cache_dir: Option<PathBuf>,
    memory: Arc<Mutex<HashMap<String, SynthesizedChunk>>>,
}

impl Default for TtsCache {
    fn default() -> Self {
        Self::new(None::<PathBuf>)
    }
}

impl TtsCache {

    pub fn new(cache_dir: Option<impl AsRef<Path>>) -> Self {
        let dir = cache_dir.map(|p| p.as_ref().to_path_buf());
        if let Some(ref d) = dir {
            let _ = fs::create_dir_all(d);
        }
        Self {
            cache_dir: dir,
            memory: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn cache_dir(&self) -> Option<&Path> {
        self.cache_dir.as_deref()
    }

    pub fn compute_key(text: &str, voice: &str, rate: &str, volume: &str, pitch: &str) -> String {
        let payload = format!("v1|{}|{}|{}|{}|{}", voice, rate, volume, pitch, text.trim());
        sha256_hex(payload.as_bytes())
    }

    pub fn get(&self, key: &str) -> Option<SynthesizedChunk> {
        if let Ok(guard) = self.memory.lock() {
            if let Some(chunk) = guard.get(key) {
                return Some(chunk.clone());
            }
        }

        let dir = self.cache_dir.as_ref()?;
        let json_path = dir.join(format!("{}.json", key));
        let mp3_path = dir.join(format!("{}.mp3", key));

        if json_path.exists() && mp3_path.exists() {
            let json_bytes = fs::read(&json_path).ok()?;
            let mut chunk: SynthesizedChunk = serde_json::from_slice(&json_bytes).ok()?;
            let mp3_bytes = fs::read(&mp3_path).ok()?;
            chunk.audio_mp3 = mp3_bytes;

            if let Ok(mut guard) = self.memory.lock() {
                guard.insert(key.to_string(), chunk.clone());
            }
            return Some(chunk);
        }

        None
    }

    pub fn put(&self, key: &str, chunk: &SynthesizedChunk) -> Result<()> {
        if let Ok(mut guard) = self.memory.lock() {
            guard.insert(key.to_string(), chunk.clone());
        }

        if let Some(ref dir) = self.cache_dir {
            fs::create_dir_all(dir)?;
            let mp3_path = dir.join(format!("{}.mp3", key));
            let json_path = dir.join(format!("{}.json", key));

            fs::write(&mp3_path, &chunk.audio_mp3)?;

            let mut meta_only = chunk.clone();
            meta_only.audio_mp3 = Vec::new();
            let json_bytes = serde_json::to_vec(&meta_only)?;
            fs::write(&json_path, json_bytes)?;
        }

        Ok(())
    }

    pub fn memory_len(&self) -> usize {
        self.memory.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn stats(&self) -> TtsCacheStats {
        let memory_entries = self.memory_len();
        let mut disk_entries = 0usize;
        let mut disk_bytes = 0u64;

        if let Some(ref dir) = self.cache_dir {
            if dir.exists() {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Ok(meta) = entry.metadata() {
                            if meta.is_file() {
                                disk_bytes += meta.len();
                                if path.extension().and_then(|e| e.to_str()) == Some("mp3") {
                                    disk_entries += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        TtsCacheStats {
            memory_entries,
            disk_entries,
            disk_bytes,
        }
    }

    pub fn prune_disk(&self, max_bytes: u64) -> Result<usize> {
        let Some(ref dir) = self.cache_dir else {
            return Ok(0);
        };
        if !dir.exists() {
            return Ok(0);
        }

        let mut mp3_files: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
        let mut total_bytes = 0u64;

        for entry in fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total_bytes += meta.len();
                    if path.extension().and_then(|e| e.to_str()) == Some("mp3") {
                        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                        mp3_files.push((path, meta.len(), modified));
                    }
                }
            }
        }

        if total_bytes <= max_bytes {
            return Ok(0);
        }

        mp3_files.sort_by_key(|(_, _, modified)| *modified);
        let mut evicted = 0usize;

        for (mp3_path, mp3_len, _) in mp3_files {
            if total_bytes <= max_bytes {
                break;
            }
            let json_path = mp3_path.with_extension("json");
            let json_len = fs::metadata(&json_path).map(|m| m.len()).unwrap_or(0);

            if let Some(stem) = mp3_path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(mut guard) = self.memory.lock() {
                    guard.remove(stem);
                }
            }

            let _ = fs::remove_file(&mp3_path);
            let _ = fs::remove_file(&json_path);
            total_bytes = total_bytes.saturating_sub(mp3_len + json_len);
            evicted += 1;
        }

        Ok(evicted)
    }

    pub fn clear(&self) -> Result<()> {
        if let Ok(mut guard) = self.memory.lock() {
            guard.clear();
        }
        if let Some(ref dir) = self.cache_dir {
            if dir.exists() {
                for entry in fs::read_dir(dir)?.flatten() {
                    let path = entry.path();
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "mp3" || ext == "json" {
                            let _ = fs::remove_file(path);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::WordAudioTiming;

    #[test]
    fn test_sha256_hex_standard_vector() {

        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_tts_cache_ram_and_disk_roundtrip() {
        let temp_dir = std::env::temp_dir().join("pagina_reader_test_cache");
        let cache = TtsCache::new(Some(&temp_dir));
        let _ = cache.clear();

        let key = TtsCache::compute_key("Ciao mondo", "it-IT-ElsaNeural", "+0%", "+0%", "+0Hz");
        let sample = SynthesizedChunk {
            paragraph_id: 2,
            chunk_index: 0,
            audio_mp3: vec![0xFF, 0xFB, 0x90, 0x64],
            word_timings: vec![WordAudioTiming {
                word_id: 1,
                paragraph_id: 2,
                text: "Ciao".to_string(),
                char_start: 0,
                char_end: 4,
                audio_offset_ms: 100,
                audio_duration_ms: 250,
                audio_end_ms: 350,
            }],
        };

        cache.put(&key, &sample).unwrap();

        let disk_reader_cache = TtsCache::new(Some(&temp_dir));
        let loaded = disk_reader_cache.get(&key).expect("must load from disk");
        assert_eq!(loaded, sample);

        let _ = disk_reader_cache.clear();
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cache_key_normalization_and_parameter_sensitivity() {
        let k1 = TtsCache::compute_key("  hello world! ", "en-US-AvaMultilingualNeural", "+0%", "+0%", "+0Hz");
        let k2 = TtsCache::compute_key("hello world!", "en-US-AvaMultilingualNeural", "+0%", "+0%", "+0Hz");
        assert_eq!(k1, k2);

        let k_diff_voice = TtsCache::compute_key("hello world!", "en-GB-RyanNeural", "+0%", "+0%", "+0Hz");
        let k_diff_pitch = TtsCache::compute_key("hello world!", "en-US-AvaMultilingualNeural", "+0%", "+0%", "-2Hz");
        assert_ne!(k1, k_diff_voice);
        assert_ne!(k1, k_diff_pitch);
    }

    #[test]
    fn test_tts_cache_prune_disk_enforces_byte_budget() {
        let temp_dir = std::env::temp_dir().join("pagina_reader_test_prune_cache");
        let cache = TtsCache::new(Some(&temp_dir));
        let _ = cache.clear();

        for idx in 0..4u32 {
            let key = TtsCache::compute_key(&format!("Paragraph chunk {}", idx), "en-US-AvaMultilingualNeural", "+0%", "+0%", "+0Hz");
            let chunk = SynthesizedChunk {
                paragraph_id: idx,
                chunk_index: 0,
                audio_mp3: vec![0xAA; 1024],
                word_timings: vec![],
            };
            cache.put(&key, &chunk).unwrap();
        }

        let stats_before = cache.stats();
        assert_eq!(stats_before.disk_entries, 4);
        assert!(stats_before.disk_bytes >= 4096);

        let evicted = cache.prune_disk(1500).unwrap();
        assert!(evicted >= 2);

        let stats_after = cache.stats();
        assert!(stats_after.disk_bytes <= 1500);

        let _ = cache.clear();
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
