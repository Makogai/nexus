---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Testing
status: active
applies: on-demand
prefix: TST
created: {{now}}
updated: {{now}}
source: import
---

What to test, how to write reliable tests, how to handle test data, and how to report verification honestly. Apply it whenever you change behaviour, fix a bug or claim that work is done.

## What to test

Tests are the executable specification that lets the code change safely.

#### Ship tests with every behaviour change
<!-- rule: id=TST-1 enforcement=rule check=test -->
Every change that adds or alters behaviour includes tests that fail without the change and pass with it, in the same commit or pull request. Pure refactors keep existing tests green without changing their assertions. If a behaviour is not testable as designed, say so and why.

#### Reproduce a bug with a failing test before fixing it
<!-- rule: id=TST-2 enforcement=rule -->
For a bug fix, first write a test that reproduces the reported failure and confirm it fails for the right reason, then fix the code and watch it pass. Keep the test as a permanent regression guard named after the defective behaviour.

#### Keep the test pyramid balanced
<!-- rule: id=TST-3 enforcement=guideline -->
Most tests are fast unit tests of logic, fewer are integration tests of real boundaries (database, HTTP, file system), and a small number are end-to-end tests. Test each behaviour at the lowest level that can catch its failure; move a test up a level only when a lower one cannot.

#### Cover critical user journeys end to end
<!-- rule: id=TST-4 enforcement=rule -->
Cover each critical journey (e.g. sign-up, login, checkout, the main create-edit-save flow) with at least one end-to-end test against a production-like build, e.g. with Playwright. Keep these tests few, stable and order-independent; they prove the parts work together, not every edge case.

#### Let risk, not a percentage, guide coverage
<!-- rule: id=TST-5 enforcement=guideline -->
Do not chase a coverage number. Test most thoroughly where failure is costly or likely: money, permissions, data migrations, parsing, concurrency and recently buggy code. Use coverage reports to find untested risky branches, not as a target to meet with assertion-free tests.

## Writing reliable tests

A test is only useful if its result can be trusted and its failure explains itself.

#### Make tests deterministic
<!-- rule: id=TST-6 enforcement=rule -->
Control time, randomness and the network: inject a clock or use fake timers, seed random generators, and replace external services with local fakes or recorded responses. Never sleep to wait for asynchronous work; wait for an explicit condition with a timeout.

#### Keep tests isolated and order-independent
<!-- rule: id=TST-7 enforcement=rule -->
Each test creates the state it needs and cleans it up (temporary directories, rolled-back transactions, fresh in-memory stores). Tests pass alone, in any order and in parallel; never depend on another test's side effects or on shared mutable globals.

#### Structure tests as Arrange-Act-Assert with descriptive names
<!-- rule: id=TST-8 enforcement=rule -->
Arrange inputs, act once, assert the outcome, with the three parts visibly separated. Name each test after the behaviour and condition, e.g. `rejects_expired_token` or `returns 404 when the order does not exist`, so a failure report explains itself without opening the test.

#### Assert observable behaviour, not implementation details
<!-- rule: id=TST-9 enforcement=rule -->
Test through the public interface and assert results, state changes or effects a caller can observe. Do not assert private calls, internal call order or exact log text unless that is the contract; such tests break on harmless refactors and still miss real regressions.

#### Mock only at boundaries you own
<!-- rule: id=TST-10 enforcement=rule -->
Replace slow or non-deterministic dependencies at your own seams (repository interface, HTTP client wrapper, clock). Do not mock third-party internals or the code under test; test your adapter around a third-party API with integration tests against a fake server or a real instance in a container.

#### Build test data with factories and fixtures
<!-- rule: id=TST-11 enforcement=guideline -->
Create test objects with factories or builders that supply valid defaults and let each test override only the fields it cares about. Keep shared fixtures small and immutable; avoid large shared datasets whose relevance to a given test is invisible.

## Test data and suite health

The suite must stay safe to share and trustworthy to run.

#### Never use production personal data in tests
<!-- rule: id=TST-12 enforcement=mandatory -->
Never copy production databases, logs or exports containing personal data into tests, fixtures, snapshots or seed files. Use synthetic data from factories, or data irreversibly anonymised by an approved process, with obviously fake identifiers such as addresses at example.com.

#### Fix or quarantine flaky tests immediately
<!-- rule: id=TST-13 enforcement=rule -->
When a test fails intermittently, fix the cause in the same change if it is small. Otherwise quarantine it (skip or tag it with a reason) and create a task to fix it; never add blind retries until it passes and never delete it silently. A quarantined test without a task is not allowed.

## Verification and honesty

Claims about tests are only worth something when they are exact.

#### Run the relevant tests before declaring work done and report exactly
<!-- rule: id=TST-14 enforcement=mandatory -->
Before reporting work as done, run the tests covering the change, and the full suite when shared code changed. Report the exact commands and which tests passed, failed or were skipped. Never claim tests pass that were not run in this session; if they could not be run, say so and why.
