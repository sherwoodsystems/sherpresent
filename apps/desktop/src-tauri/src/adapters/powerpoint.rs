use std::collections::HashMap;
use super::{PresentationAdapter, PresentationState, SlideInfo};
use crate::applescript::run_applescript;

/// PowerPoint's fixed zoom levels for presenter view notes
const ZOOM_LEVELS: [i32; 5] = [100, 150, 200, 300, 400];

pub struct PowerPointAdapter;

impl PresentationAdapter for PowerPointAdapter {
    fn get_open_presentations(&self) -> Result<Vec<String>, String> {
        let script = r#"tell application "Microsoft PowerPoint" to get name of every presentation"#;

        match run_applescript(script) {
            Ok(result) if result.is_empty() => Ok(vec![]),
            Ok(result) => {
                // AppleScript returns comma-separated list
                Ok(result.split(", ").map(|s| s.trim().to_string()).collect())
            }
            Err(_) => Ok(vec![]), // PowerPoint not running or no presentations
        }
    }

    fn get_presentation_state(&self, name: &str) -> Result<PresentationState, String> {
        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set isOpen to false
                set isPresenting to false

                try
                    set pres to presentation "{}"
                    set isOpen to true

                    try
                        set ssw to slide show window of pres
                        if ssw is not missing value then
                            set pos to current show position of slide show view of ssw
                            set isPresenting to true
                        end if
                    end try
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
            r#"tell application "Microsoft PowerPoint"
                set currentPos to current show position of slide show view of slide show window of presentation "{}"
                set totalSlides to count slides of presentation "{}"
                return (currentPos as text) & "," & (totalSlides as text)
            end tell"#,
            name, name
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
            r#"tell application "Microsoft PowerPoint"
                set ssView to slide show view of slide show window of presentation "{}"
                set pres to presentation "{}"
                set oldPos to current show position of ssView
                set totalSlides to count slides of pres
                if oldPos >= totalSlides then
                    return "BOUNDARY," & (oldPos as text) & "," & (totalSlides as text)
                end if
                go to next slide ssView
                set newPos to current show position of ssView
                return "OK," & (newPos as text) & "," & (totalSlides as text)
            end tell"#,
            name, name
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
            r#"tell application "Microsoft PowerPoint"
                set ssView to slide show view of slide show window of presentation "{}"
                set pres to presentation "{}"
                set oldPos to current show position of ssView
                set totalSlides to count slides of pres
                if oldPos <= 1 then
                    return "BOUNDARY," & (oldPos as text) & "," & (totalSlides as text)
                end if
                go to previous slide ssView
                set newPos to current show position of ssView
                return "OK," & (newPos as text) & "," & (totalSlides as text)
            end tell"#,
            name, name
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

    fn get_presenter_notes(&self, name: &str) -> Result<Option<String>, String> {
        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set pres to presentation "{}"
                set idx to slide index of slide of view of slide show window of pres
                try
                    set noteText to content of text range of text frame of shape 2 of notes page of slide idx of pres
                    return noteText
                on error
                    return ""
                end try
            end tell"#,
            name
        );

        match run_applescript(&script) {
            Ok(result) if result.trim().is_empty() => Ok(None),
            Ok(result) => Ok(Some(result)),
            Err(_) => Ok(None),
        }
    }

    fn get_all_presenter_notes(&self, name: &str) -> Result<HashMap<i32, String>, String> {
        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set pres to presentation "{}"
                set totalSlides to count slides of pres
                set output to ""
                repeat with i from 1 to totalSlides
                    try
                        set noteText to content of text range of text frame of shape 2 of notes page of slide i of pres
                        set output to output & (i as text) & "|||" & noteText & linefeed
                    on error
                        set output to output & (i as text) & "|||" & linefeed
                    end try
                end repeat
                return output
            end tell"#,
            name
        );

        let result = run_applescript(&script)?;
        let mut notes = HashMap::new();

        for line in result.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((num_str, text)) = line.split_once("|||") {
                if let Ok(slide_num) = num_str.trim().parse::<i32>() {
                    let text = text.trim();
                    if !text.is_empty() {
                        notes.insert(slide_num, text.to_string());
                    }
                }
            }
        }

        Ok(notes)
    }

    fn get_notes_zoom(&self) -> Result<Option<i32>, String> {
        let script = r#"tell application "Microsoft PowerPoint"
            try
                set pvWindow to presenter view window 1
                set pTool to presenter tool of pvWindow
                set nZoom to notes zoom of pTool
                return (nZoom as text)
            on error errMsg
                return "ERROR:" & errMsg
            end try
        end tell"#;

        let result = run_applescript(script)?;

        if result.starts_with("ERROR:") {
            return Ok(None);
        }

        match result.trim().parse() {
            Ok(zoom) => Ok(Some(zoom)),
            Err(_) => Ok(None),
        }
    }

    fn set_notes_zoom(&self, level: i32) -> Result<(), String> {
        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                try
                    set pvWindow to presenter view window 1
                    set pTool to presenter tool of pvWindow
                    set notes zoom of pTool to {}
                    return "OK"
                on error errMsg
                    return "ERROR:" & errMsg
                end try
            end tell"#,
            level
        );

        let result = run_applescript(&script)?;

        if result.starts_with("ERROR:") {
            Err(result[6..].to_string())
        } else {
            Ok(())
        }
    }
}

impl PowerPointAdapter {
    /// Get the next zoom level up from current
    pub fn get_next_zoom_level(current: i32) -> i32 {
        for &level in &ZOOM_LEVELS {
            if level > current {
                return level;
            }
        }
        ZOOM_LEVELS[ZOOM_LEVELS.len() - 1]
    }

    /// Get the next zoom level down from current
    pub fn get_prev_zoom_level(current: i32) -> i32 {
        for &level in ZOOM_LEVELS.iter().rev() {
            if level < current {
                return level;
            }
        }
        ZOOM_LEVELS[0]
    }
}
