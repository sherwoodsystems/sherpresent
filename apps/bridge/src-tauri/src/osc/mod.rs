//! # OSC
//!
//! Outgoing (`osc::sender`) and incoming (`osc::feedback`) OSC handling for
//! the SherPresent bridge.
//!
//! The bridge speaks the oscpoint OSC schema (`/oscpoint/...`) so it can
//! interoperate with both [OSCPoint](https://github.com/phuvf/oscpoint) on
//! Windows and SherPresent desktop on macOS/Linux.
//!
//! Schema (matches oscpoint v2):
//!
//! | Bridge → Desktop  | Args   | Description           |
//! |-------------------|--------|-----------------------|
//! | `/oscpoint/next`            | (none)        | Next slide             |
//! | `/oscpoint/previous`        | (none)        | Previous slide         |
//! | `/oscpoint/goto/slide`      | int (n)       | Jump to slide n         |
//!
//! | Desktop → Bridge (feedback) | Args          | Description                |
//! |------------------------------|---------------|----------------------------|
//! | `/oscpoint/slideshow/currentslide` | int (n) | Currently visible slide   |
//! | `/oscpoint/slideshow/slidecount`   | int (n) | Total slides              |
//! | `/oscpoint/presentation/name`      | string  | Presentation filename     |
//! | `/oscpoint/slideshow/notes`        | string  | Notes for current slide  |
//! | `/oscpoint/v2/event`               | string  | Lifecycle event name     |

pub mod feedback;
pub mod sender;
pub mod messages;