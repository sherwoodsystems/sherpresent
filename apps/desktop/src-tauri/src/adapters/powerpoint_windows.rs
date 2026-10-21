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

use super::{Builds, LiveStatus, PresentationAdapter, PresentationState, SlideInfo};

use windows::{
    core::{Interface, BSTR, GUID, PCWSTR},
    Win32::System::Com::{
        CLSIDFromProgID, CoCreateInstance, CoInitializeEx, IDispatch, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED, DISPATCH_FLAGS, DISPATCH_METHOD, DISPATCH_PROPERTYGET,
        DISPPARAMS,
    },
    Win32::System::Ole::GetActiveObject,
    Win32::System::Variant::VARIANT,
};

use std::ptr;

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
                    return Err(format!(
                        "Failed to initialize COM: HRESULT 0x{:08X}",
                        code as u32
                    ));
                }
            }
            Ok(())
        }
    }

    /// The running PowerPoint Application, or `None` if it isn't running.
    /// Never starts PowerPoint.
    pub(super) fn get_running_application() -> Result<Option<IDispatch>, String> {
        unsafe {
            let clsid = Self::clsid()?;
            let mut punk = None;
            if GetActiveObject(&clsid, None, &mut punk).is_err() {
                return Ok(None);
            }
            punk.map(|unk| {
                unk.cast::<IDispatch>()
                    .map_err(|e| format!("Failed to get IDispatch from running instance: {}", e))
            })
            .transpose()
        }
    }

    /// Get IDispatch interface to running PowerPoint Application
    ///
    /// First tries to attach to an existing PowerPoint instance using GetActiveObject.
    /// If no instance is running, creates a new one with CoCreateInstance.
    fn get_application() -> Result<IDispatch, String> {
        if let Some(app) = Self::get_running_application()? {
            return Ok(app);
        }
        unsafe {
            // No running instance, try to create one
            // Note: This will start PowerPoint if not running
            let app: IDispatch = CoCreateInstance(&Self::clsid()?, None, CLSCTX_LOCAL_SERVER)
                .map_err(|e| format!("Failed to create PowerPoint instance: {}", e))?;

            Ok(app)
        }
    }

    /// PowerPoint's CLSID, from its ProgID.
    fn clsid() -> Result<GUID, String> {
        let prog_id: Vec<u16> = "PowerPoint.Application\0".encode_utf16().collect();
        unsafe { CLSIDFromProgID(PCWSTR(prog_id.as_ptr())) }
            .map_err(|e| format!("PowerPoint not installed or ProgID not found: {}", e))
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

    /// Invoke `name` on `disp` with `args`, retrying while PowerPoint is busy.
    ///
    /// PowerPoint rejects incoming COM calls while it shows a modal dialog or is
    /// mid-transition (`RPC_E_CALL_REJECTED` / `RPC_E_SERVERCALL_RETRYLATER`).
    /// Those clear on their own, so a few short retries ride them out instead
    /// of dropping the poll.
    fn invoke(
        disp: &IDispatch,
        name: &str,
        flags: DISPATCH_FLAGS,
        args: &mut [VARIANT],
    ) -> Result<VARIANT, String> {
        const RPC_E_CALL_REJECTED: i32 = 0x8001_0001_u32 as i32;
        const RPC_E_SERVERCALL_RETRYLATER: i32 = 0x8001_010A_u32 as i32;
        const RETRIES: u32 = 3;

        let dispid = Self::get_dispid(disp, name)?;
        let params = DISPPARAMS {
            rgvarg: if args.is_empty() {
                ptr::null_mut()
            } else {
                args.as_mut_ptr()
            },
            rgdispidNamedArgs: ptr::null_mut(),
            cArgs: args.len() as u32,
            cNamedArgs: 0,
        };

        let mut attempt = 0;
        loop {
            let mut result = VARIANT::default();
            let outcome = unsafe {
                disp.Invoke(
                    dispid,
                    &GUID::zeroed(),
                    0, // LOCALE_SYSTEM_DEFAULT
                    flags,
                    &params,
                    Some(&mut result),
                    None,
                    None,
                )
            };
            match outcome {
                Ok(()) => return Ok(result),
                Err(e)
                    if attempt < RETRIES
                        && matches!(
                            e.code().0,
                            RPC_E_CALL_REJECTED | RPC_E_SERVERCALL_RETRYLATER
                        ) =>
                {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(50 * attempt as u64));
                }
                Err(e) => return Err(format!("'{}' failed: {}", name, e)),
            }
        }
    }

    /// Get a property from an IDispatch object
    ///
    /// Equivalent to VBA: `obj.PropertyName`
    pub(super) fn get_property(disp: &IDispatch, name: &str) -> Result<VARIANT, String> {
        Self::invoke(disp, name, DISPATCH_PROPERTYGET, &mut [])
    }

    /// Get an indexed item from a collection
    ///
    /// Equivalent to VBA: `collection.Item(index)`. Office's `Item` is a
    /// method, not a property, so this uses DISPATCH_METHOD.
    fn get_indexed_property(disp: &IDispatch, name: &str, index: i32) -> Result<VARIANT, String> {
        Self::invoke(disp, name, DISPATCH_METHOD, &mut [VARIANT::from(index)])
    }

    /// Call a method with no arguments
    ///
    /// Equivalent to VBA: `obj.MethodName`
    fn invoke_method(disp: &IDispatch, name: &str) -> Result<VARIANT, String> {
        Self::invoke(disp, name, DISPATCH_METHOD, &mut [])
    }

    /// Call a method with a single integer argument
    ///
    /// Equivalent to VBA: `obj.MethodName(intArg)`
    fn invoke_method_with_int_arg(
        disp: &IDispatch,
        name: &str,
        arg_value: i32,
    ) -> Result<VARIANT, String> {
        Self::invoke(disp, name, DISPATCH_METHOD, &mut [VARIANT::from(arg_value)])
    }

    /// Extract an IDispatch from a VARIANT
    pub(super) fn variant_to_dispatch(var: &VARIANT) -> Result<IDispatch, String> {
        IDispatch::try_from(var).map_err(|e| format!("Expected IDispatch variant: {}", e))
    }

    /// Extract an i32 from a VARIANT
    pub(super) fn variant_to_i32(var: &VARIANT) -> Result<i32, String> {
        i32::try_from(var).map_err(|e| format!("Expected i32 variant: {}", e))
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

    /// Presentation `name`, starting PowerPoint if it isn't running.
    fn presentation(name: &str) -> Result<IDispatch, String> {
        Self::init_com()?;
        Self::get_presentation_by_name(&Self::get_application()?, name)
    }

    /// The deck's slide count.
    fn slide_count(pres: &IDispatch) -> Result<i32, String> {
        Self::variant_to_i32(&Self::get_property(&Self::get_slides(pres)?, "Count")?)
    }

    /// The SlideShowView of `pres`, if it is presenting.
    fn show_view(pres: &IDispatch) -> Result<IDispatch, String> {
        Self::get_slideshow_view(&Self::get_slideshow_window(pres)?)
    }

    /// The running show of presentation `name`: its SlideShowView, and the
    /// deck's slide count.
    fn show(name: &str) -> Result<(IDispatch, i32), String> {
        let pres = Self::presentation(name)?;
        Ok((Self::show_view(&pres)?, Self::slide_count(&pres)?))
    }

    /// Speaker notes of the slide on screen, if it has any.
    fn notes_on_screen(view: &IDispatch) -> Result<Option<String>, String> {
        let slide = Self::variant_to_dispatch(&Self::get_property(view, "Slide")?)?;
        let text = Self::slide_notes(&slide)?;
        Ok((!text.is_empty()).then_some(text))
    }

    /// Where the show stands: slide, slide count and builds.
    fn position(view: &IDispatch, total: i32) -> Result<SlideInfo, String> {
        let current = Self::variant_to_i32(&Self::get_property(view, "CurrentShowPosition")?)?;
        Ok(SlideInfo {
            current,
            total,
            transition_duration: None,
            builds: Self::builds(view),
        })
    }

    /// The slide's click-triggered builds: how many have played
    /// (`GetClickIndex`) of how many (`GetClickCount`).
    fn builds(view: &IDispatch) -> Option<Builds> {
        let read = |method: &str| {
            Self::invoke_method(view, method)
                .ok()
                .and_then(|v| Self::variant_to_i32(&v).ok())
        };
        Some(Builds {
            done: read("GetClickIndex")?,
            total: read("GetClickCount")?,
        })
    }

    /// Speaker notes text of `slide`, trimmed.
    ///
    /// The notes live in the notes page's body placeholder. Its position in
    /// `Placeholders` depends on the notes master (index 2 on the default
    /// master, but not on customised ones), so find it by type, as the macOS
    /// adapter does.
    fn slide_notes(slide: &IDispatch) -> Result<String, String> {
        const PP_PLACEHOLDER_BODY: i32 = 2;

        let notes_page = Self::variant_to_dispatch(&Self::get_property(slide, "NotesPage")?)?;
        let shapes = Self::variant_to_dispatch(&Self::get_property(&notes_page, "Shapes")?)?;
        let placeholders =
            Self::variant_to_dispatch(&Self::get_property(&shapes, "Placeholders")?)?;
        let count = Self::variant_to_i32(&Self::get_property(&placeholders, "Count")?)?;

        for i in 1..=count {
            let shape =
                Self::variant_to_dispatch(&Self::get_indexed_property(&placeholders, "Item", i)?)?;
            let format =
                Self::variant_to_dispatch(&Self::get_property(&shape, "PlaceholderFormat")?)?;
            if Self::variant_to_i32(&Self::get_property(&format, "Type")?)? != PP_PLACEHOLDER_BODY {
                continue;
            }
            let text_frame = Self::variant_to_dispatch(&Self::get_property(&shape, "TextFrame")?)?;
            let text_range =
                Self::variant_to_dispatch(&Self::get_property(&text_frame, "TextRange")?)?;
            let text = Self::variant_to_string(&Self::get_property(&text_range, "Text")?)?;
            return Ok(text.trim().to_string());
        }

        Ok(String::new())
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

    fn get_presenting_presentations(&self) -> Result<Vec<String>, String> {
        Self::init_com()?;

        // Polled while nothing is presenting, so it must not start PowerPoint.
        let Some(app) = Self::get_running_application()? else {
            return Ok(vec![]);
        };

        let windows = Self::variant_to_dispatch(&Self::get_property(&app, "SlideShowWindows")?)?;
        let count = Self::variant_to_i32(&Self::get_property(&windows, "Count")?)?;

        let mut names = Vec::new();
        for i in 1..=count {
            let window =
                Self::variant_to_dispatch(&Self::get_indexed_property(&windows, "Item", i)?)?;
            let pres = Self::variant_to_dispatch(&Self::get_property(&window, "Presentation")?)?;
            names.push(Self::variant_to_string(&Self::get_property(
                &pres, "Name",
            )?)?);
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
        let (view, total) = Self::show(name)?;
        Self::position(&view, total)
    }

    fn next_slide(&self, name: &str) -> Result<SlideInfo, String> {
        let (view, total) = Self::show(name)?;
        let here = Self::position(&view, total)?;
        // Next on the last slide's last build would end the show.
        let builds_left = here.builds.is_some_and(|b| b.done < b.total);
        if here.current >= total && !builds_left {
            return Ok(here);
        }
        Self::invoke_method(&view, "Next")?;
        Self::position(&view, total)
    }

    fn prev_slide(&self, name: &str) -> Result<SlideInfo, String> {
        let (view, total) = Self::show(name)?;
        let here = Self::position(&view, total)?;
        let builds_played = here.builds.is_some_and(|b| b.done > 0);
        if here.current <= 1 && !builds_played {
            return Ok(here);
        }
        Self::invoke_method(&view, "Previous")?;
        Self::position(&view, total)
    }

    fn goto_slide(&self, name: &str, slide: i32) -> Result<SlideInfo, String> {
        let (view, total) = Self::show(name)?;
        Self::invoke_method_with_int_arg(&view, "GotoSlide", slide.clamp(1, total))?;
        Self::position(&view, total)
    }

    fn get_presenter_notes(&self, name: &str) -> Result<Option<String>, String> {
        Self::notes_on_screen(&Self::show_view(&Self::presentation(name)?)?)
    }

    fn get_all_presenter_notes(
        &self,
        name: &str,
    ) -> Result<std::collections::HashMap<i32, String>, String> {
        let slides = Self::get_slides(&Self::presentation(name)?)?;

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

            if let Ok(text) = Self::slide_notes(&slide) {
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

    fn get_live_status(&self, name: &str) -> LiveStatus {
        // Polled, so it must not start PowerPoint.
        let pres = Self::init_com()
            .and_then(|()| Self::get_running_application())
            .ok()
            .flatten()
            .and_then(|app| Self::get_presentation_by_name(&app, name).ok());
        let Some(pres) = pres else {
            return LiveStatus::default();
        };
        let Ok(view) = Self::show_view(&pres) else {
            return LiveStatus {
                is_open: true,
                ..Default::default()
            };
        };

        // Builds are reported even when there are none (total 0), so the
        // StateManager knows this slide's builds rather than guessing.
        let builds = Self::builds(&view);
        LiveStatus {
            is_open: true,
            is_presenting: true,
            current_slide: Self::get_property(&view, "CurrentShowPosition")
                .and_then(|v| Self::variant_to_i32(&v))
                .unwrap_or(0),
            total_slides: Self::slide_count(&pres).unwrap_or(0),
            zoom_level: None,
            presenter_notes: Self::notes_on_screen(&view).ok().flatten(),
            current_build: builds.map(|b| b.done),
            total_builds: builds.map(|b| b.total),
        }
    }
}
