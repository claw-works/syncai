use anyhow::Result;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::client::push_multi;

/// Watch a local directory and push changes to one or more remote syncai servers.
pub async fn watch(source: &str, targets: &[String], token: &str, debounce_ms: u64) -> Result<()> {
    let root = PathBuf::from(source).canonicalize()
        .unwrap_or_else(|_| PathBuf::from(source));

    info!("👀 Watching {:?} → {:?}", root, targets);
    info!("Debounce: {}ms. Press Ctrl+C to stop.", debounce_ms);

    // Channel to receive raw fs events
    let (tx, mut rx) = mpsc::unbounded_channel::<notify::Result<Event>>();

    let tx_clone = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx_clone.send(res);
    })?;

    watcher.watch(&root, RecursiveMode::Recursive)?;
    if targets.len() == 1 {
        println!("👀 Watching {} → {}", root.display(), targets[0]);
    } else {
        println!("👀 Watching {} → {} targets: {}", root.display(), targets.len(), targets.join(", "));
    }
    println!("   Debounce: {}ms | Ctrl+C to stop", debounce_ms);

    let token = Arc::new(token.to_string());
    let targets = Arc::new(targets.to_vec());
    let source = Arc::new(source.to_string());
    let debounce = Duration::from_millis(debounce_ms);

    let mut last_event: Option<Instant> = None;
    let mut pending = false;

    loop {
        // Try to drain events with a short timeout
        let timeout = if pending {
            // Waiting for debounce to expire
            let elapsed = last_event.map(|t| t.elapsed()).unwrap_or(debounce);
            if elapsed >= debounce {
                Duration::from_millis(0) // fire immediately
            } else {
                debounce - elapsed
            }
        } else {
            Duration::from_secs(60) // idle, just wait
        };

        tokio::select! {
            event = rx.recv() => {
                match event {
                    None => break, // channel closed
                    Some(Ok(ev)) => {
                        if should_sync(&ev) {
                            last_event = Some(Instant::now());
                            pending = true;
                        }
                    }
                    Some(Err(e)) => {
                        warn!("Watch error: {}", e);
                    }
                }
            }
            _ = sleep(timeout), if pending => {
                // Debounce expired — run push_multi
                let elapsed = last_event.map(|t| t.elapsed()).unwrap_or_default();
                if elapsed >= debounce {
                    pending = false;
                    last_event = None;
                    info!("🔄 Change detected, syncing to {} target(s)...", targets.len());
                    let t = token.clone();
                    let tgts = targets.clone();
                    let src = source.clone();
                    tokio::spawn(async move {
                        match push_multi(&src, &tgts, &t, false).await {
                            Ok(()) => {}
                            Err(e) => warn!("Sync failed: {}", e),
                        }
                    });
                }
            }
        }
    }

    Ok(())
}

/// Decide whether a filesystem event should trigger a sync
fn should_sync(event: &Event) -> bool {
    match &event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
            // Skip hidden files and common noise
            event.paths.iter().any(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                !name.starts_with('.') && !name.ends_with('~') && name != "4913"
            })
        }
        _ => false,
    }
}
