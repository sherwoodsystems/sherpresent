use super::reply::{
    notes_text, parse_live_status, parse_names, parse_presentation_state, parse_slide_info,
};
use super::{parse_notes_response, LiveStatus, PresentationAdapter, PresentationState, SlideInfo};
use crate::applescript::{is_app_running, require_running, run_applescript};
use std::collections::HashMap;

const APP_NAME: &str = "Keynote";

/// Keynote adapter for macOS - constructed conditionally in get_adapter()
pub struct KeynoteAdapter;

impl PresentationAdapter for KeynoteAdapter {
    fn get_open_presentations(&self) -> Result<Vec<String>, String> {
        if !is_app_running(APP_NAME) {
            return Ok(vec![]);
        }

        let script = r#"tell application "Keynote" to get name of every document"#;
        Ok(run_applescript(script)
            .map(|r| parse_names(&r))
            .unwrap_or_default())
    }

    fn get_presentation_state(&self, name: &str) -> Result<PresentationState, String> {
        if !is_app_running(APP_NAME) {
            return Ok(PresentationState {
                is_open: false,
                is_presenting: false,
            });
        }

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

        parse_presentation_state(&run_applescript(&script)?)
    }

    fn get_slide_info(&self, name: &str) -> Result<SlideInfo, String> {
        require_running(APP_NAME)?;

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

        parse_slide_info(&run_applescript(&script)?)
    }

    fn next_slide(&self, name: &str) -> Result<SlideInfo, String> {
        require_running(APP_NAME)?;

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
                    -- Read immediately — may still be the old slide during a transition; StateManager keeps its optimistic one
                    set newPos to slide number of current slide
                    return "OK," & (newPos as text) & "," & (totalSlides as text) & "," & (tDur as text)
                end tell
            end tell"#,
            name
        );

        parse_slide_info(&run_applescript(&script)?)
    }

    fn prev_slide(&self, name: &str) -> Result<SlideInfo, String> {
        require_running(APP_NAME)?;

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

        parse_slide_info(&run_applescript(&script)?)
    }

    fn goto_slide(&self, name: &str, slide: i32) -> Result<SlideInfo, String> {
        require_running(APP_NAME)?;

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

        parse_slide_info(&run_applescript(&script)?)
    }

    // Keynote doesn't support notes zoom control

    fn get_presenter_notes(&self, name: &str) -> Result<Option<String>, String> {
        if !is_app_running(APP_NAME) {
            return Ok(None);
        }

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
            Ok(result) => Ok(notes_text(&result)),
            Err(e) => {
                log::debug!("Keynote get_presenter_notes: error: {}", e);
                Ok(None)
            }
        }
    }

    fn get_all_presenter_notes(&self, name: &str) -> Result<HashMap<i32, String>, String> {
        if !is_app_running(APP_NAME) {
            return Ok(HashMap::new());
        }

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
        log::debug!(
            "Keynote get_all_presenter_notes raw output ({} chars):\n{}",
            result.len(),
            &result[..result.len().min(2000)]
        );
        let parsed = parse_notes_response(&result);
        log::debug!(
            "Keynote get_all_presenter_notes parsed: {} slides with notes, keys: {:?}",
            parsed.len(),
            parsed.keys().collect::<Vec<_>>()
        );
        Ok(parsed)
    }

    /// Batched live status — single AppleScript call instead of 3 separate ones.
    /// Keynote doesn't support notes zoom, so that field is always None.
    fn get_live_status(&self, name: &str) -> LiveStatus {
        if !is_app_running(APP_NAME) {
            return LiveStatus::default();
        }

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
            Ok(reply) => parse_live_status(&reply, 0)
                .map(|(status, _)| status)
                .unwrap_or_default(),
            Err(e) => {
                log::warn!("Keynote get_live_status error: {}", e);
                LiveStatus::default()
            }
        }
    }
}
