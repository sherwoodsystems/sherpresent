//! What each output's helper is fed on stdin: the same NDJSON a web page for
//! that content gets over its socket, so native and web outputs agree.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::{broadcast, watch};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::{BroadcastStream, WatchStream};
use tokio_stream::{Stream, StreamExt};

use crate::adapters::LiveStatus;
use crate::captions::{settings_message, CaptionSinks};
use crate::ontime::TimerState;
use crate::osc::StateManager;

pub type FeedStream = Pin<Box<dyn Stream<Item = String> + Send>>;

/// Opens a helper's stdin feed: opening lines with the current state, then
/// live updates. Called on every (re)spawn, so a respawned helper starts from
/// what's showing now. The stream ending stops the output.
pub type Feed = Arc<dyn Fn() -> FeedStream + Send + Sync>;

/// Captions: the `/api/captions/ws` messages (unpinned settings).
pub fn captions(sinks: CaptionSinks) -> Feed {
    Arc::new(move || {
        // Subscribe before snapshotting, so nothing published in between is lost.
        let updates = BroadcastStream::new(sinks.broadcast.subscribe());
        let mut overlay = sinks.overlay.subscribe();
        let settings = overlay.borrow_and_update().clone();
        let opening = sinks.opening_messages(&settings);

        let replay = sinks.clone();
        let updates = updates.filter_map(move |update| match update {
            Ok(u) => Some(u.to_json()),
            // Behind: only the newest lines matter, so resync from the
            // replay buffer rather than replaying the backlog.
            Err(BroadcastStreamRecvError::Lagged(n)) => {
                log::warn!("Caption output lagged, skipped {} updates", n);
                Some(replay.replay_message())
            }
        });
        let settings = WatchStream::from_changes(overlay).map(|s| settings_message(&s));
        Box::pin(tokio_stream::iter(opening).chain(updates.merge(settings)))
    })
}

/// Where the notes output's data comes from: the channels the stage view's
/// `/api/ws` socket reads.
#[derive(Clone)]
pub struct NotesSources {
    pub notes: Arc<Mutex<HashMap<i32, String>>>,
    pub notes_broadcast: broadcast::Sender<HashMap<i32, String>>,
    pub status_broadcast: broadcast::Sender<LiveStatus>,
    /// For the opening status: the broadcast only carries changes.
    pub state_manager: Arc<Mutex<Option<Arc<StateManager>>>>,
}

impl NotesSources {
    fn current_status(&self) -> LiveStatus {
        self.state_manager
            .lock()
            .unwrap()
            .as_ref()
            .map(|sm| LiveStatus::from(&sm.get_state()))
            .unwrap_or_default()
    }

    fn notes_message(&self) -> String {
        message("notes", &*self.notes.lock().unwrap())
    }
}

/// Notes: the `/api/ws` `status` and `notes` messages, plus `timer` with the
/// Ontime state (`null` when Ontime isn't configured).
pub fn notes(src: NotesSources, timer: Option<watch::Receiver<TimerState>>) -> Feed {
    Arc::new(move || {
        // Subscribe before snapshotting, so nothing published in between is lost.
        let status = BroadcastStream::new(src.status_broadcast.subscribe());
        let notes = BroadcastStream::new(src.notes_broadcast.subscribe());
        let mut opening = vec![
            message("status", &src.current_status()),
            src.notes_message(),
        ];

        let timer: FeedStream = match &timer {
            Some(rx) => {
                let mut rx = rx.clone();
                opening.push(message("timer", &*rx.borrow_and_update()));
                Box::pin(WatchStream::from_changes(rx).map(|t| message("timer", &t)))
            }
            None => {
                opening.push(message("timer", &()));
                Box::pin(tokio_stream::empty())
            }
        };

        // Lagged: resend the current state rather than the backlog.
        let s = src.clone();
        let status =
            status.map(move |r| message("status", &r.unwrap_or_else(|_| s.current_status())));
        let s = src.clone();
        let notes = notes.map(move |r| match r {
            Ok(n) => message("notes", &n),
            Err(_) => s.notes_message(),
        });
        Box::pin(tokio_stream::iter(opening).chain(status.merge(notes).merge(timer)))
    })
}

fn message(kind: &str, payload: &impl Serialize) -> String {
    serde_json::json!({ "type": kind, "payload": payload }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources() -> NotesSources {
        NotesSources {
            notes: Arc::new(Mutex::new(HashMap::from([(1, "Hello".to_string())]))),
            notes_broadcast: broadcast::channel(4).0,
            status_broadcast: broadcast::channel(4).0,
            state_manager: Arc::new(Mutex::new(None)),
        }
    }

    async fn next(feed: &mut FeedStream) -> serde_json::Value {
        serde_json::from_str(&feed.next().await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn test_notes_feed_opens_with_state_then_follows() {
        let src = sources();
        let (timer_tx, timer_rx) = watch::channel(TimerState::default());
        let mut feed = notes(src.clone(), Some(timer_rx))();

        let status = next(&mut feed).await;
        assert_eq!(status["type"], "status");
        assert_eq!(status["payload"]["current_slide"], 0);
        assert_eq!(next(&mut feed).await["payload"]["1"], "Hello");
        assert_eq!(next(&mut feed).await["payload"]["connected"], false);

        src.status_broadcast
            .send(LiveStatus {
                current_slide: 7,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(next(&mut feed).await["payload"]["current_slide"], 7);

        timer_tx.send_modify(|t| t.title = "Keynote".into());
        let timer = next(&mut feed).await;
        assert_eq!(timer["type"], "timer");
        assert_eq!(timer["payload"]["title"], "Keynote");
    }

    #[tokio::test]
    async fn test_notes_feed_without_ontime_sends_null_timer() {
        let mut feed = notes(sources(), None)();
        next(&mut feed).await;
        next(&mut feed).await;
        let timer = next(&mut feed).await;
        assert_eq!(timer["type"], "timer");
        assert!(timer["payload"].is_null());
    }
}
