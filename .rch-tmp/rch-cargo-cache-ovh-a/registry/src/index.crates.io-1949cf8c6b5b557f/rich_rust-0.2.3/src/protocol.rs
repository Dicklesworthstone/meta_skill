//! Protocol-style extensibility (Python Rich `rich.protocol` parity).
//!
//! Python Rich uses duck typing for extensibility:
//! - `__rich__` for casting objects to renderables (`rich.protocol.rich_cast`)
//! - `__rich_console__` for rendering to segments
//! - `__rich_measure__` for width measurement
//!
//! Rust can't replicate Python's dynamic attribute checks, so we provide explicit traits and
//! helpers that map directly to those protocol hooks.

use std::any::Any;
use std::collections::HashSet;
use std::fmt;

use crate::renderables::Renderable;

/// Rust equivalent of Python Rich's `__rich__` hook.
///
/// In Python, `rich.protocol.rich_cast(obj)` repeatedly calls `obj.__rich__()` until the
/// returned object no longer has `__rich__` (or a loop is detected).
///
/// In Rust, this is explicit: implement [`RichCast`] and return a [`RichCastOutput`].
pub trait RichCast: Any + Send + Sync {
    /// Cast `self` to a renderable or string representation.
    fn rich_cast(&self) -> RichCastOutput;
}

/// A value that is both castable and directly renderable.
///
/// This enables loop-break behavior to still return something renderable, mirroring Python Rich
/// which returns the current object even if it still implements `__rich__`.
pub trait RichCastRenderable: RichCast + Renderable + Send + Sync {}

impl<T> RichCastRenderable for T where T: RichCast + Renderable + Send + Sync {}

/// A cast result, mirroring Python Rich's `RenderableType` outcomes from `rich_cast`.
pub enum RichCastOutput {
    /// A string renderable (will be rendered via the Console's string pipeline).
    Str(String),
    /// A concrete renderable value.
    Renderable(Box<dyn Renderable + Send + Sync>),
    /// A value that itself supports `RichCast` and should be cast recursively.
    ///
    /// This is the Rust analog of returning an object that still has `__rich__` in Python.
    Castable(Box<dyn RichCastRenderable>),
}

impl fmt::Debug for RichCastOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str(value) => f.debug_tuple("Str").field(value).finish(),
            Self::Renderable(_) => f.write_str("Renderable(<dyn Renderable>)"),
            Self::Castable(_) => f.write_str("Castable(<dyn RichCastRenderable>)"),
        }
    }
}

impl From<String> for RichCastOutput {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}

impl From<&str> for RichCastOutput {
    fn from(value: &str) -> Self {
        Self::Str(value.to_string())
    }
}

impl<T> From<Box<T>> for RichCastOutput
where
    T: Renderable + Send + Sync + 'static,
{
    fn from(value: Box<T>) -> Self {
        Self::Renderable(value)
    }
}

impl RichCastOutput {
    fn type_id_hint(&self) -> Option<std::any::TypeId> {
        match self {
            Self::Str(_) => Some(std::any::TypeId::of::<String>()),
            Self::Renderable(_) => None,
            Self::Castable(value) => Some((**value).type_id()),
        }
    }
}

/// Recursively cast via [`RichCast::rich_cast`], with a loop breaker.
///
/// This mirrors Python Rich's `rich.protocol.rich_cast`:
/// - repeatedly cast while the result remains castable
/// - track seen types to prevent infinite loops
#[must_use]
pub fn rich_cast(value: &dyn RichCast) -> RichCastOutput {
    let mut visited: HashSet<std::any::TypeId> = HashSet::new();

    let mut current = value.rich_cast();
    if let Some(tid) = current.type_id_hint() {
        visited.insert(tid);
    }

    while let RichCastOutput::Castable(next) = current {
        current = next.rich_cast();
        if let Some(tid) = current.type_id_hint() {
            if visited.contains(&tid) {
                break;
            }
            visited.insert(tid);
        }
    }

    current
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CastToString;

    impl RichCast for CastToString {
        fn rich_cast(&self) -> RichCastOutput {
            RichCastOutput::from("True")
        }
    }

    struct LoopA;
    struct LoopB;

    impl RichCast for LoopA {
        fn rich_cast(&self) -> RichCastOutput {
            RichCastOutput::Castable(Box::new(LoopB))
        }
    }

    impl RichCast for LoopB {
        fn rich_cast(&self) -> RichCastOutput {
            RichCastOutput::Castable(Box::new(LoopA))
        }
    }

    impl Renderable for LoopA {
        fn render<'a>(
            &'a self,
            _console: &crate::console::Console,
            _options: &crate::console::ConsoleOptions,
        ) -> Vec<crate::segment::Segment<'a>> {
            vec![crate::segment::Segment::plain("LoopA")]
        }
    }

    impl Renderable for LoopB {
        fn render<'a>(
            &'a self,
            _console: &crate::console::Console,
            _options: &crate::console::ConsoleOptions,
        ) -> Vec<crate::segment::Segment<'a>> {
            vec![crate::segment::Segment::plain("LoopB")]
        }
    }

    #[test]
    fn rich_cast_returns_string() {
        let out = rich_cast(&CastToString);
        match out {
            RichCastOutput::Str(value) => assert_eq!(value, "True"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn rich_cast_loop_breaks() {
        // We don't assert a specific output beyond termination; this is a safety check.
        let _ = rich_cast(&LoopA);
    }
}
