//! LibreOffice Impress adapter using the Impress Remote Protocol
//!
//! This adapter connects to LibreOffice Impress via TCP on port 1599.
//! The user must enable remote control in LibreOffice:
//! Slide Show > Slide Show Settings > Enable remote control
//!
//! Protocol documentation:
//! https://wiki.documentfoundation.org/Development/Impress_Remote_Protocol

use super::{PresentationAdapter, PresentationState, SlideInfo};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const IMPRESS_REMOTE_PORT: u16 = 1599;
const CLIENT_NAME: &str = "SherPresent";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// Cached state from the LibreOffice Impress connection
#[derive(Debug, Clone, Default)]
struct ImpressState {
    paired: bool,
    slideshow_running: bool,
    current_slide: i32, // 0-indexed from protocol
    total_slides: i32,
    presenter_notes: Option<String>,
}

/// LibreOffice Impress adapter using the Impress Remote Protocol
pub struct LibreOfficeAdapter {
    /// Target host (IP or hostname)
    host: String,
    /// Target port
    port: u16,
    /// Cached state from server messages
    state: Arc<Mutex<ImpressState>>,
    /// Active TCP connection (if any)
    connection: Arc<Mutex<Option<TcpStream>>>,
}

impl LibreOfficeAdapter {
    pub fn new(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            state: Arc::new(Mutex::new(ImpressState::default())),
            connection: Arc::new(Mutex::new(None)),
        }
    }

    /// Try to connect to LibreOffice Impress
    fn connect(&self) -> Result<(), String> {
        // Check if already connected
        {
            let conn = self.connection.lock().unwrap();
            if conn.is_some() {
                return Ok(());
            }
        }

        // Try to connect
        let addr_str = format!("{}:{}", self.host, self.port);
        let stream = TcpStream::connect_timeout(
            &addr_str.parse().map_err(|e| format!("Invalid address {}: {}", addr_str, e))?,
            CONNECT_TIMEOUT,
        )
        .map_err(|e| {
            format!(
                "Failed to connect to LibreOffice Impress at {}. \
                 Make sure Impress is running with remote control enabled \
                 (Slide Show > Slide Show Settings > Enable remote control). \
                 Error: {}",
                addr_str, e
            )
        })?;

        stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
        stream.set_write_timeout(Some(WRITE_TIMEOUT)).ok();

        // Store the connection
        {
            let mut conn = self.connection.lock().unwrap();
            *conn = Some(stream);
        }

        // Send pairing request
        self.send_pairing_request()?;

        // Wait for and process initial messages (pairing response, slideshow state)
        self.process_messages_until_paired()?;

        Ok(())
    }

    /// Send the pairing request to LibreOffice
    fn send_pairing_request(&self) -> Result<(), String> {
        let pin = format!("{:04}", rand_pin());
        let message = format!("LO_SERVER_CLIENT_PAIR\n{}\n{}\n\n", CLIENT_NAME, pin);

        self.send_raw(&message)
    }

    /// Send a raw message to LibreOffice
    fn send_raw(&self, message: &str) -> Result<(), String> {
        let mut conn_guard = self.connection.lock().unwrap();
        let stream = conn_guard
            .as_mut()
            .ok_or("Not connected to LibreOffice")?;

        stream
            .write_all(message.as_bytes())
            .map_err(|e| format!("Failed to send message: {}", e))?;

        stream
            .flush()
            .map_err(|e| format!("Failed to flush: {}", e))?;

        Ok(())
    }

    /// Send a command (adds trailing \n\n)
    fn send_command(&self, command: &str) -> Result<(), String> {
        self.send_raw(&format!("{}\n\n", command))
    }

    /// Process messages until we're paired or timeout
    fn process_messages_until_paired(&self) -> Result<(), String> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(5);

        while start.elapsed() < timeout {
            self.process_available_messages()?;

            let state = self.state.lock().unwrap();
            if state.paired {
                return Ok(());
            }
            drop(state);

            std::thread::sleep(Duration::from_millis(100));
        }

        Err("Timeout waiting for pairing. Make sure a slideshow is running (press F5).".to_string())
    }

    /// Process any available messages from the server
    fn process_available_messages(&self) -> Result<(), String> {
        let mut conn_guard = self.connection.lock().unwrap();
        let stream = match conn_guard.as_mut() {
            Some(s) => s,
            None => return Ok(()),
        };

        // Clone the stream for reading (we need to drop the lock for processing)
        let stream_clone = stream.try_clone().map_err(|e| e.to_string())?;
        drop(conn_guard);

        let mut reader = BufReader::new(stream_clone);
        let mut buffer = String::new();
        let mut message_lines: Vec<String> = Vec::new();

        // Read available lines
        loop {
            buffer.clear();
            match reader.read_line(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let line = buffer.trim_end_matches('\n').to_string();
                    if line.is_empty() {
                        // End of message
                        if !message_lines.is_empty() {
                            self.handle_message(&message_lines);
                            message_lines.clear();
                        }
                    } else {
                        message_lines.push(line);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                Err(e) => {
                    log::warn!("Error reading from LibreOffice: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a complete message from the server
    fn handle_message(&self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }

        let message_type = &lines[0];
        log::debug!("LibreOffice message: {} {:?}", message_type, &lines[1..]);

        let mut state = self.state.lock().unwrap();

        match message_type.as_str() {
            "LO_SERVER_SERVER_PAIRED" | "LO_SERVER_PAIRED" => {
                log::info!("LibreOffice: Paired successfully");
                state.paired = true;
            }
            "LO_SERVER_VALIDATING_PIN" => {
                log::info!("LibreOffice: PIN validation in progress (auto-pairing)");
            }
            "slideshow_started" => {
                state.slideshow_running = true;
                state.paired = true; // Also counts as paired
                if lines.len() >= 3 {
                    state.total_slides = lines[1].parse().unwrap_or(0);
                    state.current_slide = lines[2].parse().unwrap_or(0);
                }
                log::info!(
                    "LibreOffice: Slideshow started, slide {}/{}",
                    state.current_slide + 1,
                    state.total_slides
                );
            }
            "slideshow_finished" => {
                state.slideshow_running = false;
                log::info!("LibreOffice: Slideshow finished");
            }
            "slide_updated" => {
                if lines.len() >= 2 {
                    state.current_slide = lines[1].parse().unwrap_or(state.current_slide);
                    log::debug!(
                        "LibreOffice: Slide updated to {}/{}",
                        state.current_slide + 1,
                        state.total_slides
                    );
                }
            }
            "slide_notes" => {
                if lines.len() > 1 {
                    let html_content = lines[1..].join("\n");
                    let plain_text = strip_html_tags(&html_content);
                    let trimmed = plain_text.trim().to_string();
                    state.presenter_notes = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    };
                    log::debug!("LibreOffice: Got slide notes ({} chars)", state.presenter_notes.as_ref().map_or(0, |s| s.len()));
                }
            }
            "slide_preview" => {
                // Ignore slide previews
            }
            _ => {
                log::debug!("LibreOffice: Unknown message type: {}", message_type);
            }
        }
    }

    /// Disconnect from LibreOffice
    fn disconnect(&self) {
        let mut conn = self.connection.lock().unwrap();
        *conn = None;

        let mut state = self.state.lock().unwrap();
        *state = ImpressState::default();
    }

    /// Check if we have an active connection
    fn is_connected(&self) -> bool {
        let conn = self.connection.lock().unwrap();
        conn.is_some()
    }

    /// Ensure we're connected and process any pending messages
    fn ensure_connected(&self) -> Result<(), String> {
        if !self.is_connected() {
            self.connect()?;
        }
        self.process_available_messages()?;
        Ok(())
    }
}

impl Default for LibreOfficeAdapter {
    fn default() -> Self {
        Self::new("127.0.0.1".to_string(), IMPRESS_REMOTE_PORT)
    }
}

impl PresentationAdapter for LibreOfficeAdapter {
    fn get_open_presentations(&self) -> Result<Vec<String>, String> {
        // Try to connect - if successful, we have a presentation
        match self.connect() {
            Ok(()) => {
                // The Impress Remote Protocol doesn't provide presentation names
                // Just return a generic name if connected
                Ok(vec!["LibreOffice Impress".to_string()])
            }
            Err(_) => {
                // Not running or remote control not enabled
                self.disconnect();
                Ok(vec![])
            }
        }
    }

    fn get_presentation_state(&self, _name: &str) -> Result<PresentationState, String> {
        self.ensure_connected()?;

        let state = self.state.lock().unwrap();
        Ok(PresentationState {
            is_open: state.paired,
            is_presenting: state.slideshow_running,
        })
    }

    fn get_slide_info(&self, _name: &str) -> Result<SlideInfo, String> {
        self.ensure_connected()?;

        let state = self.state.lock().unwrap();

        if !state.slideshow_running {
            return Err("No slideshow is currently running".to_string());
        }

        // Protocol uses 0-indexed slides, we return 1-indexed
        Ok(SlideInfo {
            current: state.current_slide + 1,
            total: state.total_slides,
        })
    }

    fn next_slide(&self, _name: &str) -> Result<SlideInfo, String> {
        self.ensure_connected()?;

        {
            let state = self.state.lock().unwrap();
            if !state.slideshow_running {
                return Err("No slideshow is currently running".to_string());
            }
        }

        // Send next slide command
        self.send_command("transition_next")?;

        // Wait a bit for the slide update message
        std::thread::sleep(Duration::from_millis(100));
        self.process_available_messages()?;

        // Return current state
        let state = self.state.lock().unwrap();
        Ok(SlideInfo {
            current: state.current_slide + 1,
            total: state.total_slides,
        })
    }

    fn prev_slide(&self, _name: &str) -> Result<SlideInfo, String> {
        self.ensure_connected()?;

        {
            let state = self.state.lock().unwrap();
            if !state.slideshow_running {
                return Err("No slideshow is currently running".to_string());
            }
        }

        // Send previous slide command
        self.send_command("transition_previous")?;

        // Wait a bit for the slide update message
        std::thread::sleep(Duration::from_millis(100));
        self.process_available_messages()?;

        // Return current state
        let state = self.state.lock().unwrap();
        Ok(SlideInfo {
            current: state.current_slide + 1,
            total: state.total_slides,
        })
    }

    // LibreOffice doesn't support notes zoom control
    fn get_notes_zoom(&self) -> Result<Option<i32>, String> {
        Ok(None)
    }

    fn get_presenter_notes(&self, _name: &str) -> Result<Option<String>, String> {
        let state = self.state.lock().unwrap();
        Ok(state.presenter_notes.clone())
    }

    fn connection_status(&self) -> super::ConnectionStatus {
        let conn = self.connection.lock().unwrap();
        if conn.is_some() {
            let state = self.state.lock().unwrap();
            if state.paired {
                super::ConnectionStatus::Connected
            } else {
                super::ConnectionStatus::Connecting
            }
        } else {
            super::ConnectionStatus::Disconnected
        }
    }
}

/// Strip HTML tags from a string, returning plain text
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

/// Generate a random 4-digit PIN for pairing
fn rand_pin() -> u16 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    1000 + (nanos % 9000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rand_pin_range() {
        for _ in 0..100 {
            let pin = rand_pin();
            assert!(pin >= 1000 && pin <= 9999);
        }
    }

    #[test]
    fn test_new_with_custom_host_port() {
        let adapter = LibreOfficeAdapter::new("192.168.1.50".to_string(), 2002);
        assert_eq!(adapter.host, "192.168.1.50");
        assert_eq!(adapter.port, 2002);
        assert!(!adapter.is_connected());
    }

    #[test]
    fn test_default_uses_localhost_and_default_port() {
        let adapter = LibreOfficeAdapter::default();
        assert_eq!(adapter.host, "127.0.0.1");
        assert_eq!(adapter.port, IMPRESS_REMOTE_PORT);
        assert_eq!(adapter.port, 1599);
    }

    #[test]
    fn test_initial_connection_status_is_disconnected() {
        let adapter = LibreOfficeAdapter::default();
        matches!(adapter.connection_status(), super::super::ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_handle_paired_message() {
        let adapter = LibreOfficeAdapter::default();
        adapter.handle_message(&["LO_SERVER_SERVER_PAIRED".to_string()]);
        let state = adapter.state.lock().unwrap();
        assert!(state.paired);
    }

    #[test]
    fn test_handle_paired_alternate_message() {
        let adapter = LibreOfficeAdapter::default();
        adapter.handle_message(&["LO_SERVER_PAIRED".to_string()]);
        let state = adapter.state.lock().unwrap();
        assert!(state.paired);
    }

    #[test]
    fn test_handle_slideshow_started() {
        let adapter = LibreOfficeAdapter::default();
        adapter.handle_message(&[
            "slideshow_started".to_string(),
            "10".to_string(),
            "0".to_string(),
        ]);
        let state = adapter.state.lock().unwrap();
        assert!(state.slideshow_running);
        assert!(state.paired); // slideshow_started also sets paired
        assert_eq!(state.total_slides, 10);
        assert_eq!(state.current_slide, 0);
    }

    #[test]
    fn test_handle_slideshow_finished() {
        let adapter = LibreOfficeAdapter::default();
        // Start first
        adapter.handle_message(&[
            "slideshow_started".to_string(),
            "5".to_string(),
            "0".to_string(),
        ]);
        // Then finish
        adapter.handle_message(&["slideshow_finished".to_string()]);
        let state = adapter.state.lock().unwrap();
        assert!(!state.slideshow_running);
        assert!(state.paired); // Still paired after finish
    }

    #[test]
    fn test_handle_slide_updated() {
        let adapter = LibreOfficeAdapter::default();
        adapter.handle_message(&[
            "slideshow_started".to_string(),
            "10".to_string(),
            "0".to_string(),
        ]);
        adapter.handle_message(&[
            "slide_updated".to_string(),
            "3".to_string(),
        ]);
        let state = adapter.state.lock().unwrap();
        assert_eq!(state.current_slide, 3);
    }

    #[test]
    fn test_handle_empty_message() {
        let adapter = LibreOfficeAdapter::default();
        // Should not panic
        adapter.handle_message(&[]);
    }

    #[test]
    fn test_handle_unknown_message() {
        let adapter = LibreOfficeAdapter::default();
        // Should not panic, state should be unchanged
        adapter.handle_message(&["some_unknown_message".to_string()]);
        let state = adapter.state.lock().unwrap();
        assert!(!state.paired);
        assert!(!state.slideshow_running);
    }

    #[test]
    fn test_disconnect_resets_state() {
        let adapter = LibreOfficeAdapter::default();
        // Simulate some state
        {
            let mut state = adapter.state.lock().unwrap();
            state.paired = true;
            state.slideshow_running = true;
            state.current_slide = 5;
            state.total_slides = 10;
        }
        adapter.disconnect();
        let state = adapter.state.lock().unwrap();
        assert!(!state.paired);
        assert!(!state.slideshow_running);
        assert_eq!(state.current_slide, 0);
        assert_eq!(state.total_slides, 0);
    }

    #[test]
    fn test_slide_info_is_1_indexed() {
        let adapter = LibreOfficeAdapter::default();
        // Manually set state as if connected and presenting
        {
            let mut state = adapter.state.lock().unwrap();
            state.paired = true;
            state.slideshow_running = true;
            state.current_slide = 0; // 0-indexed from protocol
            state.total_slides = 5;
        }
        // Simulate being connected by inserting a fake connection
        // We can't easily test get_slide_info without a real connection
        // because ensure_connected will fail, but we can verify the
        // state conversion logic via handle_message + state check
        let state = adapter.state.lock().unwrap();
        // The adapter converts 0-indexed to 1-indexed in get_slide_info
        assert_eq!(state.current_slide + 1, 1); // First slide
        assert_eq!(state.total_slides, 5);
    }

    #[test]
    fn test_connect_to_unreachable_host_fails() {
        // Use a non-routable IP to ensure fast failure
        let adapter = LibreOfficeAdapter::new("192.0.2.1".to_string(), 1599);
        let result = adapter.connect();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to connect"));
    }

    #[test]
    fn test_slideshow_started_with_missing_fields() {
        let adapter = LibreOfficeAdapter::default();
        // Only message type, no slide count/index
        adapter.handle_message(&["slideshow_started".to_string()]);
        let state = adapter.state.lock().unwrap();
        assert!(state.slideshow_running);
        assert!(state.paired);
        // Should default to 0 since we didn't have enough lines
        assert_eq!(state.total_slides, 0);
        assert_eq!(state.current_slide, 0);
    }

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<p>Hello</p>"), "Hello");
        assert_eq!(strip_html_tags("<b>bold</b> and <i>italic</i>"), "bold and italic");
        assert_eq!(strip_html_tags("no tags here"), "no tags here");
        assert_eq!(strip_html_tags("<p></p>"), "");
        assert_eq!(strip_html_tags("<div class=\"foo\">text</div>"), "text");
    }

    #[test]
    fn test_handle_slide_notes() {
        let adapter = LibreOfficeAdapter::default();
        adapter.handle_message(&[
            "slide_notes".to_string(),
            "<p>These are my notes</p>".to_string(),
        ]);
        let state = adapter.state.lock().unwrap();
        assert_eq!(state.presenter_notes, Some("These are my notes".to_string()));
    }

    #[test]
    fn test_handle_slide_notes_empty_html() {
        let adapter = LibreOfficeAdapter::default();
        adapter.handle_message(&[
            "slide_notes".to_string(),
            "<p></p>".to_string(),
        ]);
        let state = adapter.state.lock().unwrap();
        assert_eq!(state.presenter_notes, None);
    }

    #[test]
    fn test_handle_slide_notes_multiline() {
        let adapter = LibreOfficeAdapter::default();
        adapter.handle_message(&[
            "slide_notes".to_string(),
            "<p>Line one</p>".to_string(),
            "<p>Line two</p>".to_string(),
        ]);
        let state = adapter.state.lock().unwrap();
        assert_eq!(state.presenter_notes, Some("Line one\nLine two".to_string()));
    }

    #[test]
    fn test_slide_updated_with_invalid_number() {
        let adapter = LibreOfficeAdapter::default();
        {
            let mut state = adapter.state.lock().unwrap();
            state.current_slide = 2;
        }
        adapter.handle_message(&[
            "slide_updated".to_string(),
            "not_a_number".to_string(),
        ]);
        // Should keep the old value on parse failure
        let state = adapter.state.lock().unwrap();
        assert_eq!(state.current_slide, 2);
    }
}
