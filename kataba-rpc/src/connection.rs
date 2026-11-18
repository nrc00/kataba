//! Connections: transports, threads, dispatch, and the call API.
//!
//! Adapted from Neovim's `src/nvim/msgpack_rpc/channel.c` (v0.12.6):
//!
//! | Neovim construct | Kataba construct |
//! |---|---|
//! | `receive_msgpack` / `parse_msgpack` (read loop, fatal-error close) | [`reader_loop`] |
//! | `handle_request` / `request_event` (dispatch, in-order execution) | [`dispatch_loop`] |
//! | `rpc_send_call` + `ChannelCallFrame` + call stack (blocking call) | [`Connection::call`], `PendingReply` |
//! | `serialize_request` (padding the shared output queue) | [`enqueue`] + [`writer_loop`] |
//! | `chan_close_on_err` / `rpc_close` (fail pending calls, idempotent close) | [`close`] |
//! | `serialize_response` notification-error branch | [`ERROR_EVENT`] notification |
//!
//! Documented divergences from the reference:
//!
//! * Requests and notifications are always dispatched on one dedicated thread
//!   per connection (Neovim distinguishes "fast" handlers that run inline from
//!   scheduled ones). Order is preserved either way, but a slow handler never
//!   blocks reading or replies to other requests.
//! * Calls can time out. A timed-out request's entry is marked orphaned so a
//!   late reply is dropped silently instead of closing the connection as an
//!   unknown-request-id violation.
//! * `close` invokes a transport-provided shutdown hook so a blocked read
//!   unblocks without waiting for the peer (Neovim relies on its event loop).

use std::collections::HashMap;
use std::fmt;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

use rmpv::Value;

use crate::error::RpcError;
use crate::message::Message;

/// Event name for errors raised by notification handlers.
///
/// Adapted from Neovim's `nvim_error_event` (`serialize_response`: when a
/// notification handler fails, the error is emitted as an event because a
/// notification has no reply to carry it). The argument shape is the same
/// two-element `[code, message]` pair used by error responses.
pub const ERROR_EVENT: &str = "kataba.error";

/// A call handed to registered handlers.
#[derive(Clone)]
pub struct Request {
    /// The method name.
    pub method: String,
    /// Positional arguments.
    pub params: Vec<Value>,
    /// The connection the request arrived on; handlers may use it to call
    /// back (Neovim handlers receive the channel id and do exactly this).
    pub connection: Connection,
}

impl Request {
    /// Consumes the request, returning its parts.
    #[must_use]
    pub fn into_parts(self) -> (String, Vec<Value>, Connection) {
        (self.method, self.params, self.connection)
    }
}

/// Why a connection stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseReason {
    /// The peer closed the transport (clean EOF at a message boundary).
    PeerClosed,
    /// [`Connection::close`] was called locally.
    ClosedLocally,
    /// Writing to the peer failed; the connection is abandoned.
    WriteFailed(String),
    /// The peer violated the protocol (garbage bytes, wrong shape, unknown
    /// request id in a response). Mirrors Neovim closing the channel on
    /// unpack errors and desynchronized responses.
    ProtocolViolation(String),
}

impl fmt::Display for CloseReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeerClosed => write!(formatter, "closed by the peer"),
            Self::ClosedLocally => write!(formatter, "closed locally"),
            Self::WriteFailed(message) => write!(formatter, "stream write failed: {message}"),
            Self::ProtocolViolation(message) => write!(formatter, "protocol violation: {message}"),
        }
    }
}

type RequestHandler = Arc<dyn Fn(Request) -> Result<Value, RpcError> + Send + Sync>;
type NotificationHandler = Arc<dyn Fn(Request) -> Result<(), RpcError> + Send + Sync>;
type CloseHandler = Box<dyn Fn(CloseReason) + Send + Sync>;

enum Pending {
    /// A caller is blocked (or will be) on the reply.
    Waiter(Sender<Result<Value, RpcError>>),
    /// The caller timed out; a late reply must be dropped silently. This is a
    /// Kataba extension — Neovim calls block forever, so the case cannot arise
    /// there.
    Orphaned,
}

struct Inner {
    next_msgid: AtomicU32,
    pending: Mutex<HashMap<u32, Pending>>,
    requests: RwLock<HashMap<String, RequestHandler>>,
    notifications: RwLock<HashMap<String, NotificationHandler>>,
    writer: Mutex<Option<Sender<Vec<u8>>>>,
    dispatch: Mutex<Option<Sender<Message>>>,
    closed: AtomicBool,
    close_reason: Mutex<Option<CloseReason>>,
    close_handlers: Mutex<Vec<CloseHandler>>,
    shutdown: Option<Box<dyn Fn() + Send + Sync>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

/// A msgpack-RPC endpoint.
///
/// Both sides are symmetric: a `Connection` can send requests and
/// notifications and can answer incoming ones. Cloning is cheap (shared
/// handle). Handlers may call back through the `connection` field of the
/// [`Request`] they receive.
#[derive(Clone)]
pub struct Connection {
    inner: Arc<Inner>,
}

impl Connection {
    /// Wraps a reader/writer pair (Neovim: `kChannelStreamInternal`).
    ///
    /// The reader must be blocking. Without a transport shutdown hook,
    /// [`Connection::close`] cannot interrupt a pending read, so a reader
    /// blocked on a silent peer keeps its thread parked until the peer hangs
    /// up (see [`Connection::from_unix_stream`] for a transport that closes
    /// cleanly).
    pub fn from_stream<R, W>(reader: R, writer: W) -> io::Result<Self>
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        Self::start(reader, writer, None)
    }

    /// Uses stdin/stdout as the transport (Neovim: `kChannelStreamStdio`).
    ///
    /// This is how a headless daemon talks to a frontend that spawned it.
    pub fn from_stdio() -> io::Result<Self> {
        Self::start(io::stdin(), io::stdout(), None)
    }

    /// Wraps one end of a Unix domain socket.
    ///
    /// The socket is split into read/write halves and a shutdown hook is
    /// installed so [`Connection::close`] unblocks the reader immediately.
    #[cfg(unix)]
    pub fn from_unix_stream(stream: UnixStream) -> io::Result<Self> {
        let reader = stream.try_clone()?;
        let writer = stream.try_clone()?;
        let shutdown = Box::new(move || {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        });
        Self::start(reader, writer, Some(shutdown))
    }

    /// Connects to a Unix domain socket path (Neovim: `--listen`).
    #[cfg(unix)]
    pub fn connect_unix(path: &Path) -> io::Result<Self> {
        Self::from_unix_stream(UnixStream::connect(path)?)
    }

    fn start<R, W>(
        reader: R,
        writer: W,
        shutdown: Option<Box<dyn Fn() + Send + Sync>>,
    ) -> io::Result<Self>
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let inner = Arc::new(Inner {
            next_msgid: AtomicU32::new(1),
            pending: Mutex::new(HashMap::new()),
            requests: RwLock::new(HashMap::new()),
            notifications: RwLock::new(HashMap::new()),
            writer: Mutex::new(None),
            dispatch: Mutex::new(None),
            closed: AtomicBool::new(false),
            close_reason: Mutex::new(None),
            close_handlers: Mutex::new(Vec::new()),
            shutdown,
            threads: Mutex::new(Vec::new()),
        });

        let (writer_sender, writer_receiver) = mpsc::channel();
        let (dispatch_sender, dispatch_receiver) = mpsc::channel();
        *lock(&inner.writer) = Some(writer_sender);
        *lock(&inner.dispatch) = Some(dispatch_sender);

        let writer_handle = thread::Builder::new()
            .name("kataba-rpc-writer".to_string())
            .spawn({
                let inner = Arc::clone(&inner);
                move || writer_loop(inner, writer, &writer_receiver)
            })?;
        let reader_handle = thread::Builder::new()
            .name("kataba-rpc-reader".to_string())
            .spawn({
                let inner = Arc::clone(&inner);
                move || reader_loop(inner, reader)
            })?;
        let dispatch_handle = thread::Builder::new()
            .name("kataba-rpc-dispatch".to_string())
            .spawn({
                let inner = Arc::clone(&inner);
                move || dispatch_loop(inner, &dispatch_receiver)
            })?;
        lock(&inner.threads).extend([writer_handle, reader_handle, dispatch_handle]);

        Ok(Self { inner })
    }

    /// Registers a handler for incoming requests.
    ///
    /// Registering the same method twice replaces the previous handler.
    pub fn on_request<F>(&self, method: impl Into<String>, handler: F)
    where
        F: Fn(Request) -> Result<Value, RpcError> + Send + Sync + 'static,
    {
        write(&self.inner.requests).insert(method.into(), Arc::new(handler));
    }

    /// Registers a handler for incoming notifications.
    ///
    /// A handler that fails causes a [`ERROR_EVENT`] notification.
    pub fn on_notification<F>(&self, method: impl Into<String>, handler: F)
    where
        F: Fn(Request) -> Result<(), RpcError> + Send + Sync + 'static,
    {
        write(&self.inner.notifications).insert(method.into(), Arc::new(handler));
    }

    /// Registers a handler called once when the connection closes.
    ///
    /// If the connection is already closed the handler runs immediately on
    /// the calling thread.
    pub fn on_close<F>(&self, handler: F)
    where
        F: Fn(CloseReason) + Send + Sync + 'static,
    {
        let mut handlers = lock(&self.inner.close_handlers);
        if self.inner.closed.load(Ordering::SeqCst) {
            let reason = lock(&self.inner.close_reason)
                .clone()
                .unwrap_or(CloseReason::ClosedLocally);
            drop(handlers);
            handler(reason);
        } else {
            handlers.push(Box::new(handler));
        }
    }

    /// Sends a request and blocks without a timeout for its reply.
    ///
    /// This mirrors `rpc_send_call`, which waits on a call frame until the
    /// peer answers or the channel closes. Prefer [`Connection::call_timeout`]
    /// against peers you do not control.
    pub fn call(&self, method: &str, params: Vec<Value>) -> Result<Value, RpcError> {
        self.call_async(method, params)?.wait()
    }

    /// Sends a request with a reply timeout.
    ///
    /// On timeout the request is abandoned locally: a late reply is dropped
    /// silently, it does not close the connection.
    pub fn call_timeout(
        &self,
        method: &str,
        params: Vec<Value>,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        self.call_async(method, params)?.wait_timeout(timeout)
    }

    /// Sends a request without waiting, returning a [`PendingReply`].
    ///
    /// Multiple pending replies may be outstanding at once; the server is
    /// free to answer in any order (MessagePack-RPC specification, "The Order
    /// of the Response").
    pub fn call_async(&self, method: &str, params: Vec<Value>) -> Result<PendingReply, RpcError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(closed_error(&self.inner));
        }
        let msgid = next_msgid(&self.inner);
        let (sender, receiver) = mpsc::channel();
        lock(&self.inner.pending).insert(msgid, Pending::Waiter(sender));
        let message = Message::Request {
            msgid,
            method: method.to_string(),
            params,
        };
        if let Err(error) = enqueue(&self.inner, &message) {
            lock(&self.inner.pending).remove(&msgid);
            return Err(error);
        }
        Ok(PendingReply {
            inner: Arc::clone(&self.inner),
            msgid,
            method: method.to_string(),
            receiver,
        })
    }

    /// Sends a notification: no reply, no failure reporting, no waiting.
    pub fn notify(&self, method: &str, params: Vec<Value>) -> Result<(), RpcError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(closed_error(&self.inner));
        }
        let message = Message::Notification {
            method: method.to_string(),
            params,
        };
        enqueue(&self.inner, &message)
    }

    /// Queues raw, pre-encoded bytes.
    ///
    /// Adapted from `rpc_write_raw`: lets a producer that already has wire
    /// bytes (for example a UI redraw stream) skip a decode/encode round
    /// trip.
    pub fn write_raw(&self, bytes: Vec<u8>) -> Result<(), RpcError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(closed_error(&self.inner));
        }
        enqueue_bytes(&self.inner, bytes)
    }

    /// Closes the connection locally (idempotent).
    ///
    /// In-flight calls fail with a [`codes::CLOSED`](crate::error::codes::CLOSED)
    /// error; close handlers run once. The transport's shutdown hook (when
    /// available) unblocks the reader immediately.
    pub fn close(&self) {
        close(&self.inner, CloseReason::ClosedLocally);
    }

    /// Whether the connection is closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }

    /// The reason the connection closed, if it has.
    #[must_use]
    pub fn close_reason(&self) -> Option<CloseReason> {
        lock(&self.inner.close_reason).clone()
    }

    /// Waits for the reader, writer, and dispatch threads to finish.
    ///
    /// Call this after [`Connection::close`] or after the peer hangs up;
    /// joining a connection whose peer is silent blocks until the transport
    /// itself is interrupted (which is exactly what the Unix shutdown hook
    /// does). Never call it from inside a handler.
    pub fn join(&self) {
        let handles: Vec<JoinHandle<()>> = std::mem::take(&mut *lock(&self.inner.threads));
        for handle in handles {
            let _ = handle.join();
        }
    }
}

/// A request whose reply has not arrived yet.
pub struct PendingReply {
    inner: Arc<Inner>,
    msgid: u32,
    method: String,
    receiver: Receiver<Result<Value, RpcError>>,
}

impl PendingReply {
    /// The request id this reply belongs to.
    #[must_use]
    pub fn msgid(&self) -> u32 {
        self.msgid
    }

    /// The method name this reply belongs to.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Blocks without a timeout for the reply.
    pub fn wait(self) -> Result<Value, RpcError> {
        self.receiver
            .recv()
            .unwrap_or_else(|_| Err(closed_error(&self.inner)))
    }

    /// Blocks until the reply arrives or `timeout` elapses.
    ///
    /// On timeout the request is marked orphaned so a late reply is dropped.
    pub fn wait_timeout(self, timeout: Duration) -> Result<Value, RpcError> {
        match self.receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Disconnected) => Err(closed_error(&self.inner)),
            Err(RecvTimeoutError::Timeout) => {
                // Last chance: the reply may have landed between the timeout
                // firing and this check.
                if let Ok(result) = self.receiver.try_recv() {
                    return result;
                }
                let mut pending = lock(&self.inner.pending);
                if let Some(entry @ Pending::Waiter(_)) = pending.get_mut(&self.msgid) {
                    *entry = Pending::Orphaned;
                }
                drop(pending);
                Err(RpcError::timeout(format!(
                    "request `{}` (msgid {}) timed out after {timeout:?}",
                    self.method, self.msgid
                )))
            }
        }
    }

    /// Checks for the reply without blocking.
    #[must_use]
    pub fn try_wait(&mut self) -> Option<Result<Value, RpcError>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(closed_error(&self.inner))),
        }
    }

    /// Gives up on the reply, leaving it orphaned.
    pub fn discard(self) {
        let mut pending = lock(&self.inner.pending);
        if let Some(entry @ Pending::Waiter(_)) = pending.get_mut(&self.msgid) {
            *entry = Pending::Orphaned;
        }
    }
}

fn next_msgid(inner: &Inner) -> u32 {
    loop {
        let msgid = inner.next_msgid.fetch_add(1, Ordering::Relaxed);
        // 0 is not a valid msgid (notifications omit the field entirely);
        // skip it if the counter wraps.
        if msgid != 0 {
            return msgid;
        }
    }
}

fn closed_error(inner: &Inner) -> RpcError {
    match &*lock(&inner.close_reason) {
        Some(reason) => RpcError::closed(format!("connection is closed: {reason}")),
        None => RpcError::closed("connection is closed"),
    }
}

/// Encodes and queues one message for the writer thread.
fn enqueue(inner: &Inner, message: &Message) -> Result<(), RpcError> {
    let bytes = message
        .to_bytes()
        .map_err(|error| RpcError::exception(format!("failed to encode message: {error}")))?;
    enqueue_bytes(inner, bytes)
}

fn enqueue_bytes(inner: &Inner, bytes: Vec<u8>) -> Result<(), RpcError> {
    match lock(&inner.writer).clone() {
        Some(sender) => sender.send(bytes).map_err(|_| closed_error(inner)),
        None => Err(closed_error(inner)),
    }
}

/// The read loop: decodes messages, answers callers, queues work.
///
/// Mirrors `receive_msgpack`/`parse_msgpack`: any decode or shape failure is
/// a fatal channel error.
fn reader_loop<R: Read>(inner: Arc<Inner>, reader: R) {
    let mut reader = BufReader::new(reader);
    loop {
        match Message::decode_next(&mut reader) {
            Ok(None) => {
                close(&inner, CloseReason::PeerClosed);
                return;
            }
            Err(error) => {
                close(&inner, CloseReason::ProtocolViolation(error.to_string()));
                return;
            }
            Ok(Some(Message::Response {
                msgid,
                error,
                result,
            })) => {
                let entry = lock(&inner.pending).remove(&msgid);
                match entry {
                    Some(Pending::Waiter(sender)) => {
                        let _ = sender.send(match error {
                            Some(error) => Err(error),
                            None => Ok(result),
                        });
                    }
                    Some(Pending::Orphaned) => {}
                    None => {
                        // Mirrors parse_msgpack: a response to an unknown
                        // request id means the peers are out of sync, which
                        // is not recoverable.
                        close(
                            &inner,
                            CloseReason::ProtocolViolation(format!(
                                "peer returned a response with unknown request id {msgid}; \
                                 ensure the peer is properly synchronized"
                            )),
                        );
                        return;
                    }
                }
            }
            Ok(Some(message)) => {
                let sender = lock(&inner.dispatch).clone();
                match sender {
                    Some(sender) if sender.send(message).is_ok() => {}
                    // Dispatch is gone: the connection is closing.
                    _ => return,
                }
            }
        }
    }
}

/// The write loop: serializes outbound bytes, batching what is already
/// queued (mirrors the packer's flush callback, `packer_buffer_finish_channels`).
fn writer_loop<W: Write>(inner: Arc<Inner>, writer: W, jobs: &Receiver<Vec<u8>>) {
    let mut writer = BufWriter::new(writer);
    while let Ok(first) = jobs.recv() {
        let mut failure = writer.write_all(&first).err();
        if failure.is_none() {
            while let Ok(next) = jobs.try_recv() {
                if let Some(error) = writer.write_all(&next).err() {
                    failure = Some(error);
                    break;
                }
            }
        }
        if failure.is_none() {
            failure = writer.flush().err();
        }
        if let Some(error) = failure {
            close(&inner, CloseReason::WriteFailed(error.to_string()));
            return;
        }
    }
}

/// The dispatch loop: runs handlers in arrival order on one thread.
///
/// Mirrors `handle_request`/`request_event`: requests are answered, and a
/// failing notification handler turns into an [`ERROR_EVENT`] notification.
/// After close, queued work is dropped, like `request_event`'s closed-channel
/// early return.
fn dispatch_loop(inner: Arc<Inner>, queue: &Receiver<Message>) {
    while let Ok(message) = queue.recv() {
        if inner.closed.load(Ordering::SeqCst) {
            continue;
        }
        match message {
            Message::Request {
                msgid,
                method,
                params,
            } => {
                let response = match invoke(&inner.requests, &inner, &method, params) {
                    Ok(result) => Message::Response {
                        msgid,
                        error: None,
                        result,
                    },
                    Err(error) => Message::Response {
                        msgid,
                        error: Some(error),
                        result: Value::Nil,
                    },
                };
                let _ = enqueue(&inner, &response);
            }
            Message::Notification { method, params } => {
                if let Some(handler) = read(&inner.notifications).get(&method).cloned() {
                    let request = Request {
                        method: method.clone(),
                        params,
                        connection: Connection {
                            inner: Arc::clone(&inner),
                        },
                    };
                    if let Err(error) = handler(request) {
                        let notification = Message::Notification {
                            method: ERROR_EVENT.to_string(),
                            params: vec![Value::from(error.code), Value::from(error.message)],
                        };
                        let _ = enqueue(&inner, &notification);
                    }
                }
                // Unknown notifications are ignored (nothing to answer).
            }
            Message::Response { .. } => {}
        }
    }
}

fn invoke(
    handlers: &RwLock<HashMap<String, RequestHandler>>,
    inner: &Arc<Inner>,
    method: &str,
    params: Vec<Value>,
) -> Result<Value, RpcError> {
    let handler = read(handlers).get(method).cloned();
    match handler {
        Some(handler) => handler(Request {
            method: method.to_string(),
            params,
            connection: Connection {
                inner: Arc::clone(inner),
            },
        }),
        // Mirrors channel.c's unknown-method response: an exception-typed
        // error whose message matches the reference text ("Invalid method: X").
        None => Err(RpcError::exception(format!("Invalid method: {method}"))),
    }
}

/// Closes the connection once, failing in-flight calls.
///
/// Mirrors `chan_close_on_err` + `rpc_close`: idempotent, pending call frames
/// are answered with the close reason, and no further I/O is accepted.
fn close(inner: &Arc<Inner>, reason: CloseReason) {
    if inner.closed.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(shutdown) = &inner.shutdown {
        shutdown();
    }
    *lock(&inner.close_reason) = Some(reason.clone());
    let pending: Vec<Pending> = lock(&inner.pending)
        .drain()
        .map(|(_, entry)| entry)
        .collect();
    for entry in pending {
        if let Pending::Waiter(sender) = entry {
            let _ = sender.send(Err(RpcError::closed(reason.to_string())));
        }
    }
    // Dropping the senders ends the writer and dispatch loops once their
    // queues drain.
    *lock(&inner.writer) = None;
    *lock(&inner.dispatch) = None;
    let handlers: Vec<CloseHandler> = std::mem::take(&mut *lock(&inner.close_handlers));
    for handler in handlers {
        handler(reason.clone());
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read<T>(rwlock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    rwlock.read().unwrap_or_else(PoisonError::into_inner)
}

fn write<T>(rwlock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    rwlock.write().unwrap_or_else(PoisonError::into_inner)
}
