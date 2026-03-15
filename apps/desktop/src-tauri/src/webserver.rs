use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::extract::State as AxumState;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, Json};
use axum::routing::get;
use axum::Router;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::adapters::LiveStatus;
use crate::config::WebServerConfig;

/// Handle for a running web server, allowing graceful shutdown.
pub struct WebServerHandle {
    serve_handle: tokio::task::JoinHandle<()>,
    updater_handle: tokio::task::JoinHandle<()>,
}

impl WebServerHandle {
    pub fn abort(self) {
        self.serve_handle.abort();
        self.updater_handle.abort();
    }
}

/// Shared state for the axum web server
#[derive(Clone)]
struct WebServerState {
    notes_cache: Arc<Mutex<HashMap<i32, String>>>,
    status_broadcast: tokio::sync::broadcast::Sender<LiveStatus>,
    notes_broadcast: tokio::sync::broadcast::Sender<HashMap<i32, String>>,
    last_status: Arc<Mutex<LiveStatus>>,
    ontime_host: String,
    ontime_port: u16,
}

/// Start the web server on the given port. Returns a handle to stop it later.
pub async fn start(
    config: WebServerConfig,
    notes_cache: Arc<Mutex<HashMap<i32, String>>>,
    status_broadcast: tokio::sync::broadcast::Sender<LiveStatus>,
    notes_broadcast: tokio::sync::broadcast::Sender<HashMap<i32, String>>,
) -> Result<WebServerHandle, String> {
    let state = WebServerState {
        notes_cache,
        status_broadcast,
        notes_broadcast,
        last_status: Arc::new(Mutex::new(LiveStatus::default())),
        ontime_host: config.ontime_host,
        ontime_port: config.ontime_port,
    };

    // Spawn a task to keep last_status updated for REST endpoint
    let status_rx = state.status_broadcast.subscribe();
    let last_status = state.last_status.clone();
    let updater_handle = tokio::spawn(async move {
        let mut stream = BroadcastStream::new(status_rx);
        while let Some(Ok(status)) = stream.next().await {
            let mut last = last_status.lock().unwrap();
            *last = status;
        }
    });

    let app = Router::new()
        .route("/", get(page_handler))
        .route("/api/state", get(state_handler))
        .route("/api/events", get(sse_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind web server to {}: {}", addr, e))?;

    log::info!("Web server listening on http://0.0.0.0:{}", config.port);

    let serve_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("Web server error: {}", e);
        }
    });

    Ok(WebServerHandle {
        serve_handle,
        updater_handle,
    })
}

// =============================================================================
// HANDLERS
// =============================================================================

#[derive(serde::Serialize)]
struct StateResponse {
    #[serde(flatten)]
    status: LiveStatus,
    notes: HashMap<i32, String>,
}

async fn state_handler(
    AxumState(state): AxumState<WebServerState>,
) -> Json<StateResponse> {
    let status = state.last_status.lock().unwrap().clone();
    let notes = state.notes_cache.lock().unwrap().clone();

    Json(StateResponse { status, notes })
}

async fn sse_handler(
    AxumState(state): AxumState<WebServerState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    // Send initial state
    let initial_status = state.last_status.lock().unwrap().clone();
    let initial_notes = state.notes_cache.lock().unwrap().clone();

    let status_rx = state.status_broadcast.subscribe();
    let notes_rx = state.notes_broadcast.subscribe();

    let status_stream = BroadcastStream::new(status_rx).filter_map(|result| {
        result.ok().map(|status| {
            Ok(Event::default()
                .event("status")
                .json_data(&status)
                .unwrap())
        })
    });

    let notes_stream = BroadcastStream::new(notes_rx).filter_map(|result| {
        result.ok().map(|notes| {
            Ok(Event::default()
                .event("notes")
                .json_data(&notes)
                .unwrap())
        })
    });

    // Combine: initial events + live streams
    let initial = tokio_stream::iter(vec![
        Ok(Event::default()
            .event("status")
            .json_data(&initial_status)
            .unwrap()),
        Ok(Event::default()
            .event("notes")
            .json_data(&initial_notes)
            .unwrap()),
    ]);

    let combined = initial
        .chain(tokio_stream::StreamExt::merge(status_stream, notes_stream));

    Sse::new(combined).keep_alive(KeepAlive::default())
}

async fn page_handler(
    AxumState(state): AxumState<WebServerState>,
) -> Html<String> {
    let ontime_url = if !state.ontime_host.is_empty() {
        format!(
            "http://{}:{}/timer",
            html_escape(&state.ontime_host),
            state.ontime_port
        )
    } else {
        String::new()
    };

    Html(build_page_html(&ontime_url))
}

/// Basic HTML attribute escaping to prevent injection via user-configured values
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn build_page_html(ontime_url: &str) -> String {
    let ontime_section = if ontime_url.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div id="timer-section">
                <iframe src="{}" frameborder="0" allowfullscreen></iframe>
            </div>"#,
            ontime_url
        )
    };

    let layout_class = if ontime_url.is_empty() {
        "notes-only"
    } else {
        "split"
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Sher Present - Stage View</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}

        body {{
            background: #1a1a1a;
            color: #eee;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            height: 100vh;
            overflow: hidden;
        }}

        .container {{
            display: flex;
            flex-direction: column;
            height: 100vh;
        }}

        .container.split #notes-section {{
            flex: 1;
            min-height: 0;
        }}

        .container.split #timer-section {{
            height: 30%;
            min-height: 150px;
            border-top: 2px solid #333;
        }}

        .container.notes-only #notes-section {{
            flex: 1;
        }}

        #status-bar {{
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 0.75rem 1.5rem;
            background: #222;
            border-bottom: 2px solid #333;
            flex-shrink: 0;
        }}

        #slide-indicator {{
            font-size: 2rem;
            font-weight: 700;
            font-variant-numeric: tabular-nums;
        }}

        #presenting-badge {{
            font-size: 0.875rem;
            padding: 0.25rem 0.75rem;
            border-radius: 999px;
            font-weight: 600;
        }}

        #presenting-badge.live {{
            background: #22c55e;
            color: #000;
        }}

        #presenting-badge.idle {{
            background: #555;
            color: #aaa;
        }}

        #notes-section {{
            overflow-y: auto;
            padding: 2rem;
        }}

        #notes-content {{
            font-size: 2rem;
            line-height: 1.5;
            white-space: pre-wrap;
            word-wrap: break-word;
        }}

        #notes-content.empty {{
            color: #555;
            font-style: italic;
            font-size: 1.5rem;
        }}

        #timer-section {{
            position: relative;
        }}

        #timer-section iframe {{
            width: 100%;
            height: 100%;
            border: none;
        }}
    </style>
</head>
<body>
    <div class="container {layout_class}">
        <div id="status-bar">
            <span id="slide-indicator">--/--</span>
            <span id="presenting-badge" class="idle">IDLE</span>
        </div>

        <div id="notes-section">
            <div id="notes-content" class="empty">Waiting for presentation data...</div>
        </div>

        {ontime_section}
    </div>

    <script>
        const slideIndicator = document.getElementById('slide-indicator');
        const presentingBadge = document.getElementById('presenting-badge');
        const notesContent = document.getElementById('notes-content');

        let currentSlide = 0;
        let totalSlides = 0;
        let isPresenting = false;
        let notes = {{}};

        function updateUI() {{
            slideIndicator.textContent = totalSlides > 0
                ? currentSlide + ' / ' + totalSlides
                : '--/--';

            if (isPresenting) {{
                presentingBadge.textContent = 'LIVE';
                presentingBadge.className = 'live';
            }} else {{
                presentingBadge.textContent = 'IDLE';
                presentingBadge.className = 'idle';
            }}

            const slideNotes = notes[currentSlide];
            if (slideNotes && slideNotes.trim()) {{
                notesContent.textContent = slideNotes;
                notesContent.className = '';
            }} else if (currentSlide > 0) {{
                notesContent.textContent = 'No notes for slide ' + currentSlide;
                notesContent.className = 'empty';
            }} else {{
                notesContent.textContent = 'Waiting for presentation data...';
                notesContent.className = 'empty';
            }}
        }}

        function connect() {{
            const evtSource = new EventSource('/api/events');

            evtSource.addEventListener('status', function(e) {{
                const data = JSON.parse(e.data);
                currentSlide = data.current_slide;
                totalSlides = data.total_slides;
                isPresenting = data.is_presenting;
                updateUI();
            }});

            evtSource.addEventListener('notes', function(e) {{
                notes = JSON.parse(e.data);
                updateUI();
            }});

            evtSource.onerror = function() {{
                evtSource.close();
                setTimeout(connect, 2000);
            }};
        }}

        connect();
    </script>
</body>
</html>"##,
        layout_class = layout_class,
        ontime_section = ontime_section,
    )
}
