use std::collections::HashMap;
use super::{LiveStatus, PresentationAdapter, PresentationState, SlideInfo, parse_notes_response};
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
                    set totalSlides to count (every slide whose skipped is false)
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

        Ok(SlideInfo { current, total, transition_duration: None })
    }

    fn next_slide(&self, name: &str) -> Result<SlideInfo, String> {
        let script = format!(
            r#"tell application "Keynote"
                tell document "{}"
                    set oldPos to slide number of current slide
                    set totalSlides to count (every slide whose skipped is false)
                    -- Read outgoing slide's transition duration BEFORE advancing
                    set tDur to 0.0
                    try
                        set tDur to transition duration of transition properties of current slide
                    end try
                    if oldPos >= totalSlides then
                        return "BOUNDARY," & (oldPos as text) & "," & (totalSlides as text) & "," & (tDur as text)
                    end if
                    show next
                    -- Read immediately — may still be old value during transition, frontend handles it
                    set newPos to slide number of current slide
                    return "OK," & (newPos as text) & "," & (totalSlides as text) & "," & (tDur as text)
                end tell
            end tell"#,
            name
        );

        let result = run_applescript(&script)?;
        let parts: Vec<&str> = result.split(',').collect();

        if parts.len() != 4 {
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
        let transition_duration = parts[3]
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|&d| d > 0.0);

        Ok(SlideInfo { current, total, transition_duration })
    }

    fn prev_slide(&self, name: &str) -> Result<SlideInfo, String> {
        let script = format!(
            r#"tell application "Keynote"
                tell document "{}"
                    set oldPos to slide number of current slide
                    set totalSlides to count (every slide whose skipped is false)
                    if oldPos <= 1 then
                        return "BOUNDARY," & (oldPos as text) & "," & (totalSlides as text)
                    end if
                    show previous
                    -- Read immediately — reverse transitions are fast
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

        Ok(SlideInfo { current, total, transition_duration: None })
    }

    fn goto_slide(&self, name: &str, slide: i32) -> Result<SlideInfo, String> {
        let script = format!(
            r#"tell application "Keynote"
                tell document "{}"
                    set totalSlides to count (every slide whose skipped is false)
                    if {slide} < 1 or {slide} > totalSlides then
                        return "BOUNDARY," & (slide number of current slide as text) & "," & (totalSlides as text)
                    end if
                    set matchingSlides to (every slide whose slide number is {slide})
                    if (count matchingSlides) = 0 then
                        return "BOUNDARY," & (slide number of current slide as text) & "," & (totalSlides as text)
                    end if
                    set current slide to item 1 of matchingSlides
                    -- Read immediately — goto jumps have no transition
                    set newPos to slide number of current slide
                    return "OK," & (newPos as text) & "," & (totalSlides as text)
                end tell
            end tell"#,
            name,
            slide = slide
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

        Ok(SlideInfo { current, total, transition_duration: None })
    }

    // Keynote doesn't support notes zoom control

    fn get_presenter_notes(&self, name: &str) -> Result<Option<String>, String> {
        let script = format!(
            r#"tell application "Keynote"
                tell document "{}"
                    set noteText to presenter notes of current slide
                    return noteText
                end tell
            end tell"#,
            name
        );

        match run_applescript(&script) {
            Ok(result) if result.trim().is_empty() || result.trim() == "missing value" => {
                log::debug!("Keynote get_presenter_notes: empty/missing result for current slide");
                Ok(None)
            },
            Ok(result) => {
                log::debug!("Keynote get_presenter_notes: got {} chars, first 80: {:?}", result.len(), &result[..result.len().min(80)]);
                Ok(Some(result))
            },
            Err(e) => {
                log::debug!("Keynote get_presenter_notes: error: {}", e);
                Ok(None)
            },
        }
    }

    fn get_all_presenter_notes(&self, name: &str) -> Result<HashMap<i32, String>, String> {
        let script = format!(
            r#"tell application "Keynote"
                tell document "{}"
                    set output to ""
                    repeat with i from 1 to (count slides)
                        set s to slide i
                        if skipped of s is false then
                            set sNum to slide number of s
                            set noteText to presenter notes of s
                            set output to output & (sNum as text) & "|||" & noteText & linefeed
                        end if
                    end repeat
                    return output
                end tell
            end tell"#,
            name
        );

        let result = run_applescript(&script)?;
        log::debug!("Keynote get_all_presenter_notes raw output ({} chars):\n{}", result.len(), &result[..result.len().min(2000)]);
        let parsed = parse_notes_response(&result);
        log::debug!("Keynote get_all_presenter_notes parsed: {} slides with notes, keys: {:?}", parsed.len(), parsed.keys().collect::<Vec<_>>());
        Ok(parsed)
    }

    /// Batched live status — single AppleScript call instead of 3 separate ones.
    /// Keynote doesn't support notes zoom, so that field is always None.
    fn get_live_status(&self, name: &str) -> LiveStatus {
        let script = format!(
            r#"tell application "Keynote"
                set isOpen to "false"
                set isPresenting to "false"
                set currentSlide to "0"
                set totalSlides to "0"
                set noteText to ""
                try
                    set doc to document "{}"
                    set isOpen to "true"
                    set isPresenting to (playing) as text
                    if playing then
                        set currentSlide to (slide number of current slide of doc) as text
                        set totalSlides to (count (every slide of doc whose skipped is false)) as text
                        try
                            set noteText to presenter notes of current slide of doc
                            if noteText is missing value then set noteText to ""
                        end try
                    end if
                end try
                return isOpen & "|||" & isPresenting & "|||" & currentSlide & "|||" & totalSlides & "|||" & noteText
            end tell"#,
            name
        );

        match run_applescript(&script) {
            Ok(result) => {
                let parts: Vec<&str> = result.splitn(5, "|||").collect();
                if parts.len() < 4 {
                    return LiveStatus::default();
                }

                let is_open = parts[0].trim() == "true";
                let is_presenting = parts[1].trim() == "true";
                let current_slide = parts[2].trim().parse().unwrap_or(0);
                let total_slides = parts[3].trim().parse().unwrap_or(0);
                let notes = if parts.len() >= 5 && !parts[4].trim().is_empty() && parts[4].trim() != "missing value" {
                    Some(parts[4].to_string())
                } else {
                    None
                };

                LiveStatus {
                    is_open,
                    is_presenting,
                    current_slide,
                    total_slides,
                    zoom_level: None,
                    presenter_notes: notes,
                    current_build: None,
                    total_builds: None,
                }
            }
            Err(e) => {
                log::warn!("Keynote get_live_status error: {}", e);
                LiveStatus::default()
            }
        }
    }
}
