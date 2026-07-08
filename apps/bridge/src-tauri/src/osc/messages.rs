//! # Oscpoint OSC Message Definitions
//!
//! Central place for the `/oscpoint/...` address constants the bridge speaks.
//! Keeping these as constants avoids drift between the sender and the feedback
//! listener, and makes it easy to align with OSCPoint's published schema.

/// Address constants for commands the bridge SENDS to desktops / OSCPoint.
pub mod out {
    pub const NEXT: &str = "/oscpoint/next";
    pub const PREVIOUS: &str = "/oscpoint/previous";
    pub const GOTO_SLIDE: &str = "/oscpoint/goto/slide";
}

/// Address prefixes for feedback the bridge RECEIVES from desktops / OSCPoint.
pub mod feedback {
    pub const CURRENT_SLIDE: &str = "/oscpoint/slideshow/currentslide";
    pub const SLIDE_COUNT: &str = "/oscpoint/slideshow/slidecount";
    pub const PRESENTATION_NAME: &str = "/oscpoint/presentation/name";
    pub const SLIDESHOW_NOTES: &str = "/oscpoint/slideshow/notes";

    /// Lifecycle event address (oscpoint v2). The first argument is the
    /// event name string, e.g. `presentation_open`, `slideshow_begin`.
    pub const V2_EVENT: &str = "/oscpoint/v2/event";

    /// State broadcast prefix (oscpoint v2).
    pub const V2_STATE_PREFIX: &str = "/oscpoint/state/";

    /// Returns `true` if `address` belongs to any of the oscpoint feedback
    /// paths we know how to consume.
    pub fn is_known(address: &str) -> bool {
        address == CURRENT_SLIDE
            || address == SLIDE_COUNT
            || address == PRESENTATION_NAME
            || address == SLIDESHOW_NOTES
            || address == V2_EVENT
            || address.starts_with(V2_STATE_PREFIX)
    }
}