# Kataba VCS: Independent Version-Control Module Design

> **Status:** design document / M0 spec  
> **Module:** `kataba-vcs` — standalone, independent VCS layer  
> **Audience:** crate maintainers, daemon integrators, frontend authors

## Executive Summary

`kataba-vcs` is a **zero-Kataba-dependency** version-control module: no `kataba-core`, no UI, no keymaps, no global state. It speaks git (and later jj) through a capability-gated, pure-domain API plus an optional async actor layer. A daemon exposes `kataba.vcs.*` RPC methods onto it; a CLI bin lets agents and scripts drive it directly; `kataba-tui` (or any other frontend) consumes the same crate directly.

The design is split into three layers:

1. **Pure domain** (`model/`, `parse/`, `patch.rs`, `diff/`) — sync, no I/O, `#![forbid(unsafe_code)]`, proptestable, fuzzable. Byte-exact parsing of git/jj output; patch generation you can hand to `git apply --check`.
2. **Drivers** (`git/`, `jj/`) — process management, gix reads with the Helix-style trust gate, CLI writes. Reads and writes are separated so any driver can be swapped or tested with fixtures.
3. **Actor shell** (`actor.rs`, `watcher.rs`, `cache.rs`) — optional tokio layer: mailbox, supersession, debounced refresh, snapshot generations, bounded events.

This is the same three-tier discipline as `kataba-core` (Tier 1 pure state machine, Tier 2 document actors, Tier 3 background workers) reused for VCS.

---

## Axioms

**V1 — Read-back verification.** Every mutation is followed by an authoritative re-read; the mutation's declared effect must be observed in the refreshed model, or the op fails loudly. We never trust mutation output as the source of truth.

**V2 — Trust gate.** Untrusted workspaces: reads yes (no repo-local config-driven process execution, gix `Trust::Reduced`); mutations refuse with `TrustRequired`. Reuse of Helix's proven discovery + forced-trust split (`helix-vcs/src/git.rs`).

**V3 — Monotonic snapshots.** Each repo carries `snapshot_gen: u64`; read commands carry/publish generations and superseded results are dropped at the actor boundary (kataba-lsp's stale-response suppression, applied to status loops).

**V4 — Bounded work.** Every CLI invocation gets a timeout and an output cap; every cache is byte-budgeted; no unbounded allocation on hostile input.

**V5 — Byte truth.** Git/jj paths are bytes, not strings. Everything is `bstr`/`&[u8]` until the presentation boundary. Filenames with newlines, quotes, and invalid UTF-8 are first-class.

**V6 — Serialized mutations.** Max one write in flight per repo; reads parallelize; `.git/index.lock` contention surfaces as `IndexLocked`, never auto-retried blindly.

---

## Scope

### In scope
- **Git reads:** discovery + forced trust, HEAD/branch resolution, status (`--porcelain=v2 -z` authoritative), branches, log pagination, base blobs (worktree-filtered), blame (`--porcelain`), imara-diff hunks vs base.
- **Git writes:** stage (file + hunk + partial via constructed patches → `git apply --cached`), unstage, discard/restore, commit/amend, branch create/switch/delete, stash.
- **jj reads:** change ID log, diff summary, op log, bookmark list — CLI, version-probed.
- **jj writes:** `describe`, `new`, `commit`, `squash`, `restore`, bookmark add/move/delete, `op undo`.
- **Patch engine:** unified-diff generation/parsing with exact mode bits and `\ No newline at end of file`.
- **Watcher:** git-dir event coalescing + debounce; stat fallback where fs events are unreliable.
- **Caches:** byte-budgeted blob cache keyed by OID.
- **Conformance suite:** fixture-repo harness vs canonical CLI output; patch `git apply --check` oracle; proptests on every parser.

### Out of scope (this crate)
- UI, keymaps, commit buffers, status-line rendering.
- Remotes/fetch/pull/push (separate `kataba-forge` crate later; `kataba-vcs` detects hosting hints only).
- Token storage / auth.
- Rebase / merge / cherry-pick logic reimplementation (git CLI owns this; we call it).

---

## Public API shape (M0–M3)

```rust
// A repo handle: discovery + trust decision
let repo = GitRepo::open(workspace_root, Trust::Reduced)?;

// Read-only ambient info
let status = repo.status()?;
let head = repo.head_name()?;
let branches = repo.branches()?;
let log = repo.log(Limit(50))?;
let diff_hunks = repo.gutter_diff(&file)?;

// Mutation — typed recipes, verified by re-read
repo.stage_file(&path)?;
repo.stage_hunks(&path, &[HunkRange { start, end }])?;
repo.unstage(&path)?;
repo.discard(&path)?;
repo.commit(&message)?;
repo.branch_create(&name)?;

// Capabilities gate
assert!(repo.capabilities().has_index);   // git: true, jj: false
```

Errors are `thiserror` typed (`RepoNotFound`, `TrustRequired`, `IndexLocked`, `NotSupported`, `Parse`, `Io`, `Timeout`, `GitCli`, `JjCli`), never `unwrap`/`panic` in library code.

---

## Dependency notes

- `gix 0.85` (Helix pin), features: `status`, `attributes`, `revision`, `max-performance`, `sha1`; `default-features=false`. In-process object DB reads only.
- `bstr 1` — byte strings for paths/diffs.
- `imara-diff 0.2` — same engine Helix uses for gutter diffs.
- `thiserror`, `smallvec` — workspace standards.
- `tokio` + `notify` — **actor feature only** (sync core stays dependency-light).
- No `git2`/libgit2, no `regex`, no shell-string spawning, no `anyhow` in library paths.

---

## Roadmap

| Milestone | Content | Gate |
|---|---|---|
| **M0** Foundations | model types, parsers, patch engine, exec hygiene, fixture harness, KATABA_VCS.md | proptests green; patch → `git apply --check` on randomized fixtures |
| **M1** Git read | discovery/trust, status, refs/log, base blobs, blame, imara diffs | conformance vs CLI on fixture matrix (weird paths, submodules, worktrees, detached HEAD) |
| **M2** Actor + watcher | mailbox, supersession, debounced refresh, caches, events | no unbounded growth under event storms; superseded results dropped |
| **M3** Git write | file/hunk/partial stage, commit/amend, switch/branch, discard/unstage | byte-exact stage/unstage round-trips vs `git status`; V1/V6 tests |
| **M4** jj backend | capability-gated reads/writes, colocated + non-colocated, min-version probe | colocated interop: same repo through both backends, consistent models |
| **M5** Remotes | fetch/pull/push + progress; credential-needed event | scripted remotes against local file remotes |
| **M6** Daemon | `kataba.vcs.*` RPC mapping; cargo-fuzz; CI matrix | perf budgets enforced |
