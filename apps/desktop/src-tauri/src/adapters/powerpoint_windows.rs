//! Windows PowerPoint adapter using COM automation
//!
//! This adapter controls Microsoft PowerPoint on Windows via the COM (Component Object Model)
//! interface. It uses late binding via IDispatch, which is the same mechanism used by VBA.
//!
//! ## How COM Automation Works
//!
//! PowerPoint exposes its functionality through COM interfaces. The key concepts:
//!
//! - **ProgID**: `"PowerPoint.Application"` - Human-readable identifier for PowerPoint
//! - **CLSID**: The actual COM class identifier (derived from ProgID)
//! - **IDispatch**: The interface for late-bound method calls (like VBA uses)
//! - **DISPID**: Numeric identifier for each property/method
//!
//! ## Object Model Hierarchy
//!
//! ```text
//! Application
//!   └── Presentations (collection)
//!         └── Presentation
//!               ├── Name (property)
//!               ├── Slides (collection)
//!               │     └── Count (property)
//!               └── SlideShowWindow
//!                     └── View (SlideShowView)
//!                           ├── CurrentShowPosition (property)
//!                           ├── Next() (method)
//!                           ├── Previous() (method)
//!                           └── GotoSlide(index) (method)
//! ```
//!
//! ## References
//!
//! - [PowerPoint VBA Reference](https://learn.microsoft.com/en-us/office/vba/api/overview/powerpoint)
//! - [windows-rs crate](https://github.com/microsoft/windows-rs)

#![cfg(target_os = "windows")]

use super::{PresentationAdapter, PresentationState, SlideInfo};

use windows::{
    core::{Interface, BSTR, GUID, PCWSTR},
    Win32::System::Com::{
        CLSIDFromProgID, CoCreateInstance, CoInitializeEx,
        CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
        IDispatch, DISPATCH_METHOD, DISPATCH_PROPERTYGET,
        DISPPARAMS,
    },
    Win32::System::Ole::GetActiveObject,
    Win32::System::Variant::VARIANT,
};

use std::ptr;

/// PowerPoint's fixed zoom levels for presenter view notes (same as macOS)
const ZOOM_LEVELS: [i32; 5] = [100, 150, 200, 300, 400];

/// Windows PowerPoint adapter using COM automation
pub struct PowerPointWindowsAdapter;

impl PowerPointWindowsAdapter {
    /// Initialize COM library for this thread
    ///
    /// Must be called before any COM operations. Uses apartment threading model
    /// which is required for Office automation.
    fn init_com() -> Result<(), String> {
        unsafe {
            // CoInitializeEx returns S_OK on success, S_FALSE if already initialized
            // Both are acceptable - we just ignore the result for S_FALSE
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() {
                // Only fail on actual errors, not S_FALSE (already initialized)
                let code = hr.0;
                if code < 0 {
                    return Err(format!("Failed to initialize COM: HRESULT 0x{:08X}", code as u32));
                }
            }
            Ok(())
        }
    }

    /// Get IDispatch interface to running PowerPoint Application
    ///
    /// First tries to attach to an existing PowerPoint instance using GetActiveObject.
    /// If no instance is running, creates a new one with CoCreateInstance.
    fn get_application() -> Result<IDispatch, String> {
        unsafe {
            // Get CLSID from ProgID
            let prog_id: Vec<u16> = "PowerPoint.Application\0".encode_utf16().collect();

            let clsid = CLSIDFromProgID(PCWSTR(prog_id.as_ptr()))
                .map_err(|e| format!("PowerPoint not installed or ProgID not found: {}", e))?;

            // Try to get running instance first
            let mut punk = None;
            if GetActiveObject(&clsid, None, &mut punk).is_ok() {
                if let Some(unk) = punk {
                    return unk.cast::<IDispatch>()
                        .map_err(|e| format!("Failed to get IDispatch from running instance: {}", e));
                }
            }

            // No running instance, try to create one
            // Note: This will start PowerPoint if not running
            let app: IDispatch = CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER)
                .map_err(|e| format!("Failed to create PowerPoint instance: {}", e))?;

            Ok(app)
        }
    }

    /// Get DISPID (dispatch identifier) for a property or method name
    ///
    /// IDispatch uses numeric IDs rather than string names for efficiency.
    /// This converts a name like "Presentations" to its DISPID.
    fn get_dispid(disp: &IDispatch, name: &str) -> Result<i32, String> {
        unsafe {
            let wide_name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let mut dispid = 0i32;
            let names = [PCWSTR(wide_name.as_ptr())];

            disp.GetIDsOfNames(&GUID::zeroed(), names.as_ptr(), 1, 0, &mut dispid)
                .map_err(|e| format!("Property/method '{}' not found: {}", name, e))?;

            Ok(dispid)
        }
    }

    /// Get a property from an IDispatch object
    ///
    /// Equivalent to VBA: `obj.PropertyName`
    fn get_property(disp: &IDispatch, name: &str) -> Result<VARIANT, String> {
        unsafe {
            let dispid = Self::get_dispid(disp, name)?;
            let mut result = VARIANT::default();
            let params = DISPPARAMS {
                rgvarg: ptr::null_mut(),
                rgdispidNamedArgs: ptr::null_mut(),
                cArgs: 0,
                cNamedArgs: 0,
            };

            disp.Invoke(
                dispid,
                &GUID::zeroed(),
                0, // LOCALE_SYSTEM_DEFAULT
                DISPATCH_PROPERTYGET,
                &params,
                Some(&mut result),
                None,
                None,
            )
            .map_err(|e| format!("Failed to get property '{}': {}", name, e))?;

            Ok(result)
        }
    }

    /// Get an indexed property from a collection
    ///
    /// Equivalent to VBA: `collection(index)` or `collection.Item(index)`
    /// Uses DISPATCH_METHOD because Item is a method in Office collections
    fn get_indexed_property(disp: &IDispatch, name: &str, index: i32) -> Result<VARIANT, String> {
        unsafe {
            let dispid = Self::get_dispid(disp, name)?;
            let mut result = VARIANT::default();

            // Create VARIANT for the index argument using the new API
            let arg = VARIANT::from(index);

            // COM passes arguments in reverse order
            let mut args = [arg];
            let params = DISPPARAMS {
                rgvarg: args.as_mut_ptr(),
                rgdispidNamedArgs: ptr::null_mut(),
                cArgs: 1,
                cNamedArgs: 0,
            };

            // Use DISPATCH_METHOD for Office Item() calls - they are methods, not properties
            disp.Invoke(
                dispid,
                &GUID::zeroed(),
                0,
                DISPATCH_METHOD,
                &params,
                Some(&mut result),
                None,
                None,
            )
            .map_err(|e| format!("Failed to get indexed property '{}[{}]': {}", name, index, e))?;

            Ok(result)
        }
    }

    /// Call a method with no arguments
    ///
    /// Equivalent to VBA: `obj.MethodName`
    fn invoke_method(disp: &IDispatch, name: &str) -> Result<VARIANT, String> {
        unsafe {
            let dispid = Self::get_dispid(disp, name)?;
            let mut result = VARIANT::default();
            let params = DISPPARAMS {
                rgvarg: ptr::null_mut(),
                rgdispidNamedArgs: ptr::null_mut(),
                cArgs: 0,
                cNamedArgs: 0,
            };

            disp.Invoke(
                dispid,
                &GUID::zeroed(),
                0,
                DISPATCH_METHOD,
                &params,
                Some(&mut result),
                None,
                None,
            )
            .map_err(|e| format!("Failed to invoke method '{}': {}", name, e))?;

            Ok(result)
        }
    }

    /// Extract an IDispatch from a VARIANT
    fn variant_to_dispatch(var: &VARIANT) -> Result<IDispatch, String> {
        IDispatch::try_from(var)
            .map_err(|e| format!("Expected IDispatch variant: {}", e))
    }

    /// Extract an i32 from a VARIANT
    fn variant_to_i32(var: &VARIANT) -> Result<i32, String> {
        i32::try_from(var)
            .map_err(|e| format!("Expected i32 variant: {}", e))
    }

    /// Extract a String from a VARIANT (BSTR)
    fn variant_to_string(var: &VARIANT) -> Result<String, String> {
        // First convert VARIANT to BSTR, then BSTR to String
        BSTR::try_from(var)
            .map(|b| b.to_string())
            .map_err(|e| format!("Expected BSTR variant: {}", e))
    }

    /// Get the Presentations collection from the Application
    fn get_presentations(app: &IDispatch) -> Result<IDispatch, String> {
        let var = Self::get_property(app, "Presentations")?;
        Self::variant_to_dispatch(&var)
    }

    /// Get a specific presentation by name
    fn get_presentation_by_name(app: &IDispatch, name: &str) -> Result<IDispatch, String> {
        let presentations = Self::get_presentations(app)?;

        // Get count of presentations
        let count_var = Self::get_property(&presentations, "Count")?;
        let count = Self::variant_to_i32(&count_var)?;

        // Iterate through presentations to find matching name
        for i in 1..=count {
            let pres_var = Self::get_indexed_property(&presentations, "Item", i)?;
            let pres = Self::variant_to_dispatch(&pres_var)?;

            let name_var = Self::get_property(&pres, "Name")?;
            let pres_name = Self::variant_to_string(&name_var)?;

            if pres_name == name {
                return Ok(pres);
            }
        }

        Err(format!("Presentation '{}' not found", name))
    }

    /// Get the SlideShowWindow from a presentation (if presenting)
    fn get_slideshow_window(pres: &IDispatch) -> Result<IDispatch, String> {
        let var = Self::get_property(pres, "SlideShowWindow")?;
        Self::variant_to_dispatch(&var)
    }

    /// Get the SlideShowView from a SlideShowWindow
    fn get_slideshow_view(window: &IDispatch) -> Result<IDispatch, String> {
        let var = Self::get_property(window, "View")?;
        Self::variant_to_dispatch(&var)
    }

    /// Get the Slides collection from a presentation
    fn get_slides(pres: &IDispatch) -> Result<IDispatch, String> {
        let var = Self::get_property(pres, "Slides")?;
        Self::variant_to_dispatch(&var)
    }
}

impl PresentationAdapter for PowerPointWindowsAdapter {
    fn get_open_presentations(&self) -> Result<Vec<String>, String> {
        // Initialize COM
        Self::init_com()?;

        let app = match Self::get_application() {
            Ok(a) => a,
            Err(_) => return Ok(vec![]), // PowerPoint not running
        };

        let presentations = Self::get_presentations(&app)?;

        // Get count
        let count_var = Self::get_property(&presentations, "Count")?;
        let count = Self::variant_to_i32(&count_var)?;

        let mut names = Vec::new();
        for i in 1..=count {
            let pres_var = Self::get_indexed_property(&presentations, "Item", i)?;
            let pres = Self::variant_to_dispatch(&pres_var)?;

            let name_var = Self::get_property(&pres, "Name")?;
            let name = Self::variant_to_string(&name_var)?;
            names.push(name);
        }

        Ok(names)
    }

    fn get_presentation_state(&self, name: &str) -> Result<PresentationState, String> {
        Self::init_com()?;

        let app = match Self::get_application() {
            Ok(a) => a,
            Err(_) => {
                return Ok(PresentationState {
                    is_open: false,
                    is_presenting: false,
                });
            }
        };

        // Check if presentation is open
        let pres = match Self::get_presentation_by_name(&app, name) {
            Ok(p) => p,
            Err(_) => {
                return Ok(PresentationState {
                    is_open: false,
                    is_presenting: false,
                });
            }
        };

        // Check if presenting (has SlideShowWindow)
        let is_presenting = Self::get_slideshow_window(&pres).is_ok();

        Ok(PresentationState {
            is_open: true,
            is_presenting,
        })
    }

    fn get_slide_info(&self, name: &str) -> Result<SlideInfo, String> {
        Self::init_com()?;

        let app = Self::get_application()?;
        let pres = Self::get_presentation_by_name(&app, name)?;

        // Get slide count
        let slides = Self::get_slides(&pres)?;
        let count_var = Self::get_property(&slides, "Count")?;
        let total = Self::variant_to_i32(&count_var)?;

        // Get current position from slideshow view
        let window = Self::get_slideshow_window(&pres)?;
        let view = Self::get_slideshow_view(&window)?;

        let pos_var = Self::get_property(&view, "CurrentShowPosition")?;
        let current = Self::variant_to_i32(&pos_var)?;

        Ok(SlideInfo { current, total })
    }

    fn next_slide(&self, name: &str) -> Result<SlideInfo, String> {
        Self::init_com()?;

        let app = Self::get_application()?;
        let pres = Self::get_presentation_by_name(&app, name)?;
        let window = Self::get_slideshow_window(&pres)?;
        let view = Self::get_slideshow_view(&window)?;

        // Get current position before moving
        let slides = Self::get_slides(&pres)?;
        let count_var = Self::get_property(&slides, "Count")?;
        let total = Self::variant_to_i32(&count_var)?;

        let pos_var = Self::get_property(&view, "CurrentShowPosition")?;
        let current_pos = Self::variant_to_i32(&pos_var)?;

        // Don't go past the last slide
        if current_pos >= total {
            return Ok(SlideInfo {
                current: current_pos,
                total,
            });
        }

        // Call Next method
        Self::invoke_method(&view, "Next")?;

        // Get new position
        let new_pos_var = Self::get_property(&view, "CurrentShowPosition")?;
        let new_pos = Self::variant_to_i32(&new_pos_var)?;

        Ok(SlideInfo {
            current: new_pos,
            total,
        })
    }

    fn prev_slide(&self, name: &str) -> Result<SlideInfo, String> {
        Self::init_com()?;

        let app = Self::get_application()?;
        let pres = Self::get_presentation_by_name(&app, name)?;
        let window = Self::get_slideshow_window(&pres)?;
        let view = Self::get_slideshow_view(&window)?;

        // Get current position before moving
        let slides = Self::get_slides(&pres)?;
        let count_var = Self::get_property(&slides, "Count")?;
        let total = Self::variant_to_i32(&count_var)?;

        let pos_var = Self::get_property(&view, "CurrentShowPosition")?;
        let current_pos = Self::variant_to_i32(&pos_var)?;

        // Don't go before the first slide
        if current_pos <= 1 {
            return Ok(SlideInfo {
                current: current_pos,
                total,
            });
        }

        // Call Previous method
        Self::invoke_method(&view, "Previous")?;

        // Get new position
        let new_pos_var = Self::get_property(&view, "CurrentShowPosition")?;
        let new_pos = Self::variant_to_i32(&new_pos_var)?;

        Ok(SlideInfo {
            current: new_pos,
            total,
        })
    }

    fn get_presenter_notes(&self, name: &str) -> Result<Option<String>, String> {
        Self::init_com()?;

        let app = Self::get_application()?;
        let pres = Self::get_presentation_by_name(&app, name)?;

        // Get current slide index from slideshow view
        let window = Self::get_slideshow_window(&pres)?;
        let view = Self::get_slideshow_view(&window)?;
        let pos_var = Self::get_property(&view, "CurrentShowPosition")?;
        let current_idx = Self::variant_to_i32(&pos_var)?;

        // Navigate: Slides(idx) → NotesPage → Shapes → Placeholders → Item(2) → TextFrame → TextRange → Text
        let slides = Self::get_slides(&pres)?;
        let slide_var = Self::get_indexed_property(&slides, "Item", current_idx)?;
        let slide = Self::variant_to_dispatch(&slide_var)?;

        let notes_page_var = Self::get_property(&slide, "NotesPage")?;
        let notes_page = Self::variant_to_dispatch(&notes_page_var)?;

        let shapes_var = Self::get_property(&notes_page, "Shapes")?;
        let shapes = Self::variant_to_dispatch(&shapes_var)?;

        let placeholders_var = Self::get_property(&shapes, "Placeholders")?;
        let placeholders = Self::variant_to_dispatch(&placeholders_var)?;

        // Placeholder index 2 is the notes text placeholder
        let placeholder_var = Self::get_indexed_property(&placeholders, "Item", 2)?;
        let placeholder = Self::variant_to_dispatch(&placeholder_var)?;

        let text_frame_var = Self::get_property(&placeholder, "TextFrame")?;
        let text_frame = Self::variant_to_dispatch(&text_frame_var)?;

        let text_range_var = Self::get_property(&text_frame, "TextRange")?;
        let text_range = Self::variant_to_dispatch(&text_range_var)?;

        let text_var = Self::get_property(&text_range, "Text")?;
        let text = Self::variant_to_string(&text_var)?;

        if text.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(text))
        }
    }

    fn get_all_presenter_notes(&self, name: &str) -> Result<std::collections::HashMap<i32, String>, String> {
        Self::init_com()?;

        let app = Self::get_application()?;
        let pres = Self::get_presentation_by_name(&app, name)?;
        let slides = Self::get_slides(&pres)?;

        let count_var = Self::get_property(&slides, "Count")?;
        let count = Self::variant_to_i32(&count_var)?;

        let mut notes = std::collections::HashMap::new();

        for i in 1..=count {
            let slide_var = match Self::get_indexed_property(&slides, "Item", i) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let slide = match Self::variant_to_dispatch(&slide_var) {
                Ok(d) => d,
                Err(_) => continue,
            };

            // Navigate: Slide → NotesPage → Shapes → Placeholders → Item(2) → TextFrame → TextRange → Text
            let text = (|| -> Result<String, String> {
                let notes_page_var = Self::get_property(&slide, "NotesPage")?;
                let notes_page = Self::variant_to_dispatch(&notes_page_var)?;
                let shapes_var = Self::get_property(&notes_page, "Shapes")?;
                let shapes = Self::variant_to_dispatch(&shapes_var)?;
                let placeholders_var = Self::get_property(&shapes, "Placeholders")?;
                let placeholders = Self::variant_to_dispatch(&placeholders_var)?;
                let placeholder_var = Self::get_indexed_property(&placeholders, "Item", 2)?;
                let placeholder = Self::variant_to_dispatch(&placeholder_var)?;
                let text_frame_var = Self::get_property(&placeholder, "TextFrame")?;
                let text_frame = Self::variant_to_dispatch(&text_frame_var)?;
                let text_range_var = Self::get_property(&text_frame, "TextRange")?;
                let text_range = Self::variant_to_dispatch(&text_range_var)?;
                let text_var = Self::get_property(&text_range, "Text")?;
                Self::variant_to_string(&text_var)
            })();

            if let Ok(text) = text {
                let text = text.trim().to_string();
                if !text.is_empty() {
                    notes.insert(i, text);
                }
            }
        }

        Ok(notes)
    }

    fn get_notes_zoom(&self) -> Result<Option<i32>, String> {
        // Notes zoom requires accessing the Presenter View window
        // This is more complex and may not be available in all slideshow modes
        // For now, return None (not supported)
        //
        // To implement fully, you would need:
        // Application.PresenterViewWindow.PresenterTool.NotesZoom
        Ok(None)
    }

    fn set_notes_zoom(&self, _level: i32) -> Result<(), String> {
        // Not implemented for Windows yet
        Err("Notes zoom not yet supported on Windows".to_string())
    }
}

impl PowerPointWindowsAdapter {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_levels() {
        assert_eq!(PowerPointWindowsAdapter::get_next_zoom_level(100), 150);
        assert_eq!(PowerPointWindowsAdapter::get_next_zoom_level(150), 200);
        assert_eq!(PowerPointWindowsAdapter::get_next_zoom_level(400), 400);

        assert_eq!(PowerPointWindowsAdapter::get_prev_zoom_level(400), 300);
        assert_eq!(PowerPointWindowsAdapter::get_prev_zoom_level(150), 100);
        assert_eq!(PowerPointWindowsAdapter::get_prev_zoom_level(100), 100);
    }
}
