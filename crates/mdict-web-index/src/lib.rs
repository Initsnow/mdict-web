use std::collections::HashSet;
use std::fs;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fst::{Automaton, IntoStreamer, Map, MapBuilder, Streamer};
use mdict_rs::{Header, KeyOrdinal, MdxFile};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const INDEX_FORMAT_VERSION: u32 = 2;
const POSTING_ORDINAL_BYTES: usize = size_of::<u32>();
const KEYS_AT_BATCH_THRESHOLD: usize = 16;

#[derive(Debug)]
pub struct DictionarySuggestIndex {
    map: Map<Vec<u8>>,
    postings: Vec<u8>,
    metadata: DictionaryIndexMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionaryIndexMetadata {
    #[serde(default = "index_format_version")]
    pub format_version: u32,
    pub dictionary_id: String,
    pub mdx_size: u64,
    pub mdx_modified_millis: u128,
    pub entry_count: u64,
}

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("failed to access index files: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse index metadata: {0}")]
    Metadata(#[from] serde_json::Error),
    #[error("fst error: {0}")]
    Fst(#[from] fst::Error),
    #[error("failed to iterate mdx keys: {0}")]
    Mdict(#[from] mdict_rs::Error),
    #[error("index data is invalid: {0}")]
    InvalidData(String),
}

impl DictionarySuggestIndex {
    pub fn load_or_build(
        dictionary_id: &str,
        mdx_path: &Path,
        header: &Header,
        entry_count: u64,
        dir: &Path,
        rebuild_on_startup: bool,
        mdx: &MdxFile,
    ) -> Result<Self, IndexError> {
        fs::create_dir_all(dir)?;

        let expected = DictionaryIndexMetadata {
            format_version: index_format_version(),
            dictionary_id: dictionary_id.to_owned(),
            mdx_size: fs::metadata(mdx_path)?.len(),
            mdx_modified_millis: modified_millis(mdx_path)?,
            entry_count,
        };
        let paths = IndexPaths::new(dir, dictionary_id);

        if !rebuild_on_startup
            && paths.meta.exists()
            && paths.map.exists()
            && paths.postings.exists()
        {
            if let Ok(metadata) = fs::read(&paths.meta)
                .and_then(|bytes| serde_json::from_slice(&bytes).map_err(std::io::Error::other))
            {
                if metadata == expected {
                    if let Ok(index) = Self::load_from_files(&paths, metadata) {
                        return Ok(index);
                    }
                }
            }
        }

        Self::build_from_mdx(&paths, expected, header, mdx)
    }

    pub fn suggest(
        &self,
        mdx: &MdxFile,
        query: &str,
        case_sensitive: bool,
        strip_key: bool,
        limit: usize,
    ) -> Result<Vec<String>, IndexError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let normalized = normalize_key(query, case_sensitive, strip_key);
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let automaton = fst::automaton::Str::new(&normalized).starts_with();
        let mut stream = self.map.search(automaton).into_stream();
        let mut seen = HashSet::new();
        let mut out = Vec::with_capacity(limit);
        let mut pending = Vec::new();
        let batch_size = suggest_batch_size(limit);

        while let Some((_raw, packed_range)) = stream.next() {
            let (start, count) = unpack_posting_range(packed_range);
            for posting_index in start..start.saturating_add(count) {
                let ordinal =
                    posting_ordinal_at(&self.postings, posting_index).ok_or_else(|| {
                        IndexError::InvalidData(format!(
                            "posting index {posting_index} is out of range for suggest postings"
                        ))
                    })?;
                pending.push(KeyOrdinal::from(u64::from(ordinal)));
                if pending.len() >= batch_size {
                    hydrate_pending_keys(mdx, &mut pending, &mut seen, &mut out, limit)?;
                    if out.len() >= limit {
                        return Ok(out);
                    }
                }
            }
        }

        if !pending.is_empty() {
            hydrate_pending_keys(mdx, &mut pending, &mut seen, &mut out, limit)?;
        }

        Ok(out)
    }

    pub fn metadata(&self) -> &DictionaryIndexMetadata {
        &self.metadata
    }

    fn build_from_mdx(
        paths: &IndexPaths,
        metadata: DictionaryIndexMetadata,
        header: &Header,
        mdx: &MdxFile,
    ) -> Result<Self, IndexError> {
        let mut normalized_ordinals =
            Vec::with_capacity(metadata.entry_count.min(usize::MAX as u64) as usize);
        for entry in mdx.keys_with_ordinals() {
            let entry = entry?;
            let normalized = normalize_key(&entry.key, header.key_case_sensitive, header.strip_key);
            if normalized.is_empty() {
                continue;
            }
            let ordinal = u32::try_from(u64::from(entry.ordinal)).map_err(|_| {
                IndexError::InvalidData(format!(
                    "key ordinal {} exceeds u32 range",
                    entry.ordinal.get()
                ))
            })?;
            normalized_ordinals.push((normalized, ordinal));
        }
        normalized_ordinals
            .sort_unstable_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

        let mut map_bytes = Vec::new();
        let mut postings = Vec::with_capacity(normalized_ordinals.len() * POSTING_ORDINAL_BYTES);
        {
            let mut builder = MapBuilder::new(&mut map_bytes)?;
            let mut index = 0usize;
            while index < normalized_ordinals.len() {
                let normalized = normalized_ordinals[index].0.clone();
                let start =
                    u32::try_from(postings.len() / POSTING_ORDINAL_BYTES).map_err(|_| {
                        IndexError::InvalidData("suggest postings exceed u32 range".to_owned())
                    })?;
                let mut count = 0u32;
                while index < normalized_ordinals.len()
                    && normalized_ordinals[index].0 == normalized
                {
                    postings.extend_from_slice(&normalized_ordinals[index].1.to_le_bytes());
                    count = count.checked_add(1).ok_or_else(|| {
                        IndexError::InvalidData(format!(
                            "suggest postings for normalized key `{normalized}` exceed u32 range"
                        ))
                    })?;
                    index += 1;
                }
                builder.insert(&normalized, pack_posting_range(start, count))?;
            }
            builder.finish()?;
        }

        fs::write(&paths.map, &map_bytes)?;
        fs::write(&paths.postings, &postings)?;
        fs::write(&paths.meta, serde_json::to_vec_pretty(&metadata)?)?;

        Ok(Self {
            map: Map::new(map_bytes)?,
            postings,
            metadata,
        })
    }

    fn load_from_files(
        paths: &IndexPaths,
        metadata: DictionaryIndexMetadata,
    ) -> Result<Self, IndexError> {
        let map_bytes = fs::read(&paths.map)?;
        let postings = fs::read(&paths.postings)?;
        if postings.len() % POSTING_ORDINAL_BYTES != 0 {
            return Err(IndexError::InvalidData(format!(
                "suggest postings byte length {} is not aligned to {}",
                postings.len(),
                POSTING_ORDINAL_BYTES
            )));
        }
        Ok(Self {
            map: Map::new(map_bytes)?,
            postings,
            metadata,
        })
    }
}

pub fn normalize_key(key: &str, case_sensitive: bool, strip_key: bool) -> String {
    let trimmed = if strip_key {
        key.trim_matches(char::is_whitespace)
    } else {
        key
    };
    let without_nul = trimmed.trim_matches('\0');
    if case_sensitive {
        without_nul.to_owned()
    } else {
        without_nul.to_lowercase()
    }
}

fn modified_millis(path: &Path) -> Result<u128, std::io::Error> {
    let modified = fs::metadata(path)?
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH);
    Ok(modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis())
}

#[derive(Debug, Clone)]
struct IndexPaths {
    map: PathBuf,
    postings: PathBuf,
    meta: PathBuf,
}

impl IndexPaths {
    fn new(dir: &Path, dictionary_id: &str) -> Self {
        Self {
            map: dir.join(format!("{dictionary_id}.suggest.fst")),
            postings: dir.join(format!("{dictionary_id}.suggest.ordinals.bin")),
            meta: dir.join(format!("{dictionary_id}.suggest.meta.json")),
        }
    }
}

fn index_format_version() -> u32 {
    INDEX_FORMAT_VERSION
}

fn pack_posting_range(start: u32, count: u32) -> u64 {
    (u64::from(start) << 32) | u64::from(count)
}

fn unpack_posting_range(value: u64) -> (usize, usize) {
    (((value >> 32) as u32) as usize, (value as u32) as usize)
}

fn posting_ordinal_at(postings: &[u8], index: usize) -> Option<u32> {
    let start = index.checked_mul(POSTING_ORDINAL_BYTES)?;
    let end = start.checked_add(POSTING_ORDINAL_BYTES)?;
    let chunk = postings.get(start..end)?;
    Some(u32::from_le_bytes(chunk.try_into().ok()?))
}

fn suggest_batch_size(limit: usize) -> usize {
    limit.max(16).saturating_mul(4)
}

fn hydrate_pending_keys(
    mdx: &MdxFile,
    pending: &mut Vec<KeyOrdinal>,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
    limit: usize,
) -> Result<(), IndexError> {
    if pending.len() >= KEYS_AT_BATCH_THRESHOLD {
        let resolved = mdx.keys_at(pending)?;
        for (ordinal, maybe_key) in pending.iter().copied().zip(resolved.into_iter()) {
            let canonical = maybe_key.ok_or_else(|| {
                IndexError::InvalidData(format!(
                    "ordinal {} resolved to no key in source dictionary",
                    ordinal.get()
                ))
            })?;
            if seen.insert(canonical.clone()) {
                out.push(canonical);
            }
            if out.len() >= limit {
                break;
            }
        }
    } else {
        for ordinal in pending.iter().copied() {
            let canonical = mdx.key_at(ordinal)?.ok_or_else(|| {
                IndexError::InvalidData(format!(
                    "ordinal {} resolved to no key in source dictionary",
                    ordinal.get()
                ))
            })?;
            if seen.insert(canonical.clone()) {
                out.push(canonical);
            }
            if out.len() >= limit {
                break;
            }
        }
    }
    pending.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_key_respects_case_and_strip_flags() {
        assert_eq!(normalize_key(" Apple ", false, true), "apple");
        assert_eq!(normalize_key(" Apple ", true, false), " Apple ");
    }

    #[test]
    fn posting_range_round_trip_works() {
        let packed = pack_posting_range(42, 7);
        assert_eq!(unpack_posting_range(packed), (42, 7));
    }

    #[test]
    fn posting_ordinal_round_trip_works() {
        let mut postings = Vec::new();
        postings.extend_from_slice(&12u32.to_le_bytes());
        postings.extend_from_slice(&34u32.to_le_bytes());
        assert_eq!(posting_ordinal_at(&postings, 0), Some(12));
        assert_eq!(posting_ordinal_at(&postings, 1), Some(34));
        assert_eq!(posting_ordinal_at(&postings, 2), None);
    }

    #[test]
    fn suggest_batch_size_scales_with_limit() {
        assert_eq!(suggest_batch_size(1), 64);
        assert_eq!(suggest_batch_size(20), 80);
    }

    #[test]
    fn keys_at_batch_threshold_prefers_small_inline_path() {
        assert!(KEYS_AT_BATCH_THRESHOLD > 1);
    }
}
