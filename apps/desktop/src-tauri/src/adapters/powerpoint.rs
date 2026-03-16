use std::collections::HashMap;
use super::{LiveStatus, PresentationAdapter, PresentationState, SlideInfo, parse_notes_response};
use crate::applescript::run_applescript;

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
                set noteText to ""
                set notesSlide to notes page of slide idx of pres
                repeat with s in shapes of notesSlide
                    try
                        if has text frame of s then
                            set t to content of text range of text frame of s
                            if t is not "" and t is not missing value then
                                set noteText to t
                            end if
                        end if
                    end try
                end repeat
                return noteText
            end tell"#,
            name
        );

        match run_applescript(&script) {
            Ok(result) if result.trim().is_empty() || result.trim() == "missing value" => {
                log::debug!("PowerPoint get_presenter_notes: empty/missing result for current slide");
                Ok(None)
            },
            Ok(result) => {
                log::debug!("PowerPoint get_presenter_notes: got {} chars, first 80: {:?}", result.len(), &result[..result.len().min(80)]);
                Ok(Some(result))
            },
            Err(e) => {
                log::debug!("PowerPoint get_presenter_notes: error: {}", e);
                Ok(None)
            },
        }
    }

    fn get_all_presenter_notes(&self, name: &str) -> Result<HashMap<i32, String>, String> {
        // Detect the correct notes body shape index from slide 1, then use it for all slides.
        // This avoids iterating shapes per-slide which causes severe AppleScript IPC overhead.
        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set pres to presentation "{}"
                set totalSlides to count slides of pres

                -- Discover notes body shape index from slide 1
                -- The notes page has a slide number placeholder (small text) and the
                -- notes body (large text). We pick the last shape with a text frame,
                -- which is the notes body rather than the slide number.
                set bodyIdx to -1
                try
                    set notesSlide to notes page of slide 1 of pres
                    set shapeCount to count shapes of notesSlide
                    repeat with j from 1 to shapeCount
                        try
                            if has text frame of shape j of notesSlide then
                                set bodyIdx to j
                            end if
                        end try
                    end repeat
                end try

                if bodyIdx = -1 then set bodyIdx to 2

                set output to ""
                repeat with i from 1 to totalSlides
                    set noteText to ""
                    try
                        set noteText to content of text range of text frame of shape bodyIdx of notes page of slide i of pres
                        if noteText is missing value then set noteText to ""
                    on error
                        set noteText to ""
                    end try
                    set output to output & (i as text) & "|||" & noteText & linefeed
                end repeat
                return output
            end tell"#,
            name
        );

        let result = run_applescript(&script)?;
        log::debug!("PowerPoint get_all_presenter_notes raw output ({} chars):\n{}", result.len(), &result[..result.len().min(2000)]);
        let parsed = parse_notes_response(&result);
        log::debug!("PowerPoint get_all_presenter_notes parsed: {} slides with notes, keys: {:?}", parsed.len(), parsed.keys().collect::<Vec<_>>());
        Ok(parsed)
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

    /// Batched live status — single AppleScript call instead of 3-4 separate ones.
    fn get_live_status(&self, name: &str) -> LiveStatus {
        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set isOpen to "false"
                set isPresenting to "false"
                set currentSlide to "0"
                set totalSlides to "0"
                set noteText to ""
                set zoomVal to ""
                try
                    set pres to presentation "{}"
                    set isOpen to "true"
                    try
                        set ssw to slide show window of pres
                        set ssView to slide show view of ssw
                        set isPresenting to "true"
                        set currentSlide to (current show position of ssView) as text
                        set totalSlides to (count slides of pres) as text
                        try
                            set idx to current show position of ssView
                            set notesSlide to notes page of slide idx of pres
                            repeat with s in shapes of notesSlide
                                try
                                    if has text frame of s then
                                        set t to content of text range of text frame of s
                                        if t is not "" and t is not missing value then
                                            set noteText to t
                                        end if
                                    end if
                                end try
                            end repeat
                        end try
                        try
                            set pvWindow to presenter view window 1
                            set pTool to presenter tool of pvWindow
                            set zoomVal to (notes zoom of pTool) as text
                        end try
                    end try
                end try
                return isOpen & "|||" & isPresenting & "|||" & currentSlide & "|||" & totalSlides & "|||" & zoomVal & "|||" & noteText
            end tell"#,
            name
        );

        match run_applescript(&script) {
            Ok(result) => {
                let parts: Vec<&str> = result.splitn(6, "|||").collect();
                if parts.len() < 5 {
                    return LiveStatus::default();
                }

                let is_open = parts[0].trim() == "true";
                let is_presenting = parts[1].trim() == "true";
                let current_slide = parts[2].trim().parse().unwrap_or(0);
                let total_slides = parts[3].trim().parse().unwrap_or(0);
                let zoom_level = parts[4].trim().parse().ok();
                let notes = if parts.len() >= 6 && !parts[5].trim().is_empty() && parts[5].trim() != "missing value" {
                    Some(parts[5].to_string())
                } else {
                    None
                };

                LiveStatus {
                    is_open,
                    is_presenting,
                    current_slide,
                    total_slides,
                    zoom_level,
                    presenter_notes: notes,
                }
            }
            Err(_) => LiveStatus::default(),
        }
    }
}

impl PowerPointAdapter {
    pub fn get_next_zoom_level(current: i32) -> i32 {
        super::get_next_zoom_level(current)
    }

    pub fn get_prev_zoom_level(current: i32) -> i32 {
        super::get_prev_zoom_level(current)
    }
}
