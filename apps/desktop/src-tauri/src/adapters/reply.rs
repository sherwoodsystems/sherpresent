//! Parsing for the plain-text replies of the AppleScript adapters
//! (PowerPoint for Mac, Keynote).

use super::{LiveStatus, PresentationState, SlideInfo};

/// A slide position reply: `current,total[,transition]`, optionally led by a
/// status word (`OK,` or `BOUNDARY,`).
pub fn parse_slide_info(reply: &str) -> Result<SlideInfo, String> {
    let mut fields: Vec<&str> = reply.split(',').map(str::trim).collect();
    if fields.first().is_some_and(|f| f.parse::<i32>().is_err()) {
        fields.remove(0);
    }
    let [current, total, rest @ ..] = fields.as_slice() else {
        return Err(format!("Unexpected response format: {:?}", reply));
    };
    if rest.len() > 1 {
        return Err(format!("Unexpected response format: {:?}", reply));
    }
    Ok(SlideInfo {
        current: current
            .parse()
            .map_err(|_| "Failed to parse current slide")?,
        total: total.parse().map_err(|_| "Failed to parse total slides")?,
        transition_duration: rest
            .first()
            .and_then(|d| d.parse::<f64>().ok())
            .filter(|&d| d > 0.0),
    })
}

/// An `isOpen,isPresenting` reply, e.g. `true,false`.
pub fn parse_presentation_state(reply: &str) -> Result<PresentationState, String> {
    match reply
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [open, presenting] => Ok(PresentationState {
            is_open: *open == "true",
            is_presenting: *presenting == "true",
        }),
        _ => Err(format!("Unexpected response format: {:?}", reply)),
    }
}

/// A batched live status: `open|||presenting|||current|||total`, then `extra`
/// adapter-specific fields, then the notes. Notes come last so a `|||` inside
/// them can't shift the other fields. Returns the status and the extra fields.
pub fn parse_live_status(reply: &str, extra: usize) -> Option<(LiveStatus, Vec<&str>)> {
    let parts: Vec<&str> = reply.splitn(5 + extra, "|||").collect();
    if parts.len() < 4 + extra {
        return None;
    }
    let status = LiveStatus {
        is_open: parts[0].trim() == "true",
        is_presenting: parts[1].trim() == "true",
        current_slide: parts[2].trim().parse().unwrap_or(0),
        total_slides: parts[3].trim().parse().unwrap_or(0),
        presenter_notes: parts.get(4 + extra).and_then(|n| notes_text(n)),
        ..Default::default()
    };
    Some((status, parts[4..4 + extra].to_vec()))
}

/// Notes text, or None when AppleScript returned nothing useful.
pub fn notes_text(reply: &str) -> Option<String> {
    let trimmed = reply.trim();
    (!trimmed.is_empty() && trimmed != "missing value").then(|| reply.to_string())
}

/// An AppleScript list of names (`a, b, c`).
pub fn parse_names(reply: &str) -> Vec<String> {
    if reply.is_empty() {
        return vec![];
    }
    reply.split(", ").map(|s| s.trim().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_slide_info_formats() {
        let info = parse_slide_info("3,10").unwrap();
        assert_eq!((info.current, info.total), (3, 10));
        assert_eq!(info.transition_duration, None);

        let info = parse_slide_info("OK,4,10").unwrap();
        assert_eq!((info.current, info.total), (4, 10));

        let info = parse_slide_info("BOUNDARY,10,10,1.5").unwrap();
        assert_eq!((info.current, info.total), (10, 10));
        assert_eq!(info.transition_duration, Some(1.5));

        let info = parse_slide_info("OK,2,10,0.0").unwrap();
        assert_eq!(info.transition_duration, None);
    }

    #[test]
    fn test_parse_slide_info_rejects_garbage() {
        assert!(parse_slide_info("").is_err());
        assert!(parse_slide_info("OK,3").is_err());
        assert!(parse_slide_info("OK,x,10").is_err());
        assert!(parse_slide_info("1,2,3,4").is_err());
    }

    #[test]
    fn test_parse_presentation_state() {
        let s = parse_presentation_state("true, false").unwrap();
        assert!(s.is_open && !s.is_presenting);
        assert!(parse_presentation_state("true").is_err());
    }

    #[test]
    fn test_parse_live_status_with_extra_fields() {
        let (s, extra) = parse_live_status("true|||true|||2|||9|||150|||a ||| b", 1).unwrap();
        assert!(s.is_open && s.is_presenting);
        assert_eq!((s.current_slide, s.total_slides), (2, 9));
        assert_eq!(extra, vec!["150"]);
        assert_eq!(s.presenter_notes.as_deref(), Some("a ||| b"));

        let (s, _) = parse_live_status("true|||false|||0|||0|||missing value", 0).unwrap();
        assert_eq!(s.presenter_notes, None);
        assert!(parse_live_status("true|||false", 0).is_none());
    }

    #[test]
    fn test_parse_names() {
        assert_eq!(parse_names("a.pptx, b.pptx"), vec!["a.pptx", "b.pptx"]);
        assert!(parse_names("").is_empty());
    }
}
