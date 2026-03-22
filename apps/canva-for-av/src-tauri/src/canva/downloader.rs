use crate::models::{Presentation, SlideElement};
use reqwest::Client;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Base directory for all presentation data.
fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("canva-for-av")
        .join("presentations")
}

/// Get the directory for a specific presentation's assets.
pub fn presentation_dir(id: &str) -> PathBuf {
    data_dir().join(id)
}

/// Download all assets for a presentation and update local paths.
pub async fn download_assets(
    client: &Client,
    presentation: &mut Presentation,
    raw_bootstrap: &serde_json::Value,
) -> Result<(), String> {
    let base = presentation_dir(&presentation.id);
    let media_dir = base.join("media");
    let fonts_dir = base.join("fonts");

    fs::create_dir_all(&media_dir)
        .await
        .map_err(|e| format!("Failed to create media dir: {}", e))?;
    fs::create_dir_all(&fonts_dir)
        .await
        .map_err(|e| format!("Failed to create fonts dir: {}", e))?;

    // Save raw bootstrap for re-parsing later
    let bootstrap_path = base.join("bootstrap.json");
    let bootstrap_str = serde_json::to_string_pretty(raw_bootstrap)
        .map_err(|e| format!("Failed to serialize bootstrap: {}", e))?;
    fs::write(&bootstrap_path, bootstrap_str)
        .await
        .map_err(|e| format!("Failed to write bootstrap: {}", e))?;

    // Collect all URLs to download
    let mut downloads: Vec<DownloadTask> = Vec::new();

    // Slide thumbnails
    for slide in &presentation.slides {
        if let Some(ref url) = slide.thumbnail_url {
            if !url.is_empty() {
                let filename = format!("thumb_{}.jpg", slide.index);
                downloads.push(DownloadTask {
                    url: url.clone(),
                    dest: media_dir.join(&filename),
                    kind: AssetKind::Thumbnail(slide.index),
                });
            }
        }
    }

    // Image elements
    for slide in &presentation.slides {
        for (ei, elem) in slide.elements.iter().enumerate() {
            if let SlideElement::Image { asset_url, .. } = elem {
                if !asset_url.is_empty() {
                    let ext = url_extension(asset_url).unwrap_or("png");
                    let filename = format!("slide{}_{}.{}", slide.index, ei, ext);
                    downloads.push(DownloadTask {
                        url: asset_url.clone(),
                        dest: media_dir.join(&filename),
                        kind: AssetKind::Image(slide.index, ei),
                    });
                }
            }
        }
    }

    // Fonts
    for (fi, font) in presentation.fonts.iter().enumerate() {
        if let Some(ref url) = font.woff2_url {
            if !url.is_empty() {
                let ext = if url.contains(".otf") { "otf" } else { "woff2" };
                let filename = format!("font_{}_{}.{}", fi, sanitize_filename(&font.family), ext);
                downloads.push(DownloadTask {
                    url: url.clone(),
                    dest: fonts_dir.join(&filename),
                    kind: AssetKind::Font(fi),
                });
            }
        }
    }

    // Download in batches of 8
    let batch_size = 8;
    for chunk in downloads.chunks(batch_size) {
        let futures: Vec<_> = chunk
            .iter()
            .map(|task| download_file(client, &task.url, &task.dest))
            .collect();

        let results = futures::future::join_all(futures).await;

        // Update local paths for successful downloads
        for (task, result) in chunk.iter().zip(results.iter()) {
            if let Ok(path) = result {
                match &task.kind {
                    AssetKind::Thumbnail(slide_idx) => {
                        if let Some(slide) = presentation.slides.get_mut(*slide_idx) {
                            slide.thumbnail_local = Some(path.to_string_lossy().to_string());
                        }
                    }
                    AssetKind::Image(slide_idx, elem_idx) => {
                        if let Some(slide) = presentation.slides.get_mut(*slide_idx) {
                            if let Some(SlideElement::Image { local_path, .. }) =
                                slide.elements.get_mut(*elem_idx)
                            {
                                *local_path = Some(path.to_string_lossy().to_string());
                            }
                        }
                    }
                    AssetKind::Font(font_idx) => {
                        if let Some(font) = presentation.fonts.get_mut(*font_idx) {
                            font.local_path = Some(path.to_string_lossy().to_string());
                        }
                    }
                }
            } else if let Err(e) = result {
                log::warn!("Failed to download {}: {}", task.url, e);
            }
        }
    }

    // Save metadata
    let metadata_path = base.join("metadata.json");
    let metadata_str = serde_json::to_string_pretty(presentation)
        .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
    fs::write(&metadata_path, metadata_str)
        .await
        .map_err(|e| format!("Failed to write metadata: {}", e))?;

    Ok(())
}

async fn download_file(client: &Client, url: &str, dest: &Path) -> Result<PathBuf, String> {
    // Skip if already downloaded
    if dest.exists() {
        return Ok(dest.to_path_buf());
    }

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {} for {}", response.status(), url));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read bytes: {}", e))?;

    fs::write(dest, &bytes)
        .await
        .map_err(|e| format!("Failed to write file: {}", e))?;

    log::info!("Downloaded {} ({} bytes)", dest.display(), bytes.len());
    Ok(dest.to_path_buf())
}

struct DownloadTask {
    url: String,
    dest: PathBuf,
    kind: AssetKind,
}

enum AssetKind {
    Thumbnail(usize),
    Image(usize, usize),
    Font(usize),
}

fn url_extension(url: &str) -> Option<&str> {
    let path = url.split('?').next()?;
    let ext = path.rsplit('.').next()?;
    if ext.len() <= 5 {
        Some(ext)
    } else {
        None
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Load a presentation from disk by ID.
pub async fn load_presentation(id: &str) -> Result<Presentation, String> {
    let path = presentation_dir(id).join("metadata.json");
    let content = fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read presentation {}: {}", id, e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse presentation metadata: {}", e))
}

/// Delete a presentation's assets from disk.
pub async fn delete_presentation_files(id: &str) -> Result<(), String> {
    let dir = presentation_dir(id);
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .await
            .map_err(|e| format!("Failed to delete presentation: {}", e))?;
    }
    Ok(())
}
