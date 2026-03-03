use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::path::PathBuf;
use tracing::info;

use crate::sync::{build_manifest, DiffRequest, DiffResponse};

fn make_client(token: &str) -> Result<Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {}", token).parse()?,
    );
    Ok(Client::builder().default_headers(headers).build()?)
}

fn base_url(target: &str) -> String {
    if target.starts_with("http://") || target.starts_with("https://") {
        target.to_string()
    } else {
        format!("http://{}", target)
    }
}

/// Push a local directory to a remote syncai server
pub async fn push(source: &str, target: &str, token: &str, full: bool) -> Result<()> {
    let root = PathBuf::from(source);
    let base = base_url(target);
    let client = make_client(token)?;

    info!("Building local manifest for {:?}...", root);
    let local_manifest = build_manifest(&root)?;
    info!("Found {} files locally", local_manifest.files.len());

    // Determine which files need to be sent
    let needed: Vec<String> = if full {
        info!("Full sync mode — sending all files");
        local_manifest.files.iter().map(|f| f.path.clone()).collect()
    } else {
        info!("Computing diff with remote...");
        let resp = client
            .post(format!("{}/diff", base))
            .json(&DiffRequest {
                manifest: local_manifest.clone(),
            })
            .send()
            .await?;

        if resp.status() == 401 {
            anyhow::bail!("Authentication failed: check your token");
        }

        let diff: DiffResponse = resp.json().await?;
        info!(
            "Remote needs {} files, {} orphaned",
            diff.needed.len(),
            diff.orphaned.len()
        );

        // Delete orphaned files on remote
        for orphan in &diff.orphaned {
            info!("Removing orphan: {}", orphan);
            client
                .delete(format!("{}/file/{}", base, orphan))
                .send()
                .await?;
        }

        diff.needed
    };

    if needed.is_empty() {
        println!("✅ Already in sync! Nothing to send.");
        return Ok(());
    }

    // Upload needed files with progress bar
    let total_bytes: u64 = {
        let needed_set: std::collections::HashSet<&str> =
            needed.iter().map(|s| s.as_str()).collect();
        local_manifest
            .files
            .iter()
            .filter(|f| needed_set.contains(f.path.as_str()))
            .map(|f| f.size)
            .sum()
    };

    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
        )?
        .progress_chars("=>-"),
    );

    println!("📤 Syncing {} files ({:.1} MB)...", needed.len(), total_bytes as f64 / 1_048_576.0);

    for rel_path in &needed {
        let abs_path = root.join(rel_path);
        let contents = tokio::fs::read(&abs_path).await?;
        let size = contents.len() as u64;

        client
            .post(format!("{}/file/{}", base, rel_path))
            .body(contents)
            .send()
            .await?
            .error_for_status()?;

        pb.inc(size);
    }

    pb.finish_with_message("done");
    println!("✅ Sync complete! {} files sent.", needed.len());

    Ok(())
}

/// Pull a directory from a remote syncai server
pub async fn pull(source: &str, target: &str, token: &str) -> Result<()> {
    let root = PathBuf::from(target);
    let base = base_url(source);
    let client = make_client(token)?;

    info!("Fetching remote manifest from {}...", base);
    let resp = client
        .get(format!("{}/manifest", base))
        .send()
        .await?;

    if resp.status() == 401 {
        anyhow::bail!("Authentication failed: check your token");
    }

    let remote_manifest = resp.json::<crate::sync::Manifest>().await?;
    info!("Remote has {} files", remote_manifest.files.len());

    // Build local manifest
    std::fs::create_dir_all(&root)?;
    let local_manifest = build_manifest(&root)?;

    let diff = crate::sync::compute_diff(&remote_manifest, &local_manifest);
    info!("Need to pull {} files", diff.needed.len());

    if diff.needed.is_empty() {
        println!("✅ Already in sync! Nothing to pull.");
        return Ok(());
    }

    let total_bytes = crate::sync::total_size(&remote_manifest, &diff.needed);
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
        )?
        .progress_chars("=>-"),
    );

    println!("📥 Pulling {} files ({:.1} MB)...", diff.needed.len(), total_bytes as f64 / 1_048_576.0);

    for rel_path in &diff.needed {
        let dest = root.join(rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = client
            .get(format!("{}/file/{}", base, rel_path))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let size = contents.len() as u64;
        tokio::fs::write(&dest, &contents).await?;
        pb.inc(size);
    }

    pb.finish_with_message("done");
    println!("✅ Pull complete! {} files received.", diff.needed.len());

    Ok(())
}
