use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fst::{Automaton, IntoStreamer, Streamer};
use mdict_rs::{Header, MdxFile};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const KEY_SEPARATOR: char = '\u{1f}';

#[derive(Debug)]
pub struct DictionarySuggestIndex {
    set: fst::Set<Vec<u8>>,
    metadata: DictionaryIndexMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionaryIndexMetadata {
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
    #[error("failed to iterate mdx entries: {0}")]
    Mdict(#[from] mdict_rs::Error),
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
            dictionary_id: dictionary_id.to_owned(),
            mdx_size: fs::metadata(mdx_path)?.len(),
            mdx_modified_millis: modified_millis(mdx_path)?,
            entry_count,
        };
        let paths = IndexPaths::new(dir, dictionary_id);

        if !rebuild_on_startup && paths.meta.exists() && paths.set.exists() {
            let metadata: DictionaryIndexMetadata =
                serde_json::from_slice(&fs::read(&paths.meta)?)?;
            if metadata == expected {
                return Self::load_from_files(paths, metadata);
            }
        }

        Self::build_from_mdx(paths, expected, header, mdx)
    }

    pub fn suggest(
        &self,
        query: &str,
        case_sensitive: bool,
        strip_key: bool,
        limit: usize,
    ) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }

        let normalized = normalize_key(query, case_sensitive, strip_key);
        if normalized.is_empty() {
            return Vec::new();
        }

        let automaton = fst::automaton::Str::new(&normalized).starts_with();
        let mut stream = self.set.search(automaton).into_stream();
        let mut seen = HashSet::new();
        let mut out = Vec::with_capacity(limit);

        while let Some(raw) = stream.next() {
            let Ok(composite) = std::str::from_utf8(raw) else {
                continue;
            };
            let Some((_, canonical)) = split_composite_key(composite) else {
                continue;
            };
            if seen.insert(canonical.to_owned()) {
                out.push(canonical.to_owned());
            }
            if out.len() >= limit {
                break;
            }
        }

        out
    }

    pub fn metadata(&self) -> &DictionaryIndexMetadata {
        &self.metadata
    }

    fn build_from_mdx(
        paths: IndexPaths,
        metadata: DictionaryIndexMetadata,
        header: &Header,
        mdx: &MdxFile,
    ) -> Result<Self, IndexError> {
        let mut composite_keys = Vec::with_capacity(metadata.entry_count as usize);
        for entry in mdx.entries() {
            let entry = entry?;
            let normalized = normalize_key(&entry.key, header.key_case_sensitive, header.strip_key);
            composite_keys.push(compose_key(&normalized, &entry.key));
        }
        composite_keys.sort_unstable();
        composite_keys.dedup();

        let mut bytes = Vec::new();
        {
            let mut builder = fst::SetBuilder::new(&mut bytes)?;
            for key in &composite_keys {
                builder.insert(key)?;
            }
            builder.finish()?;
        }

        fs::write(&paths.set, &bytes)?;
        fs::write(&paths.meta, serde_json::to_vec_pretty(&metadata)?)?;

        Ok(Self {
            set: fst::Set::new(bytes)?,
            metadata,
        })
    }

    fn load_from_files(
        paths: IndexPaths,
        metadata: DictionaryIndexMetadata,
    ) -> Result<Self, IndexError> {
        let bytes = fs::read(paths.set)?;
        Ok(Self {
            set: fst::Set::new(bytes)?,
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

fn compose_key(normalized: &str, canonical: &str) -> String {
    format!("{normalized}{KEY_SEPARATOR}{canonical}")
}

fn split_composite_key(input: &str) -> Option<(&str, &str)> {
    input.split_once(KEY_SEPARATOR)
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
    set: PathBuf,
    meta: PathBuf,
}

impl IndexPaths {
    fn new(dir: &Path, dictionary_id: &str) -> Self {
        Self {
            set: dir.join(format!("{dictionary_id}.suggest.fst")),
            meta: dir.join(format!("{dictionary_id}.suggest.meta.json")),
        }
    }
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
    fn composite_key_round_trip_works() {
        let composite = compose_key("apple", "Apple");
        let parsed = split_composite_key(&composite).expect("composite key should parse");
        assert_eq!(parsed.0, "apple");
        assert_eq!(parsed.1, "Apple");
    }
}
