use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::State as AxumState;
use axum::extract::WebSocketUpgrade;
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
    font_size: u16,
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
        font_size: config.font_size,
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
.route("/api/ws", get(ws_handler))
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

// =============================================================================
// WEBSOCKET HANDLER
// =============================================================================

async fn ws_handler(
    ws: WebSocketUpgrade,
    AxumState(state): AxumState<WebServerState>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: WebServerState) {
    log::info!("WebSocket client connected");

    // Send initial state immediately
    let initial_status = state.last_status.lock().unwrap().clone();
    let initial_notes = state.notes_cache.lock().unwrap().clone();

    let status_json = serde_json::json!({
        "type": "status",
        "payload": initial_status,
    });
    let notes_json = serde_json::json!({
        "type": "notes",
        "payload": initial_notes,
    });

    if socket.send(Message::Text(status_json.to_string().into())).await.is_err() {
        return;
    }
    if socket.send(Message::Text(notes_json.to_string().into())).await.is_err() {
        return;
    }

    // Subscribe to broadcast channels
    let mut status_rx = state.status_broadcast.subscribe();
    let mut notes_rx = state.notes_broadcast.subscribe();

    loop {
        tokio::select! {
            result = status_rx.recv() => {
                match result {
                    Ok(status) => {
                        let msg = serde_json::json!({
                            "type": "status",
                            "payload": status,
                        });
                        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            result = notes_rx.recv() => {
                match result {
                    Ok(notes) => {
                        let msg = serde_json::json!({
                            "type": "notes",
                            "payload": notes,
                        });
                        if socket.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // ignore client messages
                }
            }
        }
    }

    log::info!("WebSocket client disconnected");
}

async fn page_handler(
    AxumState(state): AxumState<WebServerState>,
) -> Html<String> {
    let ontime_host = html_escape(&state.ontime_host);
    let ontime_port = state.ontime_port;
    let font_size = state.font_size;
    Html(build_page_html(&ontime_host, ontime_port, font_size))
}

/// Basic HTML attribute escaping to prevent injection via user-configured values
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn build_page_html(ontime_host: &str, ontime_port: u16, font_size: u16) -> String {
    let has_ontime = !ontime_host.is_empty();

    let ontime_section = if has_ontime {
        format!(
            r##"<div id="timer-section">
                <div id="timer-display">
                    <div id="timer-event-title"></div>
                    <div id="timer-value">--:--:--</div>
                    <div id="timer-phase-badge">STOPPED</div>
                </div>
                <div id="timer-status">Connecting to Ontime...</div>
            </div>

            <script>
            (function() {{
                const timerValue = document.getElementById('timer-value');
                const timerPhase = document.getElementById('timer-phase-badge');
                const timerTitle = document.getElementById('timer-event-title');
                const timerStatus = document.getElementById('timer-status');

                const ONTIME_HOST = '{ontime_host}';
                const ONTIME_PORT = {ontime_port};

                let lastPhase = null;

                function formatTime(ms) {{
                    if (ms == null) return '--:--:--';
                    const negative = ms < 0;
                    const abs = Math.abs(ms);
                    const totalSeconds = Math.floor(abs / 1000);
                    const hours = Math.floor(totalSeconds / 3600);
                    const minutes = Math.floor((totalSeconds % 3600) / 60);
                    const seconds = totalSeconds % 60;
                    const pad = (n) => String(n).padStart(2, '0');
                    const prefix = negative ? '-' : '';
                    if (hours > 0) {{
                        return prefix + hours + ':' + pad(minutes) + ':' + pad(seconds);
                    }}
                    return prefix + pad(minutes) + ':' + pad(seconds);
                }}

                function updateTimerDisplay(current, playback, duration) {{
                    console.log('[Ontime] updateTimerDisplay: current=' + current + ' playback=' + playback + ' duration=' + duration);

                    timerValue.textContent = formatTime(current);

                    // Phase badge
                    let badgeText = playback ? playback.toUpperCase() : 'STOPPED';
                    if (playback === 'play') badgeText = 'RUNNING';
                    if (playback === 'armed') badgeText = 'ARMED';
                    timerPhase.textContent = badgeText;
                    timerPhase.className = 'phase-' + (playback || 'stop');

                    // Timer value color
                    const isOvertime = current != null && current < 0;
                    const isWarning = current != null && current > 0 && current < 60000;

                    timerValue.classList.remove('overtime', 'warning', 'paused', 'stopped');
                    if (isOvertime) {{
                        timerValue.classList.add('overtime');
                    }} else if (isWarning && playback === 'play') {{
                        timerValue.classList.add('warning');
                    }} else if (playback === 'pause') {{
                        timerValue.classList.add('paused');
                    }} else if (playback === 'stop' || playback === 'armed') {{
                        timerValue.classList.add('stopped');
                    }}

                    if (playback !== lastPhase) {{
                        console.log('[Ontime] Phase transition: ' + lastPhase + ' -> ' + playback);
                        lastPhase = playback;
                    }}
                }}

                function handleMessage(data) {{
                    const tag = data.tag;
                    console.log('[Ontime] WS message: tag=' + tag, JSON.stringify(data));

                    if (tag === 'runtime-data') {{
                        const p = data.payload;
                        if (p.timer) {{
                            console.log('[Ontime] Timer update: current=' + p.timer.current + ' playback=' + p.timer.playback + ' phase=' + p.timer.phase + ' duration=' + p.timer.duration);
                            updateTimerDisplay(p.timer.current, p.timer.playback, p.timer.duration);
                        }}
                        if (p.eventNow) {{
                            const title = p.eventNow.title || '';
                            console.log('[Ontime] Event now: title=' + title);
                            timerTitle.textContent = title;
                        }}
                    }}
                }}

                function connectOntime() {{
                    const url = 'ws://' + ONTIME_HOST + ':' + ONTIME_PORT + '/ws';
                    console.log('[Ontime] Connecting to ' + url);
                    timerStatus.textContent = 'Connecting to Ontime...';
                    timerStatus.style.display = 'block';

                    const ws = new WebSocket(url);

                    ws.onopen = function() {{
                        console.log('[Ontime] WebSocket connected');
                        timerStatus.style.display = 'none';
                    }};

                    ws.onmessage = function(event) {{
                        try {{
                            const data = JSON.parse(event.data);
                            handleMessage(data);
                        }} catch (e) {{
                            console.error('[Ontime] Failed to parse message:', event.data, e);
                        }}
                    }};

                    ws.onclose = function(event) {{
                        console.log('[Ontime] WebSocket closed: code=' + event.code + ' reason=' + event.reason);
                        timerStatus.textContent = 'Disconnected. Reconnecting...';
                        timerStatus.style.display = 'block';
                        setTimeout(connectOntime, 2000);
                    }};

                    ws.onerror = function(err) {{
                        console.error('[Ontime] WebSocket error:', err);
                    }};
                }}

                connectOntime();
            }})();
            </script>"##,
            ontime_host = ontime_host,
            ontime_port = ontime_port,
        )
    } else {
        String::new()
    };

    let layout_class = if has_ontime { "split" } else { "notes-only" };

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Sher Present - Stage View</title>
    <style>
        :root {{
            --notes-font-size: {font_size}px;
        }}

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

        /* Teleprompter notes section */
        #notes-section {{
            overflow-y: auto;
            scroll-behavior: smooth;
        }}

        #notes-scroll {{
            padding: 40vh 2rem;
        }}

        #notes-scroll.empty-state {{
            display: flex;
            align-items: center;
            justify-content: center;
            min-height: 100%;
            padding: 2rem;
        }}

        #notes-scroll .empty-message {{
            color: #555;
            font-style: italic;
            font-size: 1.5rem;
        }}

        .slide-notes {{
            padding: 1rem 1.5rem;
            margin-bottom: 0.5rem;
            border-left: 3px solid transparent;
            border-radius: 4px;
            transition: border-color 0.3s, background 0.3s;
        }}

        .slide-notes.active {{
            border-left-color: #22c55e;
            background: rgba(34, 197, 94, 0.08);
        }}

        .slide-marker {{
            font-size: 0.75em;
            color: #666;
            text-transform: uppercase;
            letter-spacing: 0.05em;
            padding-bottom: 0.5rem;
            border-bottom: 1px solid #333;
            margin-bottom: 0.75rem;
            font-weight: 600;
        }}

        .slide-notes.active .slide-marker {{
            color: #22c55e;
            border-bottom-color: rgba(34, 197, 94, 0.3);
        }}

        .slide-text {{
            font-size: var(--notes-font-size);
            line-height: 1.5;
            white-space: pre-wrap;
            word-wrap: break-word;
        }}

        .slide-text.no-notes {{
            color: #555;
            font-style: italic;
            font-size: calc(var(--notes-font-size) * 0.75);
        }}

        /* Timer section styles */
        #timer-section {{
            position: relative;
            display: flex;
            align-items: center;
            justify-content: center;
        }}

        #timer-display {{
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            gap: 0.5rem;
            padding: 1rem;
        }}

        #timer-event-title {{
            font-size: 1.25rem;
            color: #aaa;
            max-width: 80vw;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            text-align: center;
            min-height: 1.5em;
        }}

        #timer-value {{
            font-size: 6rem;
            font-weight: 700;
            font-family: 'SF Mono', 'Cascadia Code', 'Consolas', 'Monaco', monospace;
            font-variant-numeric: tabular-nums;
            color: #22c55e;
            line-height: 1;
        }}

        #timer-value.overtime {{
            color: #ef4444;
        }}

        #timer-value.warning {{
            color: #eab308;
        }}

        #timer-value.paused {{
            color: #aaa;
        }}

        #timer-value.stopped {{
            color: #666;
        }}

        #timer-phase-badge {{
            font-size: 0.75rem;
            padding: 0.2rem 0.6rem;
            border-radius: 999px;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }}

        #timer-phase-badge.phase-play {{
            background: #22c55e;
            color: #000;
        }}

        #timer-phase-badge.phase-pause {{
            background: #eab308;
            color: #000;
        }}

        #timer-phase-badge.phase-roll {{
            background: #3b82f6;
            color: #fff;
        }}

        #timer-phase-badge.phase-stop {{
            background: #555;
            color: #aaa;
        }}

        #timer-phase-badge.phase-armed {{
            background: #f97316;
            color: #000;
        }}

        #timer-status {{
            position: absolute;
            bottom: 0.5rem;
            right: 0.75rem;
            font-size: 0.75rem;
            color: #666;
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
            <div id="notes-scroll" class="empty-state">
                <span class="empty-message">Waiting for presentation data...</span>
            </div>
        </div>

        {ontime_section}
    </div>

    <script>
        const slideIndicator = document.getElementById('slide-indicator');
        const presentingBadge = document.getElementById('presenting-badge');
        const notesScroll = document.getElementById('notes-scroll');

        let currentSlide = 0;
        let totalSlides = 0;
        let isPresenting = false;
        let notes = {{}};

        function escapeHtml(text) {{
            const div = document.createElement('div');
            div.textContent = text;
            return div.innerHTML;
        }}

        function buildNotesBlocks() {{
            // Get sorted slide numbers from notes
            const slideNums = Object.keys(notes).map(Number).sort(function(a, b) {{ return a - b; }});

            if (slideNums.length === 0) {{
                notesScroll.className = 'empty-state';
                notesScroll.innerHTML = '<span class="empty-message">Waiting for presentation data...</span>';
                return;
            }}

            notesScroll.className = '';
            let html = '';

            for (let i = 0; i < slideNums.length; i++) {{
                const num = slideNums[i];
                const text = notes[num];
                const isActive = num === currentSlide;
                const activeClass = isActive ? ' active' : '';
                const hasText = text && text.trim();

                html += '<div class="slide-notes' + activeClass + '" data-slide="' + num + '">';
                html += '<div class="slide-marker">Slide ' + num + '</div>';
                if (hasText) {{
                    html += '<div class="slide-text">' + escapeHtml(text) + '</div>';
                }} else {{
                    html += '<div class="slide-text no-notes">No notes</div>';
                }}
                html += '</div>';
            }}

            notesScroll.innerHTML = html;
            scrollToActive();
        }}

        function updateActiveSlide() {{
            const blocks = notesScroll.querySelectorAll('.slide-notes');
            for (let i = 0; i < blocks.length; i++) {{
                const slideNum = parseInt(blocks[i].getAttribute('data-slide'));
                if (slideNum === currentSlide) {{
                    blocks[i].classList.add('active');
                }} else {{
                    blocks[i].classList.remove('active');
                }}
            }}
            scrollToActive();
        }}

        function scrollToActive() {{
            const active = notesScroll.querySelector('.slide-notes.active');
            if (active) {{
                active.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
            }}
        }}

        function updateStatusBar() {{
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
        }}

        function connectPresentation() {{
            const wsUrl = 'ws://' + window.location.host + '/api/ws';
            console.log('[Stage] Connecting to ' + wsUrl);

            const ws = new WebSocket(wsUrl);

            ws.onopen = function() {{
                console.log('[Stage] WebSocket connected');
            }};

            ws.onmessage = function(event) {{
                try {{
                    const data = JSON.parse(event.data);
                    console.log('[Stage] WS message: type=' + data.type, data);

                    if (data.type === 'status') {{
                        const s = data.payload;
                        const slideChanged = currentSlide !== s.current_slide;
                        currentSlide = s.current_slide;
                        totalSlides = s.total_slides;
                        isPresenting = s.is_presenting;
                        updateStatusBar();
                        if (slideChanged) {{
                            updateActiveSlide();
                        }}
                    }} else if (data.type === 'notes') {{
                        notes = data.payload;
                        buildNotesBlocks();
                    }}
                }} catch (e) {{
                    console.error('[Stage] Failed to parse message:', event.data, e);
                }}
            }};

            ws.onclose = function(event) {{
                console.log('[Stage] WebSocket closed: code=' + event.code);
                setTimeout(connectPresentation, 2000);
            }};

            ws.onerror = function(err) {{
                console.error('[Stage] WebSocket error:', err);
            }};
        }}

        connectPresentation();
    </script>
</body>
</html>"##,
        font_size = font_size,
        layout_class = layout_class,
        ontime_section = ontime_section,
    )
}
