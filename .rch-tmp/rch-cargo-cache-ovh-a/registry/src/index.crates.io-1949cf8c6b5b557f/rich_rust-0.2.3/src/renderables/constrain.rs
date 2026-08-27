//! Constrain - limit the width of a renderable (Python Rich `rich.constrain` parity).
//!
//! Python Rich reference (`rich/constrain.py`):
//! - render: if width is None -> yield child; else render with `options.update_width(min(width, options.max_width))`
//! - measure: if width is not None -> `options.update_width(width)` then `Measurement.get(console, options, child)`

use crate::console::{Console, ConsoleOptions};
use crate::measure::{Measurement, RichMeasure};
use crate::renderables::Renderable;
use crate::segment::Segment;

trait RenderableMeasure: Renderable + RichMeasure {}
impl<T: Renderable + RichMeasure> RenderableMeasure for T {}

enum ConstrainChild {
    Renderable(Box<dyn Renderable>),
    Measurable(Box<dyn RenderableMeasure>),
}

impl ConstrainChild {
    fn as_renderable(&self) -> &dyn Renderable {
        match self {
            Self::Renderable(value) => &**value,
            Self::Measurable(value) => &**value,
        }
    }

    fn as_measurable(&self) -> Option<&dyn RichMeasure> {
        match self {
            Self::Renderable(_) => None,
            Self::Measurable(value) => Some(&**value),
        }
    }
}

/// Constrain the width of a renderable to a given number of characters.
pub struct Constrain {
    child: ConstrainChild,
    width: Option<usize>,
}

impl core::fmt::Debug for Constrain {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Constrain")
            .field("width", &self.width)
            .field(
                "child_measurable",
                &matches!(self.child, ConstrainChild::Measurable(_)),
            )
            .finish()
    }
}

impl Constrain {
    /// Create a `Constrain` wrapper around any renderable.
    ///
    /// If `width` is `None`, this is a pass-through wrapper (matches Python Rich behavior).
    #[must_use]
    pub fn new(renderable: impl Renderable + 'static, width: Option<usize>) -> Self {
        Self {
            child: ConstrainChild::Renderable(Box::new(renderable)),
            width,
        }
    }

    /// Create a `Constrain` wrapper around a boxed renderable.
    #[must_use]
    pub fn new_boxed(renderable: Box<dyn Renderable>, width: Option<usize>) -> Self {
        Self {
            child: ConstrainChild::Renderable(renderable),
            width,
        }
    }

    /// Create a `Constrain` wrapper around a renderable that supports measurement.
    #[must_use]
    pub fn new_measurable(
        renderable: impl Renderable + RichMeasure + 'static,
        width: Option<usize>,
    ) -> Self {
        Self {
            child: ConstrainChild::Measurable(Box::new(renderable)),
            width,
        }
    }

    /// Set / clear the constrain width.
    #[must_use]
    pub const fn width(mut self, width: Option<usize>) -> Self {
        self.width = width;
        self
    }
}

impl Renderable for Constrain {
    fn render<'a>(&'a self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        let Some(width) = self.width else {
            return self.child.as_renderable().render(console, options);
        };

        // Match Python: update_width(min(width, options.max_width)).
        let child_options = options.update_width(width.min(options.max_width));
        self.child.as_renderable().render(console, &child_options)
    }
}

impl RichMeasure for Constrain {
    fn rich_measure(&self, console: &Console, options: &ConsoleOptions) -> Measurement {
        let options = if let Some(width) = self.width {
            options.update_width(width)
        } else {
            options.clone()
        };
        Measurement::get(console, &options, self.child.as_measurable())
    }
}

#[cfg(test)]
mod tests {
    use crate::console::Console;
    use crate::renderables::{Renderable, Rule};

    use super::*;

    #[test]
    fn constrain_none_is_passthrough() {
        let console = Console::builder().width(30).build();
        let options = console.options();
        let rule = Rule::new().character("─").style(crate::style::Style::new());

        let constrained = Constrain::new(rule.clone(), None);
        assert_eq!(
            constrained.render(&console, &options),
            Renderable::render(&rule, &console, &options)
        );
    }

    #[test]
    fn constrain_limits_width_for_render() {
        let console = Console::builder().width(30).build();
        let options = console.options();
        let rule = Rule::new().character("─").style(crate::style::Style::new());

        let constrained = Constrain::new(rule, Some(10));
        let segments = constrained.render(&console, &options);
        let plain: String = segments
            .iter()
            .filter(|s| !s.is_control())
            .map(|s| s.text.as_ref())
            .collect();
        assert_eq!(plain, "──────────\n");
    }
}
