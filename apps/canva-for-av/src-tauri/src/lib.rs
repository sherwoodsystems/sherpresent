mod canva;
mod models;
mod storage;

use models::{ImportProgress, ImportStage, Presentation, RecentEntry};
use std::sync::Mutex;
use tauri::Emitter;

struct AppState {
    http_client: reqwest::Client,
    import_progress: Option<ImportProgress>,
}

struct ManagedState(Mutex<AppState>);

fn emit_progress(app: &tauri::AppHandle, stage: ImportStage, detail: &str) {
    let progress = ImportProgress {
        stage,
        detail: detail.to_string(),
    };
    let _ = app.emit("import-progress", &progress);
}

#[tauri::command]
async fn import_presentation(
    app: tauri::AppHandle,
    state: tauri::State<'_, ManagedState>,
    url: String,
) -> Result<Presentation, String> {
    // Validate URL
    if !url.contains("canva.com") {
        return Err("URL must be a canva.com link".to_string());
    }

    let client = {
        let s = state.0.lock().unwrap();
        s.http_client.clone()
    };

    // Stage 1: Fetch
    emit_progress(&app, ImportStage::Fetching, "Downloading page...");
    let (html, bootstrap) = canva::fetch::fetch_canva_page(&client, &url).await?;

    // Stage 2: Parse
    emit_progress(&app, ImportStage::Parsing, "Parsing presentation data...");
    let mut presentation = canva::parser::parse_presentation(&bootstrap, &html, &url)?;

    log::info!(
        "Parsed presentation '{}': {} slides, {} fonts",
        presentation.title,
        presentation.slides.len(),
        presentation.fonts.len()
    );

    // Debug: log per-slide element counts
    for (i, slide) in presentation.slides.iter().enumerate() {
        log::info!(
            "[IMPORT] Slide {}: {} elements, thumbnail: {}",
            i,
            slide.elements.len(),
            slide.thumbnail_url.as_ref().map(|u| &u[..u.len().min(60)]).unwrap_or("none")
        );
        for (j, elem) in slide.elements.iter().enumerate() {
            match elem {
                models::SlideElement::Text { content, x, y, width, height, .. } => {
                    log::info!("[IMPORT]   elem[{}] Text: {:?} at ({},{}) {}x{}", j, &content[..content.len().min(50)], x, y, width, height);
                }
                models::SlideElement::Image { asset_url, x, y, width, height, .. } => {
                    log::info!("[IMPORT]   elem[{}] Image: {} at ({},{}) {}x{}", j, &asset_url[..asset_url.len().min(60)], x, y, width, height);
                }
                models::SlideElement::Shape { x, y, width, height, fill_color, .. } => {
                    log::info!("[IMPORT]   elem[{}] Shape: fill={:?} at ({},{}) {}x{}", j, fill_color, x, y, width, height);
                }
            }
        }
    }

    // Stage 3: Download assets
    emit_progress(
        &app,
        ImportStage::Downloading,
        &format!("Downloading assets for {} slides...", presentation.slides.len()),
    );
    canva::downloader::download_assets(&client, &mut presentation, &bootstrap).await?;

    // Stage 4: Save to recent
    let thumbnail_path = presentation
        .slides
        .first()
        .and_then(|s| s.thumbnail_local.clone());

    storage::add_recent(RecentEntry {
        id: presentation.id.clone(),
        title: presentation.title.clone(),
        url: presentation.url.clone(),
        imported_at: presentation.imported_at.clone(),
        slide_count: presentation.slides.len(),
        thumbnail_path,
    })?;

    emit_progress(&app, ImportStage::Complete, "Import complete!");

    // Update state
    {
        let mut s = state.0.lock().unwrap();
        s.import_progress = Some(ImportProgress {
            stage: ImportStage::Complete,
            detail: "Import complete!".to_string(),
        });
    }

    Ok(presentation)
}

#[tauri::command]
fn list_presentations() -> Vec<RecentEntry> {
    storage::load_recent()
}

#[tauri::command]
async fn get_presentation(id: String) -> Result<Presentation, String> {
    canva::downloader::load_presentation(&id).await
}

#[tauri::command]
async fn delete_presentation(id: String) -> Result<(), String> {
    canva::downloader::delete_presentation_files(&id).await?;
    storage::remove_recent(&id)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
        .build()
        .expect("Failed to create HTTP client");

    let initial_state = ManagedState(Mutex::new(AppState {
        http_client: client,
        import_progress: None,
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(initial_state)
        .invoke_handler(tauri::generate_handler![
            import_presentation,
            list_presentations,
            get_presentation,
            delete_presentation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
