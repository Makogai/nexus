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

What to test, how to write reliable tests, how to handle test data and failing tests, and what to run before reporting work as done. Apply it whenever you change behaviour, fix a bug or touch tests.

## What to test

Tests are the executable specification that lets the code change safely.

#### Ship tests with every behaviour change
<!-- rule: id=TST-1 enforcement=rule -->
A change that adds or alters behaviour includes tests that fail without it and pass with it, in the same change. Pure refactors keep existing tests green with unchanged assertions. Changes without testable behaviour (docs, comments, formatting, copy) need no new tests; if a behaviour is not testable as designed, say so and why.

#### Reproduce a bug with a failing test before fixing it
<!-- rule: id=TST-2 enforcement=rule -->
For a bug fix, first write a test that reproduces the failure and confirm it fails for the right reason, then fix the code and watch it pass; keep it as a regression guard named after the defect. If no automated test can reproduce it, record the manual reproduction steps instead.

#### Test each behaviour at the lowest level that can catch it
<!-- rule: id=TST-3 enforcement=rule -->
Keep most tests fast unit tests of logic, fewer integration tests of real boundaries (database, HTTP, file system) and few end-to-end tests. In applications, cover each critical user journey (e.g. sign-up, checkout, the main create-edit-save flow) with at least one stable end-to-end test against a production-like build.

#### Let risk, not a percentage, guide coverage
<!-- rule: id=TST-4 enforcement=guideline -->
Do not chase a coverage number. Test most thoroughly where failure is costly or likely: money, permissions, data migrations, parsing, concurrency and recently buggy code. Use coverage reports to find untested risky branches, never as a target met with assertion-free tests.

## Writing reliable tests

A test is only useful if its result can be trusted and its failure explains itself.

#### Make tests deterministic
<!-- rule: id=TST-5 enforcement=rule -->
Control time, randomness and the network: inject a clock or use fake timers, seed random generators, and replace external services with local fakes or recorded responses. Never sleep to wait for asynchronous work; wait for an explicit condition with a timeout.

#### Keep tests isolated and order-independent
<!-- rule: id=TST-6 enforcement=rule -->
Each test creates the state it needs and cleans it up (temporary directories, rolled-back transactions, fresh in-memory stores). Tests pass alone, in any order and in parallel, and never depend on another test's side effects or on shared mutable globals.

#### Name tests after behaviour and keep Arrange-Act-Assert visible
<!-- rule: id=TST-7 enforcement=guideline -->
Arrange inputs, act once and assert the outcome, with the three parts visibly separated. Name each test after the behaviour and condition, e.g. `rejects_expired_token` or `returns 404 when the order does not exist`, so a failure report explains itself without opening the test.

#### Assert observable behaviour, not implementation details
<!-- rule: id=TST-8 enforcement=rule -->
Test through the public interface and assert results, state changes or effects a caller can observe. Do not assert private calls, internal call order or exact log text unless that is the contract; such tests break on harmless refactors and still miss real regressions.

#### Mock only at boundaries you own
<!-- rule: id=TST-9 enforcement=rule -->
Replace slow or non-deterministic dependencies at your own seams (repository interface, HTTP client wrapper, clock). Do not mock third-party internals or the code under test; test your adapters around third-party APIs against a fake server or a real instance in a container.

#### Build test data with factories and fixtures
<!-- rule: id=TST-10 enforcement=guideline -->
Create test objects with factories or builders that supply valid defaults and let each test override only the fields it cares about. Keep shared fixtures small and immutable; avoid large shared datasets whose relevance to a given test is invisible.

## Test data and suite integrity

The suite must stay safe to share, and its results must mean what they say.

#### Never use production personal data in tests
<!-- rule: id=TST-11 enforcement=mandatory -->
Never copy production databases, logs or exports containing personal data into tests, fixtures, snapshots or seed files. Use synthetic data from factories, or data irreversibly anonymised by an approved process, with obviously fake identifiers such as addresses at example.com.

#### Never weaken a test to get a green run
<!-- rule: id=TST-12 enforcement=mandatory -->
Fix the cause of a failing or flaky test. Never delete or skip it, loosen its assertions, change expected values or add blind retries just to make it pass; change a test only when the specified behaviour changed, and say so. Quarantine a flaky test (skip with a reason and a tracking task) only with the person's agreement.

## Verification

Claims about tests are only worth something when they are exact.

#### Run the relevant tests before reporting work as done
<!-- rule: id=TST-13 enforcement=rule check=test -->
Before reporting work as done, run the tests covering the change, and the full suite when shared code changed, then report the exact commands with passed, failed and skipped counts. If they cannot run here, say why and what remains unverified (CORE-1).
