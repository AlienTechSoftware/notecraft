# Testing philosophy: the reasons and the decision paths

The rules themselves are in `SKILL.md` ("Testing Philosophy") and in `AGENTS.md`. This file shows how
they apply, with diagrams for the decisions a designer or tester makes.

## 1. Which kind of test does this need?

```mermaid
flowchart TD
    A[A behaviour to test] --> B{Does it touch an outside service?}
    B -- No --> C[Test the real code with test data]
    B -- Yes --> D{Is it the system clipboard?}
    D -- No --> E[Test the real code with test data. Do not mock the service.<br/>Ask: can the design avoid the outside call?]
    D -- Yes --> F[Clipboard exemption. Real clipboard, run alone.<br/>See section 3]
    C --> G{Does it share state?}
    E --> G
    F --> G
    G -- No --> H[Parallel test: owns its folder, notebooks and settings]
    G -- Yes --> I[Serial test: runs alone, and the gate says so]
```

## 2. What a test is made of

```mermaid
flowchart LR
    T[Test] --> D1[Test data<br/>sample notebook, fixture, recorded input]
    T --> F1[Its own temporary folder]
    T --> S1[Its own settings file]
    T --> R[The real code path<br/>no double in the middle]
    R --> O[Observed result<br/>control channel, pixels, files]
```

A test may read shared resources only through the rules in section 3. It may not reach the
environment, the working directory or a fixed folder.

## 3. The clipboard exemption, step by step

```mermaid
flowchart TD
    A[A test needs the clipboard] --> B{Does the app already reach<br/>the clipboard through its own boundary?}
    B -- Yes --> C[Test double for that boundary, in the test folder only]
    B -- No --> D[Use the real clipboard]
    D --> E[Run the test alone, in a serial binary]
    C --> F[No product hook is added for it]
    E --> F
```

The exemption never adds a hook to product code. Product code has no `cfg(test)` branch, no
test-only feature flag and no test-only setter.

## 4. Parallel or serial?

```mermaid
flowchart TD
    A[Test added or changed] --> B{Owns all its state?}
    B -- Yes --> C[Runs in parallel with the rest]
    B -- No --> D{Can the shared state be removed?}
    D -- Yes --> C
    D -- No --> E[Runs alone. Say which state it shares.]
    E --> F[The gate runs it in its own serial step]
```

## 5. A flaky test

```mermaid
flowchart TD
    A[A test fails at random] --> B[It is a failure. Do not re-run until green.]
    B --> C[Find the shared state it reads or writes]
    C --> D{Can the state be isolated?}
    D -- Yes --> E[Isolate it, then the test runs in parallel]
    D -- No --> F[Mark it as running alone, and say why]
    E --> G[Gate green, then push]
    F --> G
```

Never push past a failing gate. Never use `--no-verify`.

## Why no mocks

- A mock records what we believe a dependency does. When the real dependency behaves differently, the
  mock keeps passing and the bug reaches the user.
- A test double often forces test-only hooks into the product. Those hooks are then maintained with the
  product and read by its authors.
- Test data is part of the project. Anyone can read a sample notebook or a fixture file and see what
  the test covers.
