//! Canva adapter using a webview window to control Canva presentations
//!
//! Opens the Canva remote control URL in a webview with an intercept script
//! that monkey-patches WebSocket/fetch/XHR to track presentation state and
//! provides a `window.__canvaNavigate()` function for slide navigation.

use super::{ConnectionStatus, LiveStatus, PresentationAdapter, PresentationState, SlideInfo};
use std::sync::{Arc, Mutex};
use tauri::{webview::WebviewWindowBuilder, Emitter, Manager, Url};

const INTERCEPT_SCRIPT: &str = r#"
(function() {
  function tauriLog(category, data) {
    const msg = typeof data === 'string' ? data : JSON.stringify(data);
    const ipc = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
      || (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
    if (ipc) {
      ipc('log_from_webview', { category: category, message: msg }).catch(function(e) {});
    }
  }

  // --- Monkey-patch WebSocket ---
  const OrigWebSocket = window.WebSocket;
  window.WebSocket = function(url, protocols) {
    tauriLog('WS_OPEN', { url: url, protocols: protocols });
    const ws = protocols ? new OrigWebSocket(url, protocols) : new OrigWebSocket(url);

    ws.addEventListener('message', function(event) {
      var data = event.data;
      if (typeof data === 'string') {
        tauriLog('WS_RECV', { url: url, data: data.substring(0, 4000) });
      } else if (data instanceof Blob) {
        data.arrayBuffer().then(function(buf) {
          var bytes = new Uint8Array(buf);
          var hex = Array.from(bytes).map(function(b) { return b.toString(16).padStart(2, '0'); }).join(' ');
          tauriLog('WS_RECV_BIN', { url: url, size: bytes.length, hex: hex });
          tryExtractCanvaState(bytes);
        });
      } else if (data instanceof ArrayBuffer) {
        var bytes = new Uint8Array(data);
        var hex = Array.from(bytes).map(function(b) { return b.toString(16).padStart(2, '0'); }).join(' ');
        tauriLog('WS_RECV_BIN', { url: url, size: bytes.length, hex: hex });
        tryExtractCanvaState(bytes);
      }
    });

    ws.addEventListener('open', function() {
      tauriLog('WS_CONNECTED', { url: url });
    });

    ws.addEventListener('close', function(event) {
      tauriLog('WS_CLOSE', { url: url, code: event.code, reason: event.reason });
    });

    ws.addEventListener('error', function() {
      tauriLog('WS_ERROR', { url: url });
    });

    const origSend = ws.send.bind(ws);
    ws.send = function(data) {
      if (typeof data === 'string') {
        tauriLog('WS_SEND', { url: url, data: data.substring(0, 4000) });
      } else if (data instanceof ArrayBuffer) {
        var bytes = new Uint8Array(data);
        var hex = Array.from(bytes).map(function(b) { return b.toString(16).padStart(2, '0'); }).join(' ');
        tauriLog('WS_SEND_BIN', { url: url, size: bytes.length, hex: hex });
      } else if (data instanceof Uint8Array) {
        var hex2 = Array.from(data).map(function(b) { return b.toString(16).padStart(2, '0'); }).join(' ');
        tauriLog('WS_SEND_BIN', { url: url, size: data.length, hex: hex2 });
      } else {
        tauriLog('WS_SEND', { url: url, type: typeof data, size: data.size || data.byteLength || '?' });
      }
      return origSend(data);
    };

    return ws;
  };
  window.WebSocket.prototype = OrigWebSocket.prototype;
  window.WebSocket.CONNECTING = OrigWebSocket.CONNECTING;
  window.WebSocket.OPEN = OrigWebSocket.OPEN;
  window.WebSocket.CLOSING = OrigWebSocket.CLOSING;
  window.WebSocket.CLOSED = OrigWebSocket.CLOSED;

  // --- Monkey-patch fetch ---
  const origFetch = window.fetch;
  window.fetch = function() {
    const args = arguments;
    const url = (args[0] && args[0].url) ? args[0].url : String(args[0]);
    if (url.startsWith('ipc://') || url.startsWith('tauri://')) {
      return origFetch.apply(this, args);
    }
    const method = (args[1] && args[1].method) ? args[1].method : 'GET';
    tauriLog('FETCH_REQ', { method: method, url: url });

    return origFetch.apply(this, args).then(function(response) {
      tauriLog('FETCH_RES', { method: method, url: url, status: response.status });
      return response;
    }).catch(function(err) {
      tauriLog('FETCH_ERR', { method: method, url: url, error: String(err) });
      throw err;
    });
  };

  // --- Monkey-patch XMLHttpRequest ---
  const origXHROpen = XMLHttpRequest.prototype.open;
  const origXHRSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url) {
    this._logMethod = method;
    this._logUrl = String(url);
    this._isIpc = this._logUrl.startsWith('ipc://') || this._logUrl.startsWith('tauri://');
    if (!this._isIpc) {
      tauriLog('XHR_OPEN', { method: method, url: this._logUrl });
    }
    return origXHROpen.apply(this, arguments);
  };
  XMLHttpRequest.prototype.send = function(body) {
    if (!this._isIpc) {
      tauriLog('XHR_SEND', { method: this._logMethod, url: this._logUrl, body: body ? String(body).substring(0, 2000) : null });
      this.addEventListener('load', function() {
        var respBody = '';
        try { respBody = this.responseText.substring(0, 4000); } catch(e) {}
        tauriLog('XHR_DONE', { method: this._logMethod, url: this._logUrl, status: this.status, response: respBody });
      });
    }
    return origXHRSend.apply(this, arguments);
  };

  // --- Forward console.* ---
  ['log', 'warn', 'error', 'info', 'debug'].forEach(function(level) {
    var orig = console[level];
    console[level] = function() {
      var args = Array.prototype.slice.call(arguments);
      var msg = args.map(function(a) {
        if (typeof a === 'string') return a;
        try { return JSON.stringify(a); } catch(e) { return String(a); }
      }).join(' ');
      tauriLog('CONSOLE_' + level.toUpperCase(), msg.substring(0, 4000));
      return orig.apply(console, arguments);
    };
  });

  // --- Debounce: ignore spurious Type A messages after snapshot ---
  var lastSnapshotTime = 0;

  // --- Extract slide state from WS binary frames ---
  function tryExtractCanvaState(bytes) {
    var jsonStart = -1;
    for (var i = 0; i < bytes.length; i++) {
      if (bytes[i] === 0x7b) { jsonStart = i; break; }
    }
    if (jsonStart < 0) return;

    var jsonStr;
    try { jsonStr = new TextDecoder().decode(bytes.slice(jsonStart)); } catch(e) { return; }
    try {
      var obj = JSON.parse(jsonStr);

      // State snapshot: {"A?":"C","Bk":{"B":{"Bk": currentPage, "Bl": totalPages}}}
      if (obj['A?'] === 'C' && obj.Bk && obj.Bk.B) {
        var currentPage = obj.Bk.B.Bk;
        var totalPages = obj.Bk.B.Bl;
        tauriLog('STATE_UPDATE', { type: 'snapshot', currentPage: currentPage, totalPages: totalPages });
        var ipc = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
          || (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
        if (ipc) {
          ipc('update_canva_state', { currentPage: currentPage, totalPages: totalPages, notes: null }).catch(function(e) {});
        }
        lastSnapshotTime = Date.now();
      }

      // Slide update: {"A?":"A","Bk": pageNumber}
      // Guard: ignore spurious Type A messages that arrive shortly after a snapshot
      // (Canva sends a handshake ack with Bk:1 that is NOT a page number)
      if (obj['A?'] === 'A' && typeof obj.Bk === 'number') {
        if (Date.now() - lastSnapshotTime < 3000) {
          tauriLog('STATE_UPDATE', { type: 'slide_change_ignored_post_snapshot', value: obj.Bk });
          return;
        }
        tauriLog('STATE_UPDATE', { type: 'slide_change', currentPage: obj.Bk });
        var ipc2 = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
          || (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
        if (ipc2) {
          ipc2('update_canva_state', { currentPage: obj.Bk, totalPages: -1, notes: null }).catch(function(e) {});
        }
      }

      // State update after navigation: {"A?":"B","Bk":{"A?":"A","Bk": page, "Bl": total, "Bp": [notes]}}
      if (obj['A?'] === 'B' && obj.Bk && typeof obj.Bk === 'object' && typeof obj.Bk.Bk === 'number') {
        var page = obj.Bk.Bk;
        var total = (typeof obj.Bk.Bl === 'number') ? obj.Bk.Bl : -1;
        var notes = null;
        if (obj.Bk.Bp && obj.Bk.Bp.length > 0) {
          for (var k = 0; k < obj.Bk.Bp.length; k++) {
            if (obj.Bk.Bp[k].A === page) {
              notes = obj.Bk.Bp[k].B || '';
              break;
            }
          }
        }
        tauriLog('STATE_UPDATE', { type: 'nav_update', currentPage: page, totalPages: total, hasNotes: notes !== null });
        var ipc3 = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
          || (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
        if (ipc3) {
          ipc3('update_canva_state', { currentPage: page, totalPages: total, notes: notes }).catch(function(e) {});
        }
      }
    } catch(e) {}
  }

  // --- Navigation helper (called from Rust via eval) ---
  window.__canvaNavigate = function(sessionId, pageIndex) {
    var body = { A: sessionId, C: 0, D: crypto.randomUUID() };
    if (pageIndex > 0) {
      body.B = pageIndex;
    }
    var xhr = new XMLHttpRequest();
    xhr.open('POST', '/_ajax/remotecontrol/remote/' + sessionId + '/navigate', true);
    xhr.setRequestHeader('content-type', 'application/json');
    xhr.setRequestHeader('x-canva-app', 'presentation_control');
    xhr.setRequestHeader('x-canva-request', 'createpagenavigationevent');
    xhr.setRequestHeader('x-canva-accept-prefix', 'no-prefix');
    xhr.send(JSON.stringify(body));
    tauriLog('NAV', { pageIndex: pageIndex, body: body });
  };

  window.__canvaCurrentPage = 0;

  tauriLog('INTERCEPT', 'All interceptors installed');
})();
"#;

/// Cached Canva presentation state
#[derive(Debug, Clone, Default)]
struct CanvaState {
    /// Whether the webview window is open
    webview_open: bool,
    /// Current page index (0-indexed)
    current_page: i32,
    /// Total pages (0 = unknown)
    total_pages: i32,
    /// Presenter notes for the current slide
    presenter_notes: Option<String>,
}

/// Canva adapter — singleton that holds webview reference and session state
pub struct CanvaAdapter {
    session_id: Arc<Mutex<Option<String>>>,
    state: Arc<Mutex<CanvaState>>,
    app_handle: tauri::AppHandle,
}

impl CanvaAdapter {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            session_id: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(CanvaState::default())),
            app_handle,
        }
    }

    /// Open a Canva remote control URL in a webview window
    pub fn open_remote(&self, url: &str) -> Result<(), String> {
        let parsed_url = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

        // Extract session ID from URL query param "id2"
        let session_id = parsed_url
            .query_pairs()
            .find(|(k, _)| k == "id2")
            .map(|(_, v)| v.to_string())
            .ok_or("URL must contain an id2 query parameter")?;

        // Close existing window if any
        if let Some(existing) = self.app_handle.get_webview_window("canva") {
            let _ = existing.close();
        }

        *self.session_id.lock().unwrap() = Some(session_id.clone());
        {
            let mut state = self.state.lock().unwrap();
            state.current_page = 0;
            state.webview_open = true;
        }

        WebviewWindowBuilder::new(&self.app_handle, "canva", tauri::WebviewUrl::External(parsed_url))
            .title("Canva Remote")
            .inner_size(400.0, 700.0)
            .initialization_script(INTERCEPT_SCRIPT)
            .build()
            .map_err(|e| format!("Failed to create window: {}", e))?;

        log::info!("Canva: Opened webview for session: {}", session_id);
        Ok(())
    }

    /// Close the Canva webview window
    pub fn close_remote(&self) {
        if let Some(window) = self.app_handle.get_webview_window("canva") {
            let _ = window.close();
        }
        *self.session_id.lock().unwrap() = None;
        let mut state = self.state.lock().unwrap();
        *state = CanvaState::default();
        log::info!("Canva: Closed webview");
    }

    /// Check if the webview window exists
    fn has_webview(&self) -> bool {
        self.app_handle.get_webview_window("canva").is_some()
    }

    /// Update state from webview WS binary frame parsing
    pub fn update_state(&self, current_page: i32, total_pages: i32, notes: Option<String>) {
        let mut state = self.state.lock().unwrap();
        state.current_page = current_page; // Already 0-indexed from Canva WS protocol
        if total_pages > 0 {
            state.total_pages = total_pages;
        }
        // Only update notes when we actually have content — avoids flicker from
        // the empty-then-populated message pattern Canva sends after navigation
        if notes.is_some() {
            state.presenter_notes = notes;
        }
        log::info!(
            "Canva: State updated - page {}/{}",
            current_page,
            state.total_pages
        );
    }

    /// Handle a log message from the webview intercept script
    pub fn handle_webview_log(&self, category: &str, message: &str) {
        // Always log at info level so it's visible without RUST_LOG=debug
        log::info!("[CANVA:{}] {}", category, message);

        // Emit to the main window so the frontend can see webview activity
        if let Some(main_window) = self.app_handle.get_webview_window("main") {
            let _ = main_window.emit("canva-webview-log", serde_json::json!({
                "category": category,
                "message": message,
            }));
        }
    }

    /// Navigate to a specific page by evaluating JS in the webview
    fn navigate_to_page(&self, page: i32) -> Result<(), String> {
        let session_id = self.session_id.lock().unwrap().clone()
            .ok_or("No Canva session open")?;

        let window = self.app_handle.get_webview_window("canva")
            .ok_or("Canva webview window not found")?;

        let js = format!("window.__canvaNavigate('{}', {});", session_id, page);
        window.eval(&js).map_err(|e| format!("Failed to navigate: {}", e))?;

        // Update cached state
        {
            let mut state = self.state.lock().unwrap();
            state.current_page = page;
        }

        Ok(())
    }
}

impl PresentationAdapter for CanvaAdapter {
    fn get_open_presentations(&self) -> Result<Vec<String>, String> {
        if self.has_webview() && self.session_id.lock().unwrap().is_some() {
            Ok(vec!["Canva Presentation".to_string()])
        } else {
            Ok(vec![])
        }
    }

    fn get_presentation_state(&self, _name: &str) -> Result<PresentationState, String> {
        let has_session = self.session_id.lock().unwrap().is_some();
        let has_webview = self.has_webview();
        Ok(PresentationState {
            is_open: has_webview && has_session,
            is_presenting: has_webview && has_session,
        })
    }

    fn get_slide_info(&self, _name: &str) -> Result<SlideInfo, String> {
        let state = self.state.lock().unwrap();
        Ok(SlideInfo {
            current: state.current_page + 1, // Convert 0-indexed to 1-indexed
            total: state.total_pages,
        })
    }

    fn next_slide(&self, _name: &str) -> Result<SlideInfo, String> {
        let current_page = {
            let state = self.state.lock().unwrap();
            state.current_page
        };

        let next_page = current_page + 1;
        self.navigate_to_page(next_page)?;

        Ok(SlideInfo {
            current: next_page + 1,
            total: self.state.lock().unwrap().total_pages,
        })
    }

    fn goto_slide(&self, _name: &str, slide: i32) -> Result<SlideInfo, String> {
        let page = slide - 1; // Convert 1-indexed to 0-indexed
        self.navigate_to_page(page)?;
        Ok(SlideInfo {
            current: slide,
            total: self.state.lock().unwrap().total_pages,
        })
    }

    fn prev_slide(&self, _name: &str) -> Result<SlideInfo, String> {
        let current_page = {
            let state = self.state.lock().unwrap();
            state.current_page
        };

        if current_page <= 0 {
            return Ok(SlideInfo {
                current: 1,
                total: self.state.lock().unwrap().total_pages,
            });
        }

        let prev_page = current_page - 1;
        self.navigate_to_page(prev_page)?;

        Ok(SlideInfo {
            current: prev_page + 1,
            total: self.state.lock().unwrap().total_pages,
        })
    }

    fn get_live_status(&self, _name: &str) -> LiveStatus {
        let state = self.state.lock().unwrap();
        let has_session = self.session_id.lock().unwrap().is_some();
        let has_webview = self.has_webview();
        let is_open = has_webview && has_session;

        if !is_open {
            return LiveStatus {
                is_open: false,
                is_presenting: false,
                current_slide: 0,
                total_slides: 0,
                zoom_level: None,
                presenter_notes: None,
            };
        }

        LiveStatus {
            is_open: true,
            is_presenting: true,
            current_slide: state.current_page + 1,
            total_slides: state.total_pages,
            zoom_level: None,
            presenter_notes: state.presenter_notes.clone(),
        }
    }

    fn connection_status(&self) -> ConnectionStatus {
        let has_session = self.session_id.lock().unwrap().is_some();
        let has_webview = self.has_webview();

        if has_webview && has_session {
            ConnectionStatus::Connected
        } else if has_webview {
            ConnectionStatus::Connecting
        } else {
            ConnectionStatus::Disconnected
        }
    }
}
