use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub fn hash_text(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}
