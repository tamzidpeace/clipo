use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::ClipoError;

/// Maximum byte length allowed for a single newline-delimited IPC message or acknowledgement.
pub const MAX_IPC_LINE_BYTES: usize = 1024;

/// IPC control commands sent from CLI invocations to the running Clipo daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcCommand {
    Pause,
    Resume,
    IgnoreNext,
    TogglePopup,
}

/// Parsed CLI command for the Clipo binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliCommand {
    RunGui,
    Pause,
    Resume,
    IgnoreNext,
    TogglePopup,
}

impl CliCommand {
    /// Converts a control CLI command to its corresponding [`IpcCommand`], or `None` for [`CliCommand::RunGui`].
    pub fn to_ipc_command(self) -> Option<IpcCommand> {
        match self {
            Self::RunGui => None,
            Self::Pause => Some(IpcCommand::Pause),
            Self::Resume => Some(IpcCommand::Resume),
            Self::IgnoreNext => Some(IpcCommand::IgnoreNext),
            Self::TogglePopup => Some(IpcCommand::TogglePopup),
        }
    }
}

/// Parses CLI flag arguments (excluding the executable name, e.g. `env::args().skip(1)`).
///
/// An empty slice selects [`CliCommand::RunGui`]. Supplying unknown flags or more than one flag
/// returns [`ClipoError::InvalidInput`].
pub fn parse_cli_args(args: &[String]) -> Result<CliCommand, ClipoError> {
    if args.is_empty() {
        return Ok(CliCommand::RunGui);
    }

    if args.len() > 1 {
        return Err(ClipoError::InvalidInput(format!(
            "expected at most one CLI flag, got {}: {}",
            args.len(),
            args.join(" ")
        )));
    }

    match args[0].as_str() {
        "--pause" => Ok(CliCommand::Pause),
        "--resume" => Ok(CliCommand::Resume),
        "--ignore-next" => Ok(CliCommand::IgnoreNext),
        "--toggle" => Ok(CliCommand::TogglePopup),
        other => Err(ClipoError::InvalidInput(format!(
            "unknown CLI argument: {other}"
        ))),
    }
}

/// Returns the default OS-appropriate IPC endpoint path.
///
/// - Unix: `$XDG_RUNTIME_DIR/clipo/clipo.sock` when set, otherwise a user-scoped directory under
///   `std::env::temp_dir()`. Ensures the parent directory exists.
/// - Windows: `\\.\pipe\clipo-ipc`.
pub fn default_ipc_endpoint() -> PathBuf {
    #[cfg(unix)]
    {
        let scope = unix_user_scope_dir();
        let candidate_dir = match std::env::var_os("XDG_RUNTIME_DIR").filter(|val| !val.is_empty()) {
            Some(runtime_dir) => PathBuf::from(runtime_dir).join("clipo"),
            None => std::env::temp_dir().join(&scope),
        };
        let candidate_sock = candidate_dir.join("clipo.sock");
        // Unix domain socket paths are limited by `sockaddr_un::sun_path` (104 bytes on macOS).
        let sock = if candidate_sock.as_os_str().len() <= 96 {
            candidate_sock
        } else {
            PathBuf::from("/tmp").join(&scope).join("clipo.sock")
        };
        if let Some(parent) = sock.parent() {
            let _ = ensure_private_unix_dir(parent);
        }
        sock
    }

    #[cfg(windows)]
    {
        PathBuf::from(r"\\.\pipe\clipo-ipc")
    }
}

#[cfg(unix)]
fn ensure_private_unix_dir(dir: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(unix)]
fn unix_user_scope_dir() -> String {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: `geteuid()` is always safe to call and has no preconditions.
        let uid = unsafe { libc::geteuid() };
        format!("clipo-{uid}")
    }

    #[cfg(not(target_os = "macos"))]
    {
        for key in ["UID", "USER", "LOGNAME"] {
            if let Ok(val) = std::env::var(key) {
                let sanitized: String = val
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                if !sanitized.is_empty() {
                    return format!("clipo-{sanitized}");
                }
            }
        }
        "clipo".to_string()
    }
}

fn serialize_ipc_line(cmd: IpcCommand) -> Result<String, ClipoError> {
    let mut line = serde_json::to_string(&cmd)?;
    line.push('\n');
    Ok(line)
}

fn check_line_length(line: &str) -> Result<(), ClipoError> {
    if line.len() > MAX_IPC_LINE_BYTES {
        return Err(ClipoError::InvalidInput(format!(
            "IPC line exceeds maximum length of {MAX_IPC_LINE_BYTES} bytes"
        )));
    }
    Ok(())
}

fn dispatch_ipc_line(
    line: &str,
    handler: &(dyn Fn(IpcCommand) + Send + Sync),
) -> Result<&'static [u8], ClipoError> {
    check_line_length(line)?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(ClipoError::InvalidInput("empty IPC command".to_string()));
    }
    let cmd: IpcCommand = serde_json::from_str(trimmed)?;
    handler(cmd);
    Ok(b"ok\n")
}

fn validate_ack_response(resp: &str) -> Result<(), ClipoError> {
    check_line_length(resp)?;
    let trimmed = resp.trim();
    if trimmed == "ok" {
        Ok(())
    } else if trimmed.is_empty() {
        Err(ClipoError::Internal(
            "IPC server closed connection without acknowledging command".to_string(),
        ))
    } else {
        Err(ClipoError::Internal(format!(
            "IPC server rejected command: {trimmed}"
        )))
    }
}

#[cfg(unix)]
mod platform {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use super::{
        check_line_length, dispatch_ipc_line, ensure_private_unix_dir, serialize_ipc_line,
        validate_ack_response, IpcCommand, MAX_IPC_LINE_BYTES,
    };
    use crate::models::ClipoError;

    const IO_TIMEOUT: Duration = Duration::from_secs(5);

    struct DeadlineStream<'a> {
        stream: &'a UnixStream,
        deadline: Instant,
    }

    impl Read for DeadlineStream<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            loop {
                let Some(remaining) = self.deadline.checked_duration_since(Instant::now()) else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "IPC read deadline exceeded",
                    ));
                };
                let _ = self
                    .stream
                    .set_read_timeout(Some(remaining.max(Duration::from_millis(1))));
                match (&*self.stream).read(buf) {
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                    other => return other,
                }
            }
        }
    }

    fn read_bounded_line(stream: &UnixStream, timeout: Duration) -> Result<String, ClipoError> {
        let limited = DeadlineStream {
            stream,
            deadline: Instant::now() + timeout,
        }
        .take((MAX_IPC_LINE_BYTES + 1) as u64);
        let mut reader = BufReader::new(limited);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        check_line_length(&line)?;
        Ok(line)
    }

    /// Local IPC server listening on a Unix domain socket.
    pub struct IpcServer {
        socket_path: PathBuf,
        stop_flag: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl IpcServer {
        /// Binds an [`IpcServer`] on `socket_path` and dispatches incoming commands to `handler`.
        pub fn bind(
            socket_path: &Path,
            handler: Arc<dyn Fn(IpcCommand) + Send + Sync>,
        ) -> Result<Self, ClipoError> {
            if let Some(parent) = socket_path.parent() {
                if !parent.as_os_str().is_empty() {
                    let is_clipo_dir = parent
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("clipo"));
                    if !parent.exists() || is_clipo_dir {
                        ensure_private_unix_dir(parent)?;
                    }
                }
            }

            match std::fs::symlink_metadata(socket_path) {
                Ok(meta) => {
                    if !meta.file_type().is_socket() {
                        return Err(ClipoError::InvalidInput(format!(
                            "refusing to overwrite non-socket IPC path {}",
                            socket_path.display()
                        )));
                    }
                    match UnixStream::connect(socket_path) {
                        Ok(_) => {
                            return Err(ClipoError::Io(std::io::Error::new(
                                std::io::ErrorKind::AddrInUse,
                                format!("IPC endpoint already active at {}", socket_path.display()),
                            )));
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                            return Err(ClipoError::Io(err));
                        }
                        Err(_) => {
                            std::fs::remove_file(socket_path)?;
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(ClipoError::Io(err)),
            }

            let listener = UnixListener::bind(socket_path)?;
            std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;
            let stop_flag = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop_flag);

            let thread = std::thread::Builder::new()
                .name("clipo-ipc-server".to_string())
                .spawn(move || loop {
                    if thread_stop.load(Ordering::Acquire) {
                        break;
                    }
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if thread_stop.load(Ordering::Acquire) {
                                break;
                            }
                            let _ = handle_client(stream, handler.as_ref());
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                })?;

            Ok(Self {
                socket_path: socket_path.to_path_buf(),
                stop_flag,
                thread: Some(thread),
            })
        }
    }

    impl Drop for IpcServer {
        fn drop(&mut self) {
            self.stop_flag.store(true, Ordering::Release);
            if let Some(handle) = self.thread.take() {
                let _ = handle.join();
            }
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }

    fn handle_client(
        mut stream: UnixStream,
        handler: &(dyn Fn(IpcCommand) + Send + Sync),
    ) -> Result<(), ClipoError> {
        // Accepted sockets may inherit non-blocking mode from the listener on BSD/macOS.
        stream.set_nonblocking(false)?;
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));

        let line = match read_bounded_line(&stream, IO_TIMEOUT) {
            Ok(line) => line,
            Err(err) => {
                let msg = format!("err:{err}\n");
                let _ = stream.write_all(msg.as_bytes());
                let _ = stream.flush();
                return Err(err);
            }
        };

        if line.is_empty() {
            return Ok(());
        }

        match dispatch_ipc_line(&line, handler) {
            Ok(ack) => {
                stream.write_all(ack)?;
                stream.flush()?;
            }
            Err(err) => {
                let msg = format!("err:{err}\n");
                let _ = stream.write_all(msg.as_bytes());
                let _ = stream.flush();
            }
        }
        Ok(())
    }

    /// Sends a single [`IpcCommand`] to a running [`IpcServer`] at `socket_path` and waits for ack.
    pub fn send_ipc_command(socket_path: &Path, cmd: IpcCommand) -> Result<(), ClipoError> {
        let mut stream = UnixStream::connect(socket_path)?;
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));

        let payload = serialize_ipc_line(cmd)?;
        stream.write_all(payload.as_bytes())?;
        stream.flush()?;

        let resp = read_bounded_line(&stream, IO_TIMEOUT)?;
        validate_ack_response(&resp)
    }
}

#[cfg(windows)]
mod platform {
    use std::future::{poll_fn, Future};
    use std::path::Path;
    use std::pin::{pin, Pin};
    use std::sync::Arc;
    use std::task::Poll;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
    use tokio::sync::oneshot;

    use super::{
        check_line_length, dispatch_ipc_line, serialize_ipc_line, validate_ack_response,
        IpcCommand, MAX_IPC_LINE_BYTES,
    };
    use crate::models::ClipoError;

    // Win32 `ERROR_PIPE_BUSY` (0xE7) returned by CreateFile when all pipe instances are busy.
    const ERROR_PIPE_BUSY: i32 = 231;
    const IO_TIMEOUT: Duration = Duration::from_secs(5);

    async fn read_bounded_line_async<R: AsyncRead + Unpin>(
        reader_src: R,
        timeout: Duration,
    ) -> Result<String, ClipoError> {
        let mut reader = BufReader::new(reader_src.take((MAX_IPC_LINE_BYTES + 1) as u64));
        let mut line = String::new();
        tokio::time::timeout(timeout, reader.read_line(&mut line))
            .await
            .map_err(|_| {
                ClipoError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "IPC read timeout",
                ))
            })??;
        check_line_length(&line)?;
        Ok(line)
    }

    enum AcceptEvent {
        Shutdown,
        Connected(std::io::Result<()>),
    }

    /// Local IPC server listening on a Windows named pipe.
    pub struct IpcServer {
        shutdown_tx: Option<oneshot::Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl IpcServer {
        /// Binds an [`IpcServer`] on the named pipe `socket_path` and dispatches incoming commands to `handler`.
        pub fn bind(
            socket_path: &Path,
            handler: Arc<dyn Fn(IpcCommand) + Send + Sync>,
        ) -> Result<Self, ClipoError> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;

            let first_server = {
                let _guard = rt.enter();
                ServerOptions::new()
                    .first_pipe_instance(true)
                    .create(socket_path)?
            };

            let pipe_name = socket_path.to_path_buf();
            let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

            let thread = std::thread::Builder::new()
                .name("clipo-ipc-server".to_string())
                .spawn(move || {
                    rt.block_on(async move {
                        let mut server = first_server;
                        loop {
                            let event = {
                                let mut connect_fut = pin!(server.connect());
                                poll_fn(|cx| {
                                    if Pin::new(&mut shutdown_rx).poll(cx).is_ready() {
                                        return Poll::Ready(AcceptEvent::Shutdown);
                                    }
                                    if let Poll::Ready(res) = connect_fut.as_mut().poll(cx) {
                                        return Poll::Ready(AcceptEvent::Connected(res));
                                    }
                                    Poll::Pending
                                })
                                .await
                            };

                            match event {
                                AcceptEvent::Shutdown => break,
                                AcceptEvent::Connected(Err(_)) => break,
                                AcceptEvent::Connected(Ok(())) => {
                                    let connected = server;
                                    match ServerOptions::new().create(&pipe_name) {
                                        Ok(next) => server = next,
                                        Err(_) => break,
                                    }
                                    let handler = Arc::clone(&handler);
                                    tokio::spawn(async move {
                                        let _ = handle_client(connected, handler.as_ref()).await;
                                    });
                                }
                            }
                        }
                    });
                })?;

            Ok(Self {
                shutdown_tx: Some(shutdown_tx),
                thread: Some(thread),
            })
        }
    }

    impl Drop for IpcServer {
        fn drop(&mut self) {
            if let Some(tx) = self.shutdown_tx.take() {
                let _ = tx.send(());
            }
            if let Some(handle) = self.thread.take() {
                let _ = handle.join();
            }
        }
    }

    async fn handle_client(
        stream: NamedPipeServer,
        handler: &(dyn Fn(IpcCommand) + Send + Sync),
    ) -> Result<(), ClipoError> {
        let (read_half, mut write_half) = tokio::io::split(stream);
        let line = match read_bounded_line_async(read_half, IO_TIMEOUT).await {
            Ok(line) => line,
            Err(err) => {
                let msg = format!("err:{err}\n");
                let _ = write_half.write_all(msg.as_bytes()).await;
                let _ = write_half.flush().await;
                return Err(err);
            }
        };

        if line.is_empty() {
            return Ok(());
        }

        match dispatch_ipc_line(&line, handler) {
            Ok(ack) => {
                tokio::time::timeout(IO_TIMEOUT, write_half.write_all(ack))
                    .await
                    .map_err(|_| ClipoError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "IPC write timeout")))??;
                write_half.flush().await?;
            }
            Err(err) => {
                let msg = format!("err:{err}\n");
                let _ = write_half.write_all(msg.as_bytes()).await;
                let _ = write_half.flush().await;
            }
        }
        Ok(())
    }

    /// Sends a single [`IpcCommand`] to a running [`IpcServer`] at named pipe `socket_path`.
    pub fn send_ipc_command(socket_path: &Path, cmd: IpcCommand) -> Result<(), ClipoError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        rt.block_on(async move {
            let deadline = Instant::now() + IO_TIMEOUT;
            let client = loop {
                match ClientOptions::new().open(socket_path) {
                    Ok(client) => break client,
                    Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                        if Instant::now() >= deadline {
                            return Err(ClipoError::Io(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "timed out waiting for busy IPC named pipe",
                            )));
                        }
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    Err(err) => return Err(ClipoError::Io(err)),
                }
            };

            let payload = serialize_ipc_line(cmd)?;
            let (read_half, mut write_half) = tokio::io::split(client);
            write_half.write_all(payload.as_bytes()).await?;
            write_half.flush().await?;

            let resp = read_bounded_line_async(read_half, IO_TIMEOUT).await?;
            validate_ack_response(&resp)
        })
    }
}

pub use platform::{send_ipc_command, IpcServer};
