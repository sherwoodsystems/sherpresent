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

const IMPRESS_REMOTE_PORT: u16 = 1599;
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
}

/// LibreOffice Impress adapter using the Impress Remote Protocol
pub struct LibreOfficeAdapter {
    /// Cached state from server messages
    state: Arc<Mutex<ImpressState>>,
    /// Active TCP connection (if any)
    connection: Arc<Mutex<Option<TcpStream>>>,
}

impl LibreOfficeAdapter {
    pub fn new() -> Self {
        Self {
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
        let stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", IMPRESS_REMOTE_PORT)
                .parse()
                .unwrap(),
            CONNECT_TIMEOUT,
        )
        .map_err(|e| {
            format!(
                "Failed to connect to LibreOffice Impress on port {}. \
                 Make sure Impress is running with remote control enabled \
                 (Slide Show > Slide Show Settings > Enable remote control). \
                 Error: {}",
                IMPRESS_REMOTE_PORT, e
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
            "slide_notes" | "slide_preview" => {
                // Ignore these for now
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
        Self::new()
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
}
