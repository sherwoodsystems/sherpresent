use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{webview::WebviewWindowBuilder, Emitter, Manager};

const INTERCEPT_SCRIPT: &str = r#"
(function() {
  var __ipcWarned = false;
  var SKIP_CATEGORIES = new Set([
    'DOM_ATTR',
    'CANVAS_CONTEXT',
    'ANIMATION_CSS',
    'ANIMATION_RAF',
    'ANIMATION_WEB',
  ]);
  function tauriLog(category, data) {
    if (SKIP_CATEGORIES.has(category)) return;
    const msg = typeof data === 'string' ? data : JSON.stringify(data);
    const ipc = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
      || (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
    if (ipc) {
      ipc('log_from_webview', { category: category, message: msg }).catch(function(e) {});
    } else if (!__ipcWarned) {
      __ipcWarned = true;
      console.warn('[CANVA-ANALYZER] Tauri IPC bridge not available');
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
          var hex = Array.from(bytes.slice(0, 500)).map(function(b) { return b.toString(16).padStart(2, '0'); }).join(' ');
          tauriLog('WS_RECV_BIN', { url: url, size: bytes.length, hex: hex });
        });
      } else if (data instanceof ArrayBuffer) {
        var bytes = new Uint8Array(data);
        var hex = Array.from(bytes.slice(0, 500)).map(function(b) { return b.toString(16).padStart(2, '0'); }).join(' ');
        tauriLog('WS_RECV_BIN', { url: url, size: bytes.length, hex: hex });
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
        var hex = Array.from(bytes.slice(0, 500)).map(function(b) { return b.toString(16).padStart(2, '0'); }).join(' ');
        tauriLog('WS_SEND_BIN', { url: url, size: bytes.length, hex: hex });
      } else if (data instanceof Uint8Array) {
        var hex2 = Array.from(data.slice(0, 500)).map(function(b) { return b.toString(16).padStart(2, '0'); }).join(' ');
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

    var FETCH_BODY_PATTERNS = [
      '/documents/',
      '/remotecontrol/',
      '/livepresentation/',
      'media-public.canva.com',
    ];
    return origFetch.apply(this, args).then(function(response) {
      tauriLog('FETCH_RES', { method: method, url: url, status: response.status });
      var shouldCapture = FETCH_BODY_PATTERNS.some(function(p) { return url.includes(p); });
      if (shouldCapture) {
        response.clone().text().then(function(body) {
          tauriLog('FETCH_BODY', { method: method, url: url, status: response.status, body: body });
        });
      }
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
      var FULL_BODY_PATTERNS = ['/documents/', '/remotecontrol/', '/livepresentation/'];
      var xhrUrl = this._logUrl;
      this.addEventListener('load', function() {
        var isFullBody = FULL_BODY_PATTERNS.some(function(p) { return xhrUrl.includes(p); });
        var respBody = '';
        try { respBody = isFullBody ? (this.responseText || '') : (this.responseText || '').substring(0, 4000); } catch(e) {}
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

  // --- Animation observer ---
  var rafCount = 0;
  const origRAF = window.requestAnimationFrame;
  window.requestAnimationFrame = function(callback) {
    rafCount++;
    return origRAF.call(window, callback);
  };
  setInterval(function() {
    if (rafCount > 0) {
      tauriLog('ANIMATION_RAF', { callsPerSecond: rafCount });
      rafCount = 0;
    }
  }, 1000);

  // Hook Element.animate (Web Animations API)
  const origAnimate = Element.prototype.animate;
  if (origAnimate) {
    Element.prototype.animate = function(keyframes, options) {
      tauriLog('ANIMATION_WEB', {
        tag: this.tagName,
        id: this.id || null,
        className: (this.className && typeof this.className === 'string') ? this.className.substring(0, 200) : null,
        options: typeof options === 'object' ? JSON.stringify(options).substring(0, 500) : String(options)
      });
      return origAnimate.apply(this, arguments);
    };
  }

  // Listen for CSS animation/transition events
  document.addEventListener('animationstart', function(e) {
    tauriLog('ANIMATION_CSS', { event: 'animationstart', name: e.animationName, tag: e.target.tagName, id: e.target.id || null });
  }, true);
  document.addEventListener('transitionstart', function(e) {
    tauriLog('TRANSITION_CSS', { event: 'transitionstart', property: e.propertyName, tag: e.target.tagName, id: e.target.id || null });
  }, true);

  // --- DOM mutation observer ---
  var mutationLog = { added: 0, removed: 0, attributes: 0, text: 0 };
  var observer = new MutationObserver(function(mutations) {
    mutations.forEach(function(m) {
      if (m.type === 'childList') {
        mutationLog.added += m.addedNodes.length;
        mutationLog.removed += m.removedNodes.length;
      } else if (m.type === 'attributes') {
        mutationLog.attributes++;
        if (m.attributeName === 'style' || m.attributeName === 'class') {
          var target = m.target;
          tauriLog('DOM_ATTR', {
            attr: m.attributeName,
            tag: target.tagName,
            id: target.id || null,
            value: (target.getAttribute(m.attributeName) || '').substring(0, 300)
          });
        }
      } else if (m.type === 'characterData') {
        mutationLog.text++;
      }
    });
  });
  function startObserver() {
    observer.observe(document.documentElement, {
      childList: true, subtree: true,
      attributes: true, attributeFilter: ['style', 'class', 'data-state', 'aria-hidden'],
      characterData: true
    });
    setInterval(function() {
      if (mutationLog.added > 0 || mutationLog.removed > 0 || mutationLog.attributes > 0 || mutationLog.text > 0) {
        tauriLog('DOM_SUMMARY', mutationLog);
        mutationLog = { added: 0, removed: 0, attributes: 0, text: 0 };
      }
    }, 1000);
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', startObserver);
  } else {
    startObserver();
  }

  // --- Canvas/WebGL detection ---
  const origGetContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function(type) {
    tauriLog('CANVAS_CONTEXT', {
      type: type,
      width: this.width,
      height: this.height,
      id: this.id || null,
      className: (this.className || '').substring(0, 200)
    });
    return origGetContext.apply(this, arguments);
  };

  var ipcAvailable = !!(
    (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
    || (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke)
  );
  tauriLog('INTERCEPT', 'All interceptors installed (IPC available: ' + ipcAvailable + ')');
})();
"#;

/// Holds the log file path and canva window state
struct AppState {
    canva_open: bool,
    log_file: PathBuf,
    entry_count: u64,
    category_counts: std::collections::HashMap<String, u64>,
}

struct ManagedState(Mutex<AppState>);

fn log_dir() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("canva-for-av")
        .join("captures");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn new_log_file() -> PathBuf {
    let now = chrono::Local::now();
    log_dir().join(format!("canva-capture-{}.jsonl", now.format("%Y%m%d-%H%M%S")))
}

#[tauri::command]
fn open_canva(app: tauri::AppHandle, state: tauri::State<'_, ManagedState>) -> Result<String, String> {
    if let Some(existing) = app.get_webview_window("canva") {
        let _ = existing.close();
    }

    // Start a new capture file for this session
    let log_path = new_log_file();
    {
        let mut s = state.0.lock().unwrap();
        s.canva_open = true;
        s.log_file = log_path.clone();
        s.entry_count = 0;
        s.category_counts.clear();
    }

    let url = tauri::Url::parse("https://www.canva.com").unwrap();
    WebviewWindowBuilder::new(&app, "canva", tauri::WebviewUrl::External(url))
        .title("Canva Analyzer")
        .inner_size(1280.0, 900.0)
        .initialization_script(INTERCEPT_SCRIPT)
        .build()
        .map_err(|e| format!("Failed to create canva window: {}", e))?;

    log::info!("Canva analyzer window opened, writing to {:?}", log_path);
    Ok(log_path.to_string_lossy().to_string())
}

#[tauri::command]
fn close_canva(app: tauri::AppHandle, state: tauri::State<'_, ManagedState>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("canva") {
        window.close().map_err(|e| format!("Failed to close: {}", e))?;
    }
    let mut s = state.0.lock().unwrap();
    s.canva_open = false;
    log::info!("Canva analyzer window closed. {} entries captured to {:?}", s.entry_count, s.log_file);
    Ok(())
}

#[tauri::command]
fn log_from_webview(app: tauri::AppHandle, state: tauri::State<'_, ManagedState>, category: String, message: String) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let line = serde_json::json!({
        "ts": now,
        "cat": category,
        "msg": message,
    });

    let mut s = state.0.lock().unwrap();
    s.entry_count += 1;
    *s.category_counts.entry(category.clone()).or_insert(0) += 1;

    // Append to JSONL file
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&s.log_file) {
        let _ = writeln!(file, "{}", line);
    }

    drop(s);

    // Emit lightweight summary to UI (not every log line — just counts)
    if let Some(main_window) = app.get_webview_window("main") {
        let s = state.0.lock().unwrap();
        // Only emit summary updates every 50 entries to avoid flooding
        if s.entry_count % 50 == 0 || category.starts_with("STATE") || category == "INTERCEPT" {
            let _ = main_window.emit("canva-stats", serde_json::json!({
                "totalEntries": s.entry_count,
                "categories": s.category_counts,
                "logFile": s.log_file.to_string_lossy(),
            }));
        }
    }
}

#[tauri::command]
fn get_stats(state: tauri::State<'_, ManagedState>) -> serde_json::Value {
    let s = state.0.lock().unwrap();
    serde_json::json!({
        "totalEntries": s.entry_count,
        "categories": s.category_counts,
        "logFile": s.log_file.to_string_lossy(),
        "canvaOpen": s.canva_open,
    })
}

#[tauri::command]
fn get_log_dir() -> String {
    log_dir().to_string_lossy().to_string()
}

#[tauri::command]
fn list_captures() -> Vec<serde_json::Value> {
    let dir = log_dir();
    let mut files: Vec<_> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some(serde_json::json!({
                "name": e.file_name().to_string_lossy(),
                "path": e.path().to_string_lossy(),
                "size": meta.len(),
            }))
        })
        .collect();
    files.sort_by(|a, b| b["name"].as_str().cmp(&a["name"].as_str()));
    files
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let initial_state = ManagedState(Mutex::new(AppState {
        canva_open: false,
        log_file: log_dir().join("no-session.jsonl"),
        entry_count: 0,
        category_counts: std::collections::HashMap::new(),
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(initial_state)
        .invoke_handler(tauri::generate_handler![
            open_canva,
            close_canva,
            log_from_webview,
            get_stats,
            get_log_dir,
            list_captures,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
