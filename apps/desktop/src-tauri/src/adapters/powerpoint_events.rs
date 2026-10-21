//! PowerPoint's own slide show events (Windows only).
//!
//! PowerPoint's `Application` fires events on its outgoing `EApplication`
//! interface: `SlideShowNextSlide` when the show lands on a slide,
//! `SlideShowNextClick` when a build plays, `SlideShowOnNext` /
//! `SlideShowOnPrevious` for Next and Previous within a slide, and so on.
//! Listening to them lets a clicker press in PowerPoint be read the moment it
//! happens instead of at the next poll. Polling carries on as before, so a
//! missed event costs only time.
//!
//! Every event is treated alike: "something changed, read the slide now".
//! That keeps this module free of per-event logic and makes events whose
//! DISPIDs aren't documented (`SlideShowOnNext`, `SlideShowOnPrevious`) work
//! too. Each event is logged with its DISPID.
//!
//! We listen only while a slide show is running. A connection holds a
//! reference to PowerPoint, and Office apps keep running in the background
//! while another program holds one, so PowerPoint could otherwise linger
//! after the operator closes it.
//!
//! ## References
//!
//! - Microsoft KB 309309, "How to handle PowerPoint events by using Visual
//!   C++": the `EApplication` IID and the event DISPIDs 2001-2019.
//! - [Application events](https://learn.microsoft.com/en-us/office/vba/api/powerpoint.application)

use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::util::LockExt;

/// Whether PowerPoint's events are reaching us, for the Control tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum EventsStatus {
    /// No slide show is running, so there's nothing to listen to.
    NoShow,
    /// Listening to a running show; no event has arrived yet.
    Listening,
    /// Events have arrived during this show.
    Receiving,
    /// PowerPoint refused the connection.
    Failed { error: String },
}

/// The latest `EventsStatus`, published to the UI as `powerpoint-events`.
pub struct PowerPointEvents {
    status: Mutex<EventsStatus>,
    app: OnceLock<AppHandle>,
}

impl Default for PowerPointEvents {
    fn default() -> Self {
        Self {
            status: Mutex::new(EventsStatus::NoShow),
            app: OnceLock::new(),
        }
    }
}

impl PowerPointEvents {
    pub fn status(&self) -> EventsStatus {
        self.status.locked().clone()
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn set(&self, status: EventsStatus) {
        {
            let mut current = self.status.locked();
            if *current == status {
                return;
            }
            log::info!("PowerPoint events: {:?}", status);
            *current = status.clone();
        }
        if let Some(app) = self.app.get() {
            let _ = app.emit("powerpoint-events", &status);
        }
    }
}

/// Listen to PowerPoint's events for as long as the app runs. `active` says
/// whether PowerPoint is the adapter in use; `on_event` is called on a COM
/// thread for every event and must return quickly.
#[cfg(windows)]
pub fn start(
    events: std::sync::Arc<PowerPointEvents>,
    app: AppHandle,
    active: impl Fn() -> bool + Send + 'static,
    on_event: impl Fn() + Send + Sync + 'static,
) {
    let _ = events.app.set(app);
    let spawned = std::thread::Builder::new()
        .name("powerpoint-events".into())
        .spawn(move || listener::run(&events, active, on_event));
    if let Err(e) = spawned {
        log::error!(
            "PowerPoint events: couldn't start the listener thread: {}",
            e
        );
    }
}

#[cfg(windows)]
mod listener {
    use std::sync::Arc;
    use std::time::Duration;

    use windows::core::{implement, interface, IUnknown, Interface, GUID, PCWSTR};
    use windows::Win32::Foundation::E_NOTIMPL;
    use windows::Win32::System::Com::{
        CoInitializeEx, IConnectionPoint, IConnectionPointContainer, IDispatch, IDispatch_Impl,
        IDispatch_Vtbl, ITypeInfo, COINIT_MULTITHREADED, DISPATCH_FLAGS, DISPPARAMS, EXCEPINFO,
    };
    use windows::Win32::System::Variant::VARIANT;

    use super::{EventsStatus, PowerPointEvents};
    use crate::adapters::powerpoint_windows::PowerPointWindowsAdapter as Com;

    /// How often to check whether a slide show has started or ended.
    const CHECK_EVERY: Duration = Duration::from_secs(1);

    /// PowerPoint's event interface. It derives from `IDispatch` but isn't a
    /// dispinterface: events arrive through `Invoke` with fixed DISPIDs.
    #[interface("914934C2-5A91-11CF-8700-00AA0060263B")]
    unsafe trait EApplication: IDispatch {}

    /// The object PowerPoint calls for each event.
    #[implement(EApplication)]
    struct Sink {
        on_event: Arc<dyn Fn(i32) + Send + Sync>,
    }

    impl EApplication_Impl for Sink_Impl {}

    impl IDispatch_Impl for Sink_Impl {
        fn GetTypeInfoCount(&self) -> windows::core::Result<u32> {
            Ok(0)
        }

        fn GetTypeInfo(&self, _itinfo: u32, _lcid: u32) -> windows::core::Result<ITypeInfo> {
            Err(E_NOTIMPL.into())
        }

        fn GetIDsOfNames(
            &self,
            _riid: *const GUID,
            _rgsznames: *const PCWSTR,
            _cnames: u32,
            _lcid: u32,
            _rgdispid: *mut i32,
        ) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }

        fn Invoke(
            &self,
            dispidmember: i32,
            _riid: *const GUID,
            _lcid: u32,
            _wflags: DISPATCH_FLAGS,
            _pdispparams: *const DISPPARAMS,
            _pvarresult: *mut VARIANT,
            _pexcepinfo: *mut EXCEPINFO,
            _puargerr: *mut u32,
        ) -> windows::core::Result<()> {
            (self.on_event)(dispidmember);
            Ok(())
        }
    }

    /// An `Advise`d sink; dropping it disconnects.
    struct Connection {
        point: IConnectionPoint,
        cookie: u32,
    }

    impl Connection {
        fn open(app: &IDispatch, sink: &IUnknown) -> Result<Self, String> {
            let container: IConnectionPointContainer = app.cast().map_err(|e| e.to_string())?;
            unsafe {
                let point = container
                    .FindConnectionPoint(&EApplication::IID)
                    .map_err(|e| e.to_string())?;
                let cookie = point.Advise(sink).map_err(|e| e.to_string())?;
                Ok(Self { point, cookie })
            }
        }
    }

    impl Drop for Connection {
        fn drop(&mut self) {
            // Fails if PowerPoint has quit, which disconnects us anyway.
            let _ = unsafe { self.point.Unadvise(self.cookie) };
        }
    }

    /// The PowerPoint application, if it's running with a slide show up.
    fn running_show() -> Option<IDispatch> {
        let app = Com::get_running_application().ok()??;
        let shows = Com::get_property(&app, "SlideShowWindows").ok()?;
        let shows = Com::variant_to_dispatch(&shows).ok()?;
        let count = Com::variant_to_i32(&Com::get_property(&shows, "Count").ok()?).ok()?;
        (count > 0).then_some(app)
    }

    /// Event names by DISPID, for the log (KB 309309). Later events, such as
    /// `SlideShowOnNext`, are logged by number.
    fn event_name(dispid: i32) -> Option<&'static str> {
        Some(match dispid {
            2011 => "SlideShowBegin",
            2012 => "SlideShowNextBuild",
            2013 => "SlideShowNextSlide",
            2014 => "SlideShowEnd",
            2019 => "SlideShowNextClick",
            _ => return None,
        })
    }

    pub(super) fn run(
        events: &Arc<PowerPointEvents>,
        active: impl Fn() -> bool,
        on_event: impl Fn() + Send + Sync + 'static,
    ) {
        // A free-threaded apartment: events arrive on COM's own threads, so
        // this one needs no message loop and can sleep between checks.
        let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if init.is_err() {
            events.set(EventsStatus::Failed {
                error: format!("COM didn't start: {init:?}"),
            });
            return;
        }

        let sink: EApplication = Sink {
            on_event: Arc::new({
                let events = events.clone();
                move |dispid| {
                    match event_name(dispid) {
                        Some(name) => log::info!("PowerPoint event: {name} ({dispid})"),
                        None => log::info!("PowerPoint event: {dispid}"),
                    }
                    events.set(EventsStatus::Receiving);
                    on_event();
                }
            }),
        }
        .into();
        let sink: IUnknown = match sink.cast() {
            Ok(unknown) => unknown,
            Err(e) => {
                events.set(EventsStatus::Failed {
                    error: e.to_string(),
                });
                return;
            }
        };

        let mut connection: Option<Connection> = None;
        loop {
            match (active().then(running_show).flatten(), &connection) {
                (Some(app), None) => match Connection::open(&app, &sink) {
                    Ok(opened) => {
                        connection = Some(opened);
                        events.set(EventsStatus::Listening);
                    }
                    Err(error) => events.set(EventsStatus::Failed { error }),
                },
                (None, _) => {
                    connection = None;
                    events.set(EventsStatus::NoShow);
                }
                (Some(_), Some(_)) => {}
            }
            std::thread::sleep(CHECK_EVERY);
        }
    }
}
