# 0003. One worker thread, no async runtime

**Status:** Accepted

## Context

A launch sequence blocks: 2 s after each app launch, 3 s after a Steam launch, up to 3 s while closing a process tree. v2.3.2 ran all of it on the Tk thread and forced a repaint per log line, so the window could not be used until the sequence ended. The work is a short, strictly ordered list of blocking Win32 calls and sleeps; nothing in it runs concurrently with anything else.

## Decision

- Launch and close sequences run on one worker thread (`worker::Worker`), started when the window opens.
- The controller sends a `Command` (the checked apps and the selected game, captured at the press) over an `mpsc` channel. The worker runs commands one at a time in the order sent and reports `Event`s over another channel, calling `egui::Context::request_repaint` after each. The UI drains the events at the start of each frame (`Controller::pump`).
- The managers, and with them the process tracker, live on the worker thread. The config is the one shared object (`SharedConfig`, a mutex).
- No async runtime.

## Consequences

- The window stays responsive. A button pressed mid-sequence queues its command; a button stays disabled while a command of its kind is queued or running.
- The sequence logic (`Sequencer`) is plain synchronous code. Tests run it inline (`worker::Inline`) for deterministic transcripts, or on the real thread (`tests/app_worker.rs`).
- A panic on the worker thread aborts the app (release builds use `panic = "abort"`); catching it is issue #5.
- Running sequences in parallel would need a different design; nothing asks for it.
