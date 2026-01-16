---
id: testing-patterns
name: Testing Patterns
description: >-
  Common testing patterns and practices. This skill is designed to be
  included in composite skills via the 'includes' feature.
tags: [testing, example]
---

# Testing Patterns

Foundational testing practices applicable across languages and frameworks.

## Rules

- Write tests for happy paths and error cases
- Test edge cases and boundary conditions
- Each test should test one thing
- Tests should be deterministic and repeatable
- Use descriptive test names that explain the scenario

## Checklist

- [ ] Happy path is covered
- [ ] Error cases are tested
- [ ] Edge cases are identified and tested
- [ ] Tests are independent and can run in any order
- [ ] Test data is isolated per test
- [ ] No flaky tests in the suite

## Pitfalls

- Testing implementation details instead of behavior
- Not testing error paths
- Shared mutable state between tests
- Tests that depend on execution order
