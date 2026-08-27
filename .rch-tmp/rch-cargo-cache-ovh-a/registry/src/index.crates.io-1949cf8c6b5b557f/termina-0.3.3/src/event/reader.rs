//! Thread-safe terminal event reader.
//!
//! This module provides an `Arc<Mutex<T>>` wrapper around the platform event source. That lets a
//! reader live on a terminal handle and also be shared with the optional async stream, rather than
//! being stored globally.
//!
//! # Implementation Notes
//!
//! This is adapted from [crossterm's event reader]. The shared reader is mostly an
//! `Arc<Mutex<T>>` wrapper around the same shape as crossterm's internal event reader. This lets
//! it live on a [`Terminal`] and on an `EventStream` instead of being stored globally. Termina uses
//! `Fn(&Event) -> bool` filters instead of a dedicated filter trait so callers can pass ordinary
//! closures.
//!
//! [crossterm's event reader]: https://docs.rs/crossterm/latest/crossterm/event/index.html
//! [`Terminal`]: crate::Terminal

use std::{collections::VecDeque, io, sync::Arc, time::Duration};

use parking_lot::Mutex;

use super::{
    source::{EventSource as _, PlatformEventSource, PlatformWaker, PollTimeout},
    Event,
};

/// A reader of events from the terminal's input handle.
///
/// Note that this type wraps an `Arc` and is cheap to clone. If the `event-stream` feature is
/// enabled then this value should be passed to `EventStream::new`.
///
/// [`Self::read`] and [`Self::poll`] both take filters. Events rejected by a filter remain buffered
/// so a caller can wait for a key press without discarding protocol responses, mouse events, or
/// other input that another part of the application may read later. Filtering preserves rejected
/// events for later reads, but callers should not rely on rejected events being re-buffered in exact
/// stream order across multiple filtered reads.
///
/// # Examples
///
/// Read every event and branch on the event kind:
///
/// ```no_run
/// use std::io;
///
/// use termina::{
///     event::{Event, KeyCode, KeyEventKind},
///     PlatformTerminal, Terminal,
/// };
///
/// fn main() -> io::Result<()> {
///     let reader = PlatformTerminal::new()?.event_reader();
///     loop {
///         let event = reader.read(|_| true)?;
///         match event {
///             Event::Key(key)
///                 if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') =>
///             {
///                 break
///             }
///             Event::Mouse(mouse) => eprintln!("mouse at {},{}", mouse.column, mouse.row),
///             Event::Csi(csi) => eprintln!("CSI response: {csi:?}"),
///             _ => {}
///         }
///     }
///     Ok(())
/// }
/// ```
///
/// Use a filter when a call should wait for a specific class of event:
///
/// ```no_run
/// use std::io;
///
/// use termina::{
///     event::{Event, KeyEventKind},
///     PlatformTerminal, Terminal,
/// };
///
/// fn main() -> io::Result<()> {
///     let reader = PlatformTerminal::new()?.event_reader();
///     let event = reader.read(|event| {
///         matches!(event, Event::Key(key) if key.kind == KeyEventKind::Press)
///     })?;
///     println!("received {event:?}");
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct EventReader {
    shared: Arc<Mutex<Shared>>,
}

impl EventReader {
    pub(crate) fn new(source: PlatformEventSource) -> Self {
        let shared = Shared {
            events: VecDeque::with_capacity(32),
            source,
            skipped_events: Vec::with_capacity(32),
        };
        Self {
            shared: Arc::new(Mutex::new(shared)),
        }
    }

    /// Returns a platform-specific waker that can unblock [`poll`](Self::poll) calls.
    pub fn waker(&self) -> PlatformWaker {
        let reader = self.shared.lock();
        reader.source.waker()
    }

    /// Polls for availability of an event matching `filter`.
    ///
    /// When `timeout` is `None`, this call blocks indefinitely. Events rejected by `filter` are
    /// retained so a later call can still return them. Use the same filter with [`Self::read`] if
    /// the follow-up read should consume the event that made this method return `true`.
    pub fn poll<F>(&self, timeout: Option<Duration>, filter: F) -> io::Result<bool>
    where
        F: FnMut(&Event) -> bool,
    {
        let (mut reader, timeout) = if let Some(timeout) = timeout {
            let poll_timeout = PollTimeout::new(Some(timeout));
            if let Some(reader) = self.shared.try_lock_for(timeout) {
                (reader, poll_timeout.leftover())
            } else {
                return Ok(false);
            }
        } else {
            (self.shared.lock(), None)
        };
        reader.poll(timeout, filter)
    }

    /// Blocks until an event matching `filter` is available.
    ///
    /// Events rejected by `filter` are retained for later reads. For keyboard shortcuts, filter on
    /// `Event::Key(key) if key.kind == KeyEventKind::Press` unless the application intentionally
    /// handles release or repeat events.
    pub fn read<F>(&self, filter: F) -> io::Result<Event>
    where
        F: FnMut(&Event) -> bool,
    {
        let mut reader = self.shared.lock();
        reader.read(filter)
    }
}

#[derive(Debug)]
struct Shared {
    events: VecDeque<Event>,
    source: PlatformEventSource,
    skipped_events: Vec<Event>,
}

impl Shared {
    fn poll<F>(&mut self, timeout: Option<Duration>, mut filter: F) -> io::Result<bool>
    where
        F: FnMut(&Event) -> bool,
    {
        if self.events.iter().any(&mut (filter)) {
            return Ok(true);
        }

        let timeout = PollTimeout::new(timeout);

        loop {
            let maybe_event = match self.source.try_read(timeout.leftover()) {
                Ok(None) => None,
                Ok(Some(event)) => {
                    if (filter)(&event) {
                        Some(event)
                    } else {
                        self.skipped_events.push(event);
                        None
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => return Ok(false),
                Err(err) => return Err(err),
            };

            if timeout.elapsed() || maybe_event.is_some() {
                self.events.extend(self.skipped_events.drain(..));

                if let Some(event) = maybe_event {
                    self.events.push_front(event);
                    return Ok(true);
                }

                return Ok(false);
            }
        }
    }

    fn read<F>(&mut self, mut filter: F) -> io::Result<Event>
    where
        F: FnMut(&Event) -> bool,
    {
        let mut skipped_events = VecDeque::new();

        loop {
            while let Some(event) = self.events.pop_front() {
                if (filter)(&event) {
                    self.events.extend(skipped_events.drain(..));
                    return Ok(event);
                } else {
                    skipped_events.push_back(event);
                }
            }
            let _ = self.poll(None, &mut filter)?;
        }
    }
}
