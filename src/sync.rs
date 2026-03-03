use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;
use anyhow::Result;

/// File metadata with hash for incremental sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative path from the sync root
    pub path: String,
    /// SHA256 hash of file contents
    pub hash: String,
    /// File size in bytes
    pub size: u64,
}

/// Manifest of all files in a directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub files: Vec<FileEntry>,
}

/// Diff request: client sends its manifest, server returns which files it needs
#[derive(Debug, Serialize, Deserialize)]
pub struct DiffRequest {
    pub manifest: Manifest,
}

/// Diff response: list of paths the server is missing or has stale versions of
#[derive(Debug, Serialize, Deserialize)]
pub struct DiffResponse {
    /// Paths the server needs (missing or hash mismatch)
    pub needed: Vec<String>,
    /// Paths on server not in client manifest (orphaned)
    pub orphaned: Vec<String>,
}

/// Build a manifest for a local directory
pub fn build_manifest(root: &Path) -> Result<Manifest> {
    let mut files = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let abs_path = entry.path();
        let rel_path = abs_path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/"); // normalize Windows paths

        // Skip common non-essential dirs
        if rel_path.starts_with(".git/")
            || rel_path.starts_with("target/")
            || rel_path.starts_with("node_modules/")
        {
            continue;
        }

        let contents = std::fs::read(abs_path)?;
        let hash = hex::encode(Sha256::digest(&contents));
        let size = contents.len() as u64;

        files.push(FileEntry {
            path: rel_path,
            hash,
            size,
        });
    }

    Ok(Manifest { files })
}

/// Compute diff: which files does the target need from the source manifest?
pub fn compute_diff(source: &Manifest, target: &Manifest) -> DiffResponse {
    let target_map: HashMap<&str, &str> = target
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.hash.as_str()))
        .collect();

    let source_paths: std::collections::HashSet<&str> =
        source.files.iter().map(|f| f.path.as_str()).collect();

    let needed: Vec<String> = source
        .files
        .iter()
        .filter(|f| {
            match target_map.get(f.path.as_str()) {
                None => true,              // target doesn't have it
                Some(h) => *h != f.hash,  // target has stale version
            }
        })
        .map(|f| f.path.clone())
        .collect();

    let orphaned: Vec<String> = target
        .files
        .iter()
        .filter(|f| !source_paths.contains(f.path.as_str()))
        .map(|f| f.path.clone())
        .collect();

    DiffResponse { needed, orphaned }
}

/// Compute total size of files in a list
pub fn total_size(manifest: &Manifest, paths: &[String]) -> u64 {
    let path_set: std::collections::HashSet<&str> =
        paths.iter().map(|p| p.as_str()).collect();
    manifest
        .files
        .iter()
        .filter(|f| path_set.contains(f.path.as_str()))
        .map(|f| f.size)
        .sum()
}
