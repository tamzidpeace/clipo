use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clipo_lib::ipc::{
    default_ipc_endpoint, parse_cli_args, send_ipc_command, CliCommand, IpcCommand, IpcServer,
};
use clipo_lib::models::ClipoError;
use uuid::Uuid;

fn unique_test_endpoint() -> (Option<PathBuf>, PathBuf) {
    let short_id = &Uuid::new_v4().simple().to_string()[..10];
    #[cfg(unix)]
    {
        let candidate_dir = std::env::temp_dir().join(format!("clipo-{short_id}"));
        let candidate_sock = candidate_dir.join("c.sock");
        if candidate_sock.as_os_str().len() <= 96 {
            (Some(candidate_dir), candidate_sock)
        } else {
            let dir = PathBuf::from("/tmp").join(format!("clipo-{short_id}"));
            let sock = dir.join("c.sock");
            (Some(dir), sock)
        }
    }
    #[cfg(windows)]
    {
        let pipe = PathBuf::from(format!(r"\\.\pipe\clipo-ipc-test-{short_id}"));
        (None, pipe)
    }
}

#[test]
fn test_parse_cli_flags() {
    assert_eq!(parse_cli_args(&[]).unwrap(), CliCommand::RunGui);
    assert_eq!(
        parse_cli_args(&["--pause".to_string()]).unwrap(),
        CliCommand::Pause
    );
    assert_eq!(
        parse_cli_args(&["--resume".to_string()]).unwrap(),
        CliCommand::Resume
    );
    assert_eq!(
        parse_cli_args(&["--ignore-next".to_string()]).unwrap(),
        CliCommand::IgnoreNext
    );
    assert_eq!(
        parse_cli_args(&["--toggle".to_string()]).unwrap(),
        CliCommand::TogglePopup
    );

    let unknown = parse_cli_args(&["--unknown".to_string()]);
    assert!(
        matches!(unknown, Err(ClipoError::InvalidInput(_))),
        "expected InvalidInput for unknown flag, got {unknown:?}"
    );

    let conflicting = parse_cli_args(&["--pause".to_string(), "--resume".to_string()]);
    assert!(
        matches!(conflicting, Err(ClipoError::InvalidInput(_))),
        "expected InvalidInput for conflicting flags, got {conflicting:?}"
    );

    let duplicate = parse_cli_args(&["--pause".to_string(), "--pause".to_string()]);
    assert!(
        matches!(duplicate, Err(ClipoError::InvalidInput(_))),
        "expected InvalidInput for duplicate flags, got {duplicate:?}"
    );
}

#[test]
fn test_ipc_roundtrip_pause_resume_ignore_next() {
    let (temp_dir, endpoint) = unique_test_endpoint();
    let received: Arc<Mutex<Vec<IpcCommand>>> = Arc::new(Mutex::new(Vec::new()));
    let received_cb = Arc::clone(&received);

    let server = IpcServer::bind(
        &endpoint,
        Arc::new(move |cmd| {
            received_cb.lock().expect("lock received").push(cmd);
        }),
    )
    .expect("bind IpcServer");

    let expected = vec![
        IpcCommand::Pause,
        IpcCommand::IgnoreNext,
        IpcCommand::Resume,
        IpcCommand::TogglePopup,
    ];

    for &cmd in &expected {
        send_ipc_command(&endpoint, cmd).expect("send IPC command");
    }

    assert_eq!(*received.lock().expect("lock received"), expected);

    drop(server);

    #[cfg(unix)]
    assert!(
        !endpoint.exists(),
        "Unix socket file should be removed when IpcServer is dropped"
    );

    let after_shutdown = send_ipc_command(&endpoint, IpcCommand::Pause);
    assert!(
        after_shutdown.is_err(),
        "sending after IpcServer shutdown should fail"
    );

    if let Some(dir) = temp_dir {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(unix)]
#[test]
fn test_ipc_stale_socket_cleanup_and_active_conflict() {
    use std::os::unix::net::UnixListener;

    let (temp_dir, endpoint) = unique_test_endpoint();
    let parent = endpoint.parent().expect("socket parent");
    std::fs::create_dir_all(parent).expect("create temp socket dir");

    // Create a stale socket file (bind and drop listener without removing file).
    {
        let _stale = UnixListener::bind(&endpoint).expect("create stale unix socket");
    }
    assert!(endpoint.exists());

    let server = IpcServer::bind(&endpoint, Arc::new(|_| {}))
        .expect("bind should unlink stale socket and succeed");

    // Binding a second server while the first is active must fail.
    let conflict = IpcServer::bind(&endpoint, Arc::new(|_| {}));
    assert!(
        conflict.is_err(),
        "binding over an active IPC listener should fail"
    );

    drop(server);
    if let Some(dir) = temp_dir {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn test_default_ipc_endpoint_is_non_empty() {
    let ep = default_ipc_endpoint();
    assert!(!ep.as_os_str().is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let parent = ep.parent().expect("default IPC endpoint has parent dir");
        let mode = std::fs::metadata(parent)
            .expect("stat default IPC dir")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "default IPC dir should have 0o700 permissions");
    }
}

#[cfg(unix)]
#[test]
fn test_ipc_malformed_empty_and_oversized_resilience() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let (temp_dir, endpoint) = unique_test_endpoint();
    let received: Arc<Mutex<Vec<IpcCommand>>> = Arc::new(Mutex::new(Vec::new()));
    let received_cb = Arc::clone(&received);

    let server = IpcServer::bind(
        &endpoint,
        Arc::new(move |cmd| {
            received_cb.lock().expect("lock received").push(cmd);
        }),
    )
    .expect("bind IpcServer");

    // 1. Malformed JSON payload returns error and leaves server healthy.
    {
        let mut stream = UnixStream::connect(&endpoint).expect("connect malformed");
        stream
            .write_all(b"{not valid json}\n")
            .expect("write malformed");
        stream.flush().expect("flush malformed");
        let mut resp = String::new();
        BufReader::new(stream)
            .read_line(&mut resp)
            .expect("read malformed resp");
        assert!(
            resp.starts_with("err:"),
            "expected error response for malformed JSON, got {resp:?}"
        );
    }
    send_ipc_command(&endpoint, IpcCommand::Pause).expect("Pause after malformed");

    // 2. Empty EOF connect/disconnect leaves server healthy.
    {
        let _stream = UnixStream::connect(&endpoint).expect("connect empty EOF");
    }
    send_ipc_command(&endpoint, IpcCommand::Resume).expect("Resume after empty EOF");

    // 3. Oversized line (> MAX_IPC_LINE_BYTES without '\n') is rejected safely and leaves server healthy.
    {
        let mut stream = UnixStream::connect(&endpoint).expect("connect oversized");
        let oversized = vec![b'a'; clipo_lib::ipc::MAX_IPC_LINE_BYTES + 128];
        stream.write_all(&oversized).expect("write oversized");
        stream.flush().expect("flush oversized");
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut resp = String::new();
        BufReader::new(stream)
            .read_line(&mut resp)
            .expect("read oversized resp");
        assert!(
            resp.starts_with("err:"),
            "expected error response for oversized line, got {resp:?}"
        );
    }
    send_ipc_command(&endpoint, IpcCommand::IgnoreNext).expect("IgnoreNext after oversized");

    assert_eq!(
        *received.lock().expect("lock received"),
        vec![
            IpcCommand::Pause,
            IpcCommand::Resume,
            IpcCommand::IgnoreNext
        ]
    );

    drop(server);
    if let Some(dir) = temp_dir {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(unix)]
#[test]
fn test_unix_ipc_permissions_and_non_socket_rejection() {
    use std::os::unix::fs::PermissionsExt;

    let (temp_dir, endpoint) = unique_test_endpoint();
    let dir = temp_dir.as_ref().expect("unix temp dir").clone();

    let server = IpcServer::bind(&endpoint, Arc::new(|_| {})).expect("bind IpcServer");

    let dir_mode = std::fs::metadata(&dir)
        .expect("stat socket dir")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(dir_mode, 0o700, "IPC socket dir must be 0o700");

    let sock_mode = std::fs::metadata(&endpoint)
        .expect("stat socket file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(sock_mode, 0o600, "IPC socket file must be 0o600");

    drop(server);

    // Non-socket regular file at socket_path must be rejected and NOT deleted.
    std::fs::create_dir_all(&dir).expect("recreate dir");
    std::fs::write(&endpoint, b"do not delete").expect("write regular file");

    let err = IpcServer::bind(&endpoint, Arc::new(|_| {}))
        .err()
        .expect("non-socket file should fail to bind");
    assert!(
        matches!(err, ClipoError::InvalidInput(_)),
        "expected InvalidInput for non-socket path, got {err:?}"
    );
    assert_eq!(
        std::fs::read(&endpoint).expect("regular file still intact"),
        b"do not delete"
    );

    let _ = std::fs::remove_dir_all(dir);
}
