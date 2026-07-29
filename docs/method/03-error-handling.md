# Error handling — the decision, so it reads as one

Specola has no `thiserror`, no `anyhow`, and one domain error type. That is a
choice for an app this size, and writing it down is the difference between a
choice and an oversight.

## What we do

**Ingestion gets a real error type.** `io::record::IngestError` has three variants,
because "the pipe was truncated", "the hook payload changed shape" and "a hook kind
we don't handle" are three different operational problems. Collapsing them into
`None` meant an operator reading a log could not tell which had happened. Three
variants, a hand-written `Display`, about twenty lines, no dependency.

**Everything else keeps `Option` and `io::Error`.** A missing config file, an
unreadable transcript, a session with no cwd — the caller's response to all of them
is the same: fall back and carry on. An enum that only ever routes to one branch is
ceremony.

**Where a failure is not actionable, it is ignored on purpose**, with the reason
stated once per file. `tray.rs` is the clearest case: every ignored result there is a
window-server call. What is *not* allowed is ignoring a persistence error — that one
was a real defect, and it is the reason this pass happened.

## Why not `thiserror`

It would save about ten lines of the twenty above. The audit that prompted this
warned against sprinkling `anyhow` through the codebase — reaching for `thiserror` at
this size is the same trade in a smaller coat. If a second error type earns its place,
that arithmetic changes and so should this document.

## The line to hold

Add a variant when a caller would *branch* on it. Not when it would merely read
nicer in a log — that is what `Display` is for.
