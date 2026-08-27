#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::time::{Duration, Instant};

#[cfg(unix)]
pub(crate) use unix::{UnixEventSource, UnixWaker};
#[cfg(windows)]
pub(crate) use windows::{WindowsEventSource, WindowsWaker};

#[cfg(unix)]
pub(crate) type PlatformEventSource = UnixEventSource;
#[cfg(windows)]
pub(crate) type PlatformEventSource = WindowsEventSource;

#[cfg(unix)]
pub(crate) type PlatformWaker = UnixWaker;
#[cfg(windows)]
pub(crate) type PlatformWaker = WindowsWaker;

// CREDIT: <https://github.com/crossterm-rs/crossterm/blob/36d95b26a26e64b0f8c12edfe11f410a6d56a812/src/event/source.rs#L12-L27>
pub(crate) trait EventSource: Send + Sync {
    fn try_read(&mut self, timeout: Option<Duration>) -> std::io::Result<Option<crate::Event>>;

    fn waker(&self) -> PlatformWaker;
}

// CREDIT: <https://github.com/crossterm-rs/crossterm/blob/36d95b26a26e64b0f8c12edfe11f410a6d56a812/src/event/timeout.rs#L5-L40>
#[derive(Debug, Clone)]
pub(crate) struct PollTimeout {
    timeout: Option<Duration>,
    start: Instant,
}

impl PollTimeout {
    pub fn new(timeout: Option<Duration>) -> Self {
        Self {
            timeout,
            start: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> bool {
        self.timeout
            .map(|timeout| self.start.elapsed() >= timeout)
            .unwrap_or(false)
    }

    pub fn leftover(&self) -> Option<Duration> {
        self.timeout.map(|timeout| {
            let elapsed = self.start.elapsed();

            if elapsed >= timeout {
                Duration::ZERO
            } else {
                timeout - elapsed
            }
        })
    }
}
