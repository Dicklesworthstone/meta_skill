//! Rule rendering conformance tests.

use super::TestCase;
use rich_rust::renderables::rule::Rule;
use rich_rust::segment::Segment;
use rich_rust::text::JustifyMethod;

/// Test case for basic rule rendering.
#[derive(Debug)]
pub struct RuleTest {
    pub name: &'static str,
    pub title: Option<&'static str>,
    pub character: Option<&'static str>,
    pub align: Option<JustifyMethod>,
    pub width: usize,
}

impl TestCase for RuleTest {
    fn name(&self) -> &str {
        self.name
    }

    fn render(&self) -> Vec<Segment<'static>> {
        let mut rule = match self.title {
            Some(title) => Rule::with_title(title),
            None => Rule::new(),
        };
        if let Some(ch) = self.character {
            rule = rule.character(ch);
        }
        if let Some(align) = self.align {
            rule = rule.align(align);
        }
        rule.render(self.width)
    }

    fn python_rich_code(&self) -> Option<String> {
        let title_arg = match self.title {
            Some(t) => format!("\"{}\"", t),
            None => String::from(""),
        };
        let char_arg = match self.character {
            Some(c) => format!(", characters=\"{}\"", c),
            None => String::new(),
        };
        let align_arg = match self.align {
            Some(JustifyMethod::Left) => ", align=\"left\"",
            Some(JustifyMethod::Right) => ", align=\"right\"",
            Some(JustifyMethod::Center) => ", align=\"center\"",
            _ => "",
        };
        Some(format!(
            r#"from rich.console import Console
from rich.rule import Rule

console = Console(force_terminal=True, width={})
rule = Rule({}{}{})
console.print(rule, end="")"#,
            self.width, title_arg, char_arg, align_arg
        ))
    }
}

/// Standard rule test cases for conformance testing.
pub fn standard_rule_tests() -> Vec<Box<dyn TestCase>> {
    vec![
        Box::new(RuleTest {
            name: "rule_no_title",
            title: None,
            character: None,
            align: None,
            width: 40,
        }),
        Box::new(RuleTest {
            name: "rule_with_title",
            title: Some("Section"),
            character: None,
            align: None,
            width: 40,
        }),
        Box::new(RuleTest {
            name: "rule_left_align",
            title: Some("Left"),
            character: None,
            align: Some(JustifyMethod::Left),
            width: 40,
        }),
        Box::new(RuleTest {
            name: "rule_right_align",
            title: Some("Right"),
            character: None,
            align: Some(JustifyMethod::Right),
            width: 40,
        }),
        Box::new(RuleTest {
            name: "rule_custom_char",
            title: None,
            character: Some("="),
            align: None,
            width: 40,
        }),
        Box::new(RuleTest {
            name: "rule_ascii",
            title: Some("ASCII"),
            character: Some("-"),
            align: None,
            width: 40,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::run_test;

    #[test]
    fn test_rule_no_title() {
        let test = RuleTest {
            name: "rule_no_title",
            title: None,
            character: None,
            align: None,
            width: 40,
        };
        let output = run_test(&test);
        // Rule should fill the width with rule characters
        assert!(
            output.contains('─') || output.contains('-'),
            "Rule should render horizontal line characters"
        );
    }

    #[test]
    fn test_rule_with_title() {
        let test = RuleTest {
            name: "rule_with_title",
            title: Some("Test"),
            character: None,
            align: None,
            width: 40,
        };
        let output = run_test(&test);
        assert!(output.contains("Test"));
    }

    #[test]
    fn test_all_standard_rule_tests() {
        for test in standard_rule_tests() {
            let output = run_test(test.as_ref());
            assert!(
                !output.is_empty(),
                "Test '{}' produced empty output",
                test.name()
            );
        }
    }
}
