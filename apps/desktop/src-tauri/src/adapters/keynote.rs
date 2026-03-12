use super::{PresentationAdapter, PresentationState, SlideInfo};
use crate::applescript::run_applescript;

/// Keynote adapter for macOS - constructed conditionally in get_adapter()
#[allow(dead_code)]
pub struct KeynoteAdapter;

impl PresentationAdapter for KeynoteAdapter {
    fn get_open_presentations(&self) -> Result<Vec<String>, String> {
        let script = r#"tell application "Keynote" to get name of every document"#;

        match run_applescript(script) {
            Ok(result) if result.is_empty() => Ok(vec![]),
            Ok(result) => {
                // AppleScript returns comma-separated list
                Ok(result.split(", ").map(|s| s.trim().to_string()).collect())
            }
            Err(_) => Ok(vec![]), // Keynote not running or no documents
        }
    }

    fn get_presentation_state(&self, name: &str) -> Result<PresentationState, String> {
        let script = format!(
            r#"tell application "Keynote"
                set isOpen to false
                set isPresenting to false

                try
                    set doc to document "{}"
                    set isOpen to true
                    set isPresenting to playing
                end try

                return (isOpen as text) & "," & (isPresenting as text)
            end tell"#,
            name
        );

        let result = run_applescript(&script)?;
        let parts: Vec<&str> = result.split(',').collect();

        if parts.len() != 2 {
            return Err("Unexpected response format".to_string());
        }

        Ok(PresentationState {
            is_open: parts[0].trim() == "true",
            is_presenting: parts[1].trim() == "true",
        })
    }

    fn get_slide_info(&self, name: &str) -> Result<SlideInfo, String> {
        let script = format!(
            r#"tell application "Keynote"
                tell document "{}"
                    set currentNum to slide number of current slide
                    set totalSlides to count slides
                    return (currentNum as text) & "," & (totalSlides as text)
                end tell
            end tell"#,
            name
        );

        let result = run_applescript(&script)?;
        let parts: Vec<&str> = result.split(',').collect();

        if parts.len() != 2 {
            return Err("Unexpected response format".to_string());
        }

        let current = parts[0]
            .trim()
            .parse()
            .map_err(|_| "Failed to parse current slide")?;
        let total = parts[1]
            .trim()
            .parse()
            .map_err(|_| "Failed to parse total slides")?;

        Ok(SlideInfo { current, total })
    }

    fn next_slide(&self, name: &str) -> Result<SlideInfo, String> {
        let script = format!(
            r#"tell application "Keynote"
                tell document "{}"
                    set oldPos to slide number of current slide
                    set totalSlides to count slides
                    if oldPos >= totalSlides then
                        return "BOUNDARY," & (oldPos as text) & "," & (totalSlides as text)
                    end if
                    show next
                    set newPos to slide number of current slide
                    return "OK," & (newPos as text) & "," & (totalSlides as text)
                end tell
            end tell"#,
            name
        );

        let result = run_applescript(&script)?;
        let parts: Vec<&str> = result.split(',').collect();

        if parts.len() != 3 {
            return Err("Unexpected response format".to_string());
        }

        let current = parts[1]
            .trim()
            .parse()
            .map_err(|_| "Failed to parse current slide")?;
        let total = parts[2]
            .trim()
            .parse()
            .map_err(|_| "Failed to parse total slides")?;

        Ok(SlideInfo { current, total })
    }

    fn prev_slide(&self, name: &str) -> Result<SlideInfo, String> {
        let script = format!(
            r#"tell application "Keynote"
                tell document "{}"
                    set oldPos to slide number of current slide
                    set totalSlides to count slides
                    if oldPos <= 1 then
                        return "BOUNDARY," & (oldPos as text) & "," & (totalSlides as text)
                    end if
                    show previous
                    set newPos to slide number of current slide
                    return "OK," & (newPos as text) & "," & (totalSlides as text)
                end tell
            end tell"#,
            name
        );

        let result = run_applescript(&script)?;
        let parts: Vec<&str> = result.split(',').collect();

        if parts.len() != 3 {
            return Err("Unexpected response format".to_string());
        }

        let current = parts[1]
            .trim()
            .parse()
            .map_err(|_| "Failed to parse current slide")?;
        let total = parts[2]
            .trim()
            .parse()
            .map_err(|_| "Failed to parse total slides")?;

        Ok(SlideInfo { current, total })
    }

    // Keynote doesn't support notes zoom control
}
