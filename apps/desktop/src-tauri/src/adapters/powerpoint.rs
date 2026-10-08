use super::reply::{
    notes_text, parse_live_status, parse_names, parse_presentation_state, parse_slide_info,
};
use super::{parse_notes_response, LiveStatus, PresentationAdapter, PresentationState, SlideInfo};
use crate::applescript::{is_app_running, require_running, run_applescript};
use std::collections::HashMap;

const APP_NAME: &str = "Microsoft PowerPoint";

/// AppleScript that sets `noteText` to the notes of slide `idx` of `pres`.
///
/// Index, not `repeat with s in shapes of ...`: since PowerPoint 16.113
/// iterating notes-page shape references hangs PowerPoint indefinitely (and
/// our AppleScript lock with it). Match the body placeholder: "last shape
/// with text" picks the slide number.
const CURRENT_NOTES: &str = r#"
                set notesSlide to notes page of slide idx of pres
                repeat with j from 1 to count shapes of notesSlide
                    try
                        if placeholder type of placeholder format of shape j of notesSlide is placeholder type body placeholder then
                            set t to content of text range of text frame of shape j of notesSlide
                            if t is not missing value then set noteText to t
                            exit repeat
                        end if
                    end try
                end repeat"#;

pub struct PowerPointAdapter;

impl PowerPointAdapter {
    /// Run a slide show command (`next slide`, `previous slide`) and report
    /// where the show ended up.
    fn step(&self, name: &str, command: &str) -> Result<SlideInfo, String> {
        require_running(APP_NAME)?;
        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set ssView to slide show view of slide show window of presentation "{name}"
                set totalSlides to count slides of presentation "{name}"
                go to {command} ssView
                set newPos to current show position of ssView
                return "OK," & (newPos as text) & "," & (totalSlides as text)
            end tell"#
        );
        parse_slide_info(&run_applescript(&script)?)
    }
}

impl PresentationAdapter for PowerPointAdapter {
    fn get_open_presentations(&self) -> Result<Vec<String>, String> {
        if !is_app_running(APP_NAME) {
            return Ok(vec![]);
        }
        let script = r#"tell application "Microsoft PowerPoint" to get name of every presentation"#;
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

        parse_presentation_state(&run_applescript(&script)?)
    }

    fn get_slide_info(&self, name: &str) -> Result<SlideInfo, String> {
        require_running(APP_NAME)?;
        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set currentPos to current show position of slide show view of slide show window of presentation "{name}"
                set totalSlides to count slides of presentation "{name}"
                return (currentPos as text) & "," & (totalSlides as text)
            end tell"#
        );
        parse_slide_info(&run_applescript(&script)?)
    }

    fn next_slide(&self, name: &str) -> Result<SlideInfo, String> {
        self.step(name, "next slide")
    }

    fn prev_slide(&self, name: &str) -> Result<SlideInfo, String> {
        self.step(name, "previous slide")
    }

    fn get_presenter_notes(&self, name: &str) -> Result<Option<String>, String> {
        if !is_app_running(APP_NAME) {
            return Ok(None);
        }

        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set pres to presentation "{name}"
                set idx to current show position of slide show view of slide show window of pres
                set noteText to ""
                {CURRENT_NOTES}
                return noteText
            end tell"#
        );

        Ok(run_applescript(&script).ok().and_then(|r| notes_text(&r)))
    }

    fn get_all_presenter_notes(&self, name: &str) -> Result<HashMap<i32, String>, String> {
        if !is_app_running(APP_NAME) {
            return Ok(HashMap::new());
        }

        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set pres to presentation "{}"
                set totalSlides to count slides of pres
                set bodyIdx to -1
                try
                    set notesSlide to notes page of slide 1 of pres
                    set shapeCount to count shapes of notesSlide
                    repeat with j from 1 to shapeCount
                        try
                            if placeholder type of placeholder format of shape j of notesSlide is placeholder type body placeholder then
                                set bodyIdx to j
                                exit repeat
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
        Ok(parse_notes_response(&result))
    }

    fn get_notes_zoom(&self) -> Result<Option<i32>, String> {
        if !is_app_running(APP_NAME) {
            return Ok(None);
        }

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

        // An "ERROR:" reply (no presenter view) fails to parse, which is None too.
        Ok(run_applescript(script)?.trim().parse().ok())
    }

    fn set_notes_zoom(&self, level: i32) -> Result<(), String> {
        require_running(APP_NAME)?;

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
        match result.strip_prefix("ERROR:") {
            Some(err) => Err(err.to_string()),
            None => Ok(()),
        }
    }

    /// Batched live status — one AppleScript call. No build info: PowerPoint
    /// for Mac's dictionary has no click index/count (that's Windows COM
    /// only), and naming them is a *compile* error that no try block catches.
    fn get_live_status(&self, name: &str) -> LiveStatus {
        if !is_app_running(APP_NAME) {
            return LiveStatus::default();
        }

        let script = format!(
            r#"tell application "Microsoft PowerPoint"
                set isOpen to "false"
                set isPresenting to "false"
                set currentSlide to "0"
                set totalSlides to "0"
                set noteText to ""
                set zoomLevel to "0"
                try
                    set pres to presentation "{name}"
                    set isOpen to "true"
                    try
                        set ssw to slide show window of pres
                        set ssView to slide show view of ssw
                        set isPresenting to "true"
                        set currentSlide to (current show position of ssView) as text
                        set totalSlides to (count slides of pres) as text
                        try
                            set idx to current show position of ssView
                            {CURRENT_NOTES}
                        end try
                        try
                            set pvWindow to presenter view window 1
                            set zoomLevel to (notes zoom of presenter tool of pvWindow) as text
                        end try
                    end try
                end try
                return isOpen & "|||" & isPresenting & "|||" & currentSlide & "|||" & totalSlides & "|||" & zoomLevel & "|||" & noteText
            end tell"#
        );

        let reply = match run_applescript(&script) {
            Ok(reply) => reply,
            Err(e) => {
                log::warn!("PowerPoint get_live_status error: {}", e);
                return LiveStatus::default();
            }
        };
        let Some((status, extra)) = parse_live_status(&reply, 1) else {
            return LiveStatus::default();
        };
        LiveStatus {
            zoom_level: extra[0].trim().parse().ok().filter(|&z: &i32| z > 0),
            ..status
        }
    }
}
