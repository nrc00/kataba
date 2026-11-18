# Kataba RPC: Unified Vision, Architecture, Feature Reference & Development Diary

**Module:** `kataba-rpc`
**Status:** M0 landed (2026-10-16) — 40 tests, conformance-verified against Neovim v0.12.6
**Reference implementation:** Neovim `v0.12.6`, `src/nvim/msgpack_rpc/*` (adapted; see §1.5)

---

## Table of Contents

- [Section 1: Executive Vision & Architectural Foundations](#section-1-executive-vision--architectural-foundations)
  - [1.1 Where `kataba-rpc` Sits](#11-where-kataba-rpc-sits)
  - [1.2 Why MessagePack-RPC (and Not gRPC / JSON-RPC / postcard)](#12-why-messagepack-rpc-and-not-grpc--json-rpc--postcard)
  - [1.3 Explicit Scope](#13-explicit-scope)
  - [1.4 Core Axioms (Set R)](#14-core-axioms-set-r)
  - [1.5 The Adaptation Map (Line-by-Line Provenance)](#15-the-adaptation-map-line-by-line-provenance)
  - [1.6 Documented Divergences from the Reference](#16-documented-divergences-from-the-reference)
  - [1.7 Wire Reference](#17-wire-reference)
  - [1.8 Measured Evidence](#18-measured-evidence)
  - [1.9 Roadmap (M0 → M3)](#19-roadmap-m0--m3)
- [Section 2: Detailed Architectural Layers & Module Audit](#section-2-detailed-architectural-layers--module-audit)
  - [2.1 Architectural Hierarchy](#21-architectural-hierarchy)
  - [2.2 `src/message.rs` — Wire Shapes & Strictness](#22-srcmessagers--wire-shapes--strictness)
  - [2.3 `src/connection.rs` — Threads, Call Frames, Close](#23-srcconnectionrs--threads-call-frames-close)
  - [2.4 `src/error.rs` — Codes & Error Shapes](#24-srcerrorrs--codes--error-shapes)
  - [2.5 `src/server.rs` — Listen & Broadcast](#25-srcserverrs--listen--broadcast)
  - [2.6 Code Quality & Safety Scorecard](#26-code-quality--safety-scorecard)
- [Section 3: Development Diary & Milestones](#section-3-development-diary--milestones)
  - [3.1 Session Log — M0 Landing (2026-10-16)](#31-session-log--m0-landing-2026-10-16)
  - [3.2 Findings Recorded During M0](#32-findings-recorded-during-m0)
  - [3.3 Current Test Verification Status (2026-10-16)](#33-current-test-verification-status-2026-10-16)
  - [3.4 Known Limitations & Next Steps](#34-known-limitations--next-steps)

---

# Section 1: Executive Vision & Architectural Foundations

## 1.1 Where `kataba-rpc` Sits

`kataba-rpc` is the **protocol layer**: it moves messages, nothing else. It has
**zero Kataba dependencies** and knows nothing about buffers, trees, or
windows. Every editor module (engine daemon, frontends, plugins, agents)
speaks through it; the method vocabulary is owned by the modules above it.

```
      kataba-tui        headless clients      plugins / agents
          │                    │                    │
          └────────────┬───────┴──────────┬─────────┘
                       │                  │
                 ┌─────▼──────────────────▼─────┐
                 │          kataba-rpc          │   ← this module
                 │  message · connection · server│
                 └─────▲──────────────────▲─────┘
                       │                  │
                 (future: kataba daemon exposing kataba-core,
                  kataba-treesitter, kataba-lsp, kataba-store)
```

The dependency rule is one-way: `kataba-rpc` never imports another Kataba
crate. A consumer that speaks the wire gets no special treatment over one
that connects from another process, another language, or another machine.

This supersedes the earlier "in-process façade" discussion: per the project
decision of 2026-10-16, engines and frontends are separated by the wire from
the start. The measured cost of that decision is in §1.8: a full call
round-trip is ~23 µs — 0.14% of a 60 fps frame budget — while the
architectural payoff (headless mode, multiple frontends, plugins, remote,
agent access) is structural rather than incremental.

## 1.2 Why MessagePack-RPC (and Not gRPC / JSON-RPC / postcard)

The reference is Neovim. It is the only editor in the field with a
decade-hardened internal RPC used by an entire plugin ecosystem (Python,
Node, Ruby, Rust, Go, Java, Haskell, …), and its protocol surface is small
enough to adapt faithfully. The core documentation (`KATABA_CORE.md` §4.5)
already committed to *"MessagePack-RPC over standard streams (stdin/stdout)
or Unix sockets"* — this module is that commitment made real.

| Option | Wire cost (measured, §1.8) | Deps | Verdict |
|---|---|---|---|
| **MessagePack-RPC (Neovim-style)** | ~23 µs/round-trip end-to-end | `rmpv` only | **chosen**: compact binary, self-delimiting, spec-level simple, cross-language clients exist, decade of editor provenance |
| JSON-RPC (LSP-style) | ~2–3× msgpack size for typical payloads | `serde_json` | fine, but text overhead for grid/redraw traffic and binary payloads are awkward |
| gRPC / tonic | HTTP/2 + protobuf runtime; typically 0.2–1 ms per in-machine unary call | very large tree | designed for service meshes; contradicts the core-doc plan; 50–250× the raw socket cost for zero editor benefit |
| postcard/bincode | same socket cost, codec ~50–100 ns | `serde` + codec | fastest wire format, but loses cross-language interop for the same socket cost (codec is ~0.05% of round-trip anyway) |

The decisive measurements (this machine, release build):

| Path | Cost |
|---|---|
| Engine edit delta in-process (`ChangeSet` build + OT apply + selection map, ~4 MB doc) | **84 ns** |
| Raw Unix socket round-trip (32 B) | **4.1 µs** |
| Full `kataba-rpc` call round-trip (encode → write thread → peer dispatch → reply → wake caller) | **~23 µs**, ~44k calls/s |
| 16 MiB echo (32 MiB total traffic) | **52.5 ms**, ~610 MiB/s |

Even the *slowest* path is microscopic against a 16.7 ms frame. The
architectural costs of RPC are in complexity, not latency — and this module
absorbs that complexity once, in ~1,200 lines with 40 tests.

## 1.3 Explicit Scope

### In-Scope (What `kataba-rpc` Owns)

- MessagePack-RPC wire shapes and strict decoding (`message.rs`).
- The connection state machine: call frames, dispatch order, close
  semantics (`connection.rs`).
- Transports: stdin/stdout and Unix domain sockets; listener helper
  (`server.rs`).
- Error values: reference-compatible codes plus local-only codes
  (`error.rs`).
- Broadcast delivery (encode once, deliver to many).
- Raw byte writes for producers that already have wire bytes.

### Out-of-Scope (Strict Non-Goals)

- **No API vocabulary.** Methods like `buffer.set_lines` are defined by the
  modules above (the future daemon); this crate routes whatever it is given.
- **No windows support yet.** Unix sockets first; named pipes are a
  follow-up.
- **No auth/TLS.** Local trust boundary for 0.1; documented in §3.4.
- **No UI redraw fast path.** Neovim's `redraw` message type 3 and packed
  grids are roadmap (§1.9), building on `write_raw`.

## 1.4 Core Axioms (Set R)

- **R1 — Protocol purity.** No Kataba dependencies, ever. The wire is the
  API; anything that can encode msgpack can participate.
- **R2 — Envelope fidelity.** Message shapes, strictness, and error
  encodings match the reference implementation byte-for-byte, proven by
  conformance vectors (§3.3) and live interop tests against `nvim` v0.12.6.
- **R3 — Self-delimiting transport.** MessagePack values are
  self-delimiting, so no extra framing layer exists (unlike LSP's
  `Content-Length`). A message stream is just concatenated msgpack values.
- **R4 — Response discipline.** Replies match requests by `msgid`. A reply
  to an unknown request id is a fatal protocol violation (reference
  semantics). Timeouts orphan locally — a late reply is dropped, never
  fatal (Kataba extension; the reference has no timeouts).
- **R5 — Order.** One dispatch thread per connection runs incoming requests
  and notifications in arrival order. A slow handler never blocks reads or
  reply delivery. Handlers may issue nested calls through the connection
  they arrived on.
- **R6 — Monotonic close.** A connection closes exactly once, with exactly
  one recorded reason. Pending calls fail with that reason; close handlers
  run exactly once; every later operation fails immediately.
- **R7 — Error pairs.** Errors are `(code, message)`. Codes `0`
  (exception) and `1` (validation) are the reference's wire codes. Negative
  codes exist only locally (`-1` closed, `-2` timeout) and never travel.

## 1.5 The Adaptation Map (Line-by-Line Provenance)

Everything in this crate was adapted from Neovim **v0.12.6** (the version
installed on this machine, used live in the interop tests). File paths are
relative to the Neovim source tree.

| Reference (Neovim v0.12.6) | Kataba construct | Notes |
|---|---|---|
| `msgpack_rpc/channel.c` `serialize_request` (~L548–565) | `Message::to_value` / `Request`+`Notification` arms | 4-element request with msgid; 3-element notification without; the reference selects the tag with `request_id ? 0 : 2` |
| `msgpack_rpc/channel.c` `serialize_response` (~L567–610) | `Message::Response` | error is `nil` or `[code, message]`; result is `nil` when errored |
| `msgpack_rpc/channel.c` `parse_msgpack` (~L245–304) | `Message::decode_next`, `reader_loop` | any decode/shape failure is fatal → close; request params **must** be an array → close |
| `msgpack_rpc/channel.c` `handle_request` / `request_event` (~L308–375) | `dispatch_loop`, `invoke` | unknown method → exception error; in-order execution; closed-channel early return |
| `msgpack_rpc/channel.c` `rpc_send_call` (~L140–215) | `call_async` + `Pending` map + `wait_timeout` | call-frame blocking replaced by per-request channels + timeouts |
| `msgpack_rpc/channel.c` unknown-response-id branch (~L260–270) | `reader_loop` → `CloseReason::ProtocolViolation` | "…unknown request id…; ensure the peer is properly synchronized" kept in spirit |
| `msgpack_rpc/channel.c` write-failure branch (~L411–421) | `writer_loop` → `CloseReason::WriteFailed` | |
| `msgpack_rpc/channel.c` `broadcast_event` (~L457–474) | `server::broadcast` | encode once, then deliver to every connection |
| `msgpack_rpc/channel.c` `rpc_close` (~L478–488) | `close()` | idempotent |
| `msgpack_rpc/channel.c` `chan_close_on_err` (~L530–546) | `close()` pending drain | every in-flight call gets the close reason |
| `msgpack_rpc/channel.c` notification-error branch (~L570–583) | `ERROR_EVENT` = `"kataba.error"` | reference emits `nvim_error_event`; same `[code, message]` argument shape |
| `msgpack_rpc/channel.c` `rpc_write_raw` (~L383) | `Connection::write_raw` | pre-encoded bytes skip re-encoding |
| `msgpack_rpc/packer.c` flush callback batching | `writer_loop` try-recv batching | one flush per batch, not per message |
| `msgpack_rpc/unpacker.h` (`unpacker_closed` on fatal parse) | `DecodeError` → close | stricter message shapes enforced in Rust |
| `api/private/defs.h` L31–35 (`kErrorType*`) | `error::codes` | `EXCEPTION=0`, `VALIDATION=1` |
| `api/private/defs.h` L37–44 (`kMessageType*`) | `message::message_type` | `REQUEST=0`, `RESPONSE=1`, `NOTIFICATION=2` |
| `msgpack_rpc/server.c` (`--listen`) | `Connection::connect_unix`, `Server::bind_unix` | |
| request-id allocation (`channel.c` L84/L152) | `next_msgid` | starts at 1; 0 reserved (notifications omit the field) |
| rmpv default (MAX_DEPTH 1024) | `message::MAX_DEPTH = 128` | hardening: no editor message needs 128 levels |

## 1.6 Documented Divergences from the Reference

1. **Reply timeouts (new).** The reference blocks forever in `rpc_send_call`.
   Kataba adds `call_timeout`, and a timed-out request is *orphaned*: a late
   reply is dropped silently instead of tripping the unknown-request-id
   violation. Without this, a single slow handler plus one impatient caller
   would kill the channel.
2. **Dedicated dispatch thread (improvement).** The reference distinguishes
   "fast" handlers (run inline on the read thread) from scheduled ones. Kataba
   always dispatches on one dedicated thread per connection: order is still
   guaranteed, but a slow handler cannot stall reads or reply delivery.
3. **Shutdown hook (improvement).** `close()` calls a transport-provided hook
   (`UnixStream::shutdown`) so a reader blocked on a silent peer unblocks
   immediately. The reference relies on its event loop for this.
4. **No redraw fast path yet.** `kMessageTypeRedrawEvent = 3` and Neovim's
   packed-grid streaming are roadmap; the `write_raw` API is the building
   block that makes them cheap when a UI needs them.
5. **No client-type/channel registry yet.** `nvim_set_client_info`, client
   types (`ui`, `plugin`, `host`, …), and per-channel broadcast registries
   are roadmap M1.
6. **Unix-first.** stdin/stdout and Unix sockets today; named pipes later.
7. **No message size cap yet.** rmpv reads incrementally with a depth cap;
   a top-level size cap is a hardening item (§3.4).

## 1.7 Wire Reference

Message shapes (MessagePack arrays):

```
Request:       [0, msgid(uint32 >= 1), method(string), params(array)]
Response:      [1, msgid, error, result]        error = nil | [code(int), message(str)]
Notification:  [2, method(string), params(array)]
```

Error codes:

| Code | Source | Meaning |
|---|---|---|
| `0` | reference (`kErrorTypeException`) | general error (including unknown methods: `Invalid method: <name>`) |
| `1` | reference (`kErrorTypeValidation`) | invalid arguments/values |
| `-1` | Kataba-local | connection closed / transport failure (never on the wire) |
| `-2` | Kataba-local | reply timeout (never on the wire) |

Decoding tolerance for the error field mirrors `rpc_send_call`: a bare
string is promoted to an exception; a `[0|1, string]` array keeps its code;
anything else becomes `"unknown error"`.

Close reasons (exactly one per connection): `PeerClosed`, `ClosedLocally`,
`WriteFailed(message)`, `ProtocolViolation(message)`.

Local error event: a failing **notification handler** produces a
`kataba.error` notification with `[code, message]` params (the reference's
`nvim_error_event` analogue).

## 1.8 Measured Evidence

All numbers: this machine, release build, 2026-10-16.

| Measurement | Result | How |
|---|---|---|
| `kataba-rpc` small call round-trip (in-process UDS pair, echo) | mean **22.8 µs**, p50 20.8 µs, p99 43.1 µs, ~44k calls/s | `cargo run --release --example bench` |
| Same, but 16 MiB payload echoed (32 MiB total) | **52.5 ms**, ~610 MiB/s | `--example bench` |
| Raw socket round-trip (no RPC machinery), 32 B | 4.1 µs | design-phase microbenchmark |
| In-process engine delta (`ChangeSet`+apply+map, 4 MB doc) | 84 ns | design-phase microbenchmark |

Interpretation: the RPC machinery adds ~19 µs over the raw socket
(thread hops, codec, pending-map bookkeeping) and remains ~0.14% of a
60 fps frame. Large payloads flow at hover-under-local-memory-speed through
a single encode/decode copy. Neither number is a reason to ever special-case
the hot path.

Live conformance evidence (nvim 0.12.6, `--headless --clean --listen`):

- `nvim_get_api_info` → `[channel_id, metadata]`, `api_level` 14.
- `nvim_eval "40 + 2"` → `42`.
- Unknown method → code `0`, message `Invalid method: nvim_not_a_method`
  (message text adopted verbatim).
- `vim.rpcnotify(0, 'kataba_ping', 42)` from Lua → our notification handler
  receives `42` (bidirectional proof).
- 128 pipelined `nvim_eval` calls → every reply matched by `msgid`.

## 1.9 Roadmap (M0 → M3)

- **M0 (landed 2026-10-16).** Protocol, connection state machine,
  stdio/UDS transports, broadcast, raw writes, conformance + interop tests,
  plus a reference stdio daemon (`src/bin/kataba-rpc-daemon.rs`) exercised by
  a child-process test.
- **M1 — Kataba API surface.** A daemon binary (working name `kataba`) that
  exposes `kataba.*` methods over `kataba-rpc`, backed by `kataba-core`
  (buffers, transactions, history) and later `kataba-treesitter` /
  `kataba-lsp` / `kataba-store`. Client info dict, client types, channel
  registry. This is what `kataba-tui` will talk to.
- **M2 — UI fast path.** Redraw-style streaming on `write_raw` (packed
  grids, batched flushes), so a frontend can render without per-cell RPC
  calls.
- **M3 — Hardening & reach.** Top-level message size caps, async (deferred)
  handlers, request cancellation, named pipes for Windows, optional TCP/TLS
  for remote sessions, optional mio-style reader for full event-loop parity.

---

# Section 2: Detailed Architectural Layers & Module Audit

## 2.1 Architectural Hierarchy

```
kataba-rpc
├── message.rs     wire shapes, strict decode, encode          (~"unpacker"+"serializer")
├── error.rs       RpcError, codes, error-field decode rules   (~"api_set_error")
├── connection.rs  Connection, PendingReply, Request, threads  (~"channel.c")
└── server.rs      Server (listen), broadcast                  (~"server.c")
```

Per-connection threads:

```
peer ──▶ reader thread ──▶ [dispatch queue] ──▶ dispatch thread ──▶ handler
             │                                                     │
             │ responses resolve Pending entries                   ▼
             └────────────▶ pending map ◀────── callers      writer queue
                                                                   │
peer ◀──────────────────────────────────────────── writer thread ◀─┘
```

## 2.2 `src/message.rs` — Wire Shapes & Strictness

Adapts the spec + Neovim's unpacker. `Message::decode_next` distinguishes a
**clean EOF at a message boundary** (`Ok(None)` — the peer hung up) from
every other failure, which is fatal for the connection. Shapes are enforced
as the reference does: request/notification params must be arrays; msgids
must fit in `u32`; unknown message-type tags are protocol violations. The
nesting depth cap is 128 (rmpv's default is 1024).

## 2.3 `src/connection.rs` — Threads, Call Frames, Close

- `call_async` allocates a msgid (starting at 1, wrapping), registers a
  `Pending::Waiter`, enqueues, and returns a `PendingReply`.
- `reader_loop` resolves waiters; unknown ids close the connection
  (reference semantics).
- `writer_loop` batches queued messages per flush (reference packer
  behavior).
- `dispatch_loop` runs handlers in arrival order; handler failures become
  error responses; failing notification handlers become `kataba.error`
  events.
- `close()` is idempotent, records one reason, fails pending waiters,
  unblocks the transport, and runs close handlers once.
- **Close races are real and intentional.** A peer that hangs up can be
  observed as read-side EOF *or* write-side `EPIPE`; whichever thread
  reaches `close()` first records the reason. The test suite synchronizes
  on the handler to pin the intended case.
- `join()` waits for the three threads; it must not be called from a
  handler.

## 2.4 `src/error.rs` — Codes & Error Shapes

Encodes/decodes the `[code, message]` pair; decodes with the reference's
tolerance rules (string → exception; unknown shapes → `"unknown error"`).
Negative codes are local-only by construction: only `0`/`1` are produced
for wire errors.

## 2.5 `src/server.rs` — Listen & Broadcast

`Server::bind_unix` + `accept()` mirror `--listen`; `broadcast` encodes one
notification once and hands the bytes to every connection via
`write_raw`, returning the delivered count.

## 2.6 Code Quality & Safety Scorecard

| Property | Status |
|---|---|
| `unsafe_code` | `forbid` — zero unsafe in the entire crate |
| `clippy::unwrap_used` / `expect_used` / `panic` / `todo` / `unimplemented` | denied in library code |
| Clippy | clean (`--all-targets`, 0 warnings) |
| Rustfmt | clean |
| Tests | 41 (13 unit · 7 protocol conformance · 14 connection · 5 nvim interop · 1 stdio child-process · 1 doctest) |
| Dependencies | `rmpv` (codec) + `thiserror` (errors) — no async runtime, no protobuf, no TLS stack |

---

# Section 3: Development Diary & Milestones

## 3.1 Session Log — M0 Landing (2026-10-16)

1. Boundary decision made: RPC-first (superseding the in-process façade
   option). Design evidence gathered first: engine delta 84 ns, raw socket
   4.1 µs, then the reference implementation was studied directly.
2. Reference acquisition: no local Neovim checkout existed, so the sources
   for the exact installed version (`nvim v0.12.6`) were fetched from
   `github.com/neovim/neovim` at tag `v0.12.6` (`msgpack_rpc/channel.c`,
   `channel_defs.h`, `packer.c`, `unpacker.h`, `server.c`,
   `api/private/defs.h`, plus the MessagePack-RPC specification from the
   `msgpack-rpc` project). Every adaptation is cited in §1.5.
3. `kataba-rpc` implemented as a standalone crate (workspace member added):
   `error.rs` → `message.rs` → `connection.rs` → `server.rs` → `lib.rs`,
   then unit tests, protocol vectors, connection integration tests, Neovim
   interop tests, and three examples (`echo`, `bench`, `nvim_probe`).
4. Verification: full workspace green (113 tests), clippy and fmt clean,
   live interop against `nvim --headless --listen` passed on first run.
5. One test race found and fixed during the session: peer-hangup
   close-reason flakiness (write-side `EPIPE` vs read-side EOF). The test
   now synchronizes on the handler having started, pinning the read-side
   EOF case deterministically.
6. One fidelity fix: unknown-method message text changed to the
   reference's exact wording, `Invalid method: <name>`.

## 3.2 Findings Recorded During M0

- **Notification arity is the trap.** Requests are 4-element arrays,
  notifications 3-element; the reference switches the tag on `request_id`
  presence. Conformance vectors pin both byte layouts.
- **Error encoding is a pair, not a struct.** Neovim's `[code, message]`
  convention (with string-promotion tolerance) is subtly different from
  JSON-RPC's error object; both directions are now covered by tests.
- **Desync is fatal, timeouts are not.** The reference kills the channel on
  an unknown response id because it is unrecoverable; timeouts, which the
  reference doesn't have, must *not* be treated the same way.
- **Batching matters more than per-message speed.** Draining the write
  queue per flush (reference packer behavior) collapses N messages into one
  syscall burst, which is what makes pipelined calls cheap.
- **Live interop is the strongest test.** 128 pipelined calls plus a
  Lua-issued `rpcnotify` back into our handler proved both directions
  against the real implementation, not just our own encoder talking to our
  own decoder.
- **Stdout is part of the wire.** The first stdio test used the test binary
  itself as the child; the harness's own stdout ("running 1 test…") is
  indistinguishable from protocol bytes and killed the channel instantly.
  A stdio daemon must own stdout byte-clean — the same constraint Neovim's
  `--embed` and every stdio LSP server live under. The test now drives a
  dedicated daemon binary.

## 3.3 Current Test Verification Status (2026-10-16)

| Suite | Count | Status |
|---|---|---|
| `kataba-rpc` unit (codec, error shapes, server, doctest) | 14 | ✅ |
| `kataba-rpc` protocol conformance vectors | 7 | ✅ |
| `kataba-rpc` connection integration | 14 | ✅ |
| `kataba-rpc` Neovim v0.12.6 interop | 5 | ✅ |
| `kataba-rpc` stdio child-process transport | 1 | ✅ |
| Workspace total (core 49 · lsp 10 · treesitter 14 · rpc 41) | 114 | ✅ |
| Clippy `--all-targets` · Rustfmt | — | ✅ clean |

## 3.4 Known Limitations & Next Steps

- **Next milestone: the daemon.** `kataba-rpc` is transport; the `kataba.*`
  method vocabulary, backed by `kataba-core`, is the companion deliverable
  (roadmap M1). `kataba-tui` (the frontend; earlier placeholder name
  `kataba-term`) becomes the daemon's first client, and its rendering path
  will drive the M2 redraw fast path.
- **No top-level message size cap.** A hostile peer can make us allocate a
  laptop's worth of `Value`. Depth is capped (128), size is not yet.
- **Blocking reader.** The reader thread parks in `read(2)`; transports
  without a shutdown hook cannot be interrupted locally. Acceptable for
  stdio/UDS; revisit with mio/event-loop parity (M3).
- **No async handlers.** Handlers are synchronous closures; a handler that
  needs to await external work (LSP, store) currently blocks its
  connection's dispatch thread. Deferred-reply support is M3.
- **Unix-only.** Windows named pipes and remote TCP/TLS are M3; auth is
  explicitly out of scope for 0.1 (local trust boundary).
- **`PendingReply` leak shape.** Dropping a `PendingReply` without waiting
  leaves its map entry until the reply (or close) arrives; a peer that
  never replies to an abandoned request retains one map slot. Bounded by
  peer behavior; revisit with M3 cancellation.
