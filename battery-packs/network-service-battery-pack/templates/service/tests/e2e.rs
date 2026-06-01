//! End-to-end test against the real binary. Spawns two instances of this service: one with no
//! downstream URL (the in-memory backing store) and one pointed at it (the forwarder), then drives
//! item get/set over HTTP and checks that telemetry was emitted.

use std::time::{Duration, Instant};

use tokio::process::Command;

async fn free_port() -> u16 {
    tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Names and byte lengths of everything in `dir`, for diagnosing a telemetry assertion failure.
async fn dir_listing(dir: &std::path::Path) -> String {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return String::new(),
    };
    let mut out = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let len = tokio::fs::read(entry.path()).await.map(|b| b.len()).unwrap_or(0);
        out.push(format!("{} ({len} bytes)", entry.file_name().to_string_lossy()));
    }
    out.join(", ")
}

/// Polls `/health` every 50ms until it succeeds, up to 3s.
async fn wait_for_health(client: &reqwest::Client, base: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(resp) = client.get(format!("{base}/health")).send().await
            && resp.status().is_success()
        {
            return;
        }
        assert!(Instant::now() < deadline, "{base} was not healthy within 3s");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn forwards_items_through_a_downstream_instance() {
    let bin = env!("CARGO_BIN_EXE_{{ project_name }}");
    let port_backing = free_port().await;
    let port_forwarder = free_port().await;
    let client = reqwest::Client::new();

    // `kill_on_drop` stops the children when this test returns or panics; a tokio child otherwise
    // keeps running after its handle is dropped. Backing store: no downstream URL, so in-memory.
    let _backing = Command::new(bin)
        .args(["--bind-addr", &format!("127.0.0.1:{port_backing}")])
        .kill_on_drop(true)
        .spawn()
        .expect("spawn backing instance");
    // Forward to the backing instance, and route telemetry to a temp dir so we can assert on it.
    let telemetry = tempfile::tempdir().expect("create telemetry dir");
    let _forwarder = Command::new(bin)
        .args([
            "--bind-addr",
            &format!("127.0.0.1:{port_forwarder}"),
            "--downstream-url",
            &format!("http://127.0.0.1:{port_backing}"),
            "--telemetry-dir",
            &telemetry.path().to_string_lossy(),
        ])
        .kill_on_drop(true)
        .spawn()
        .expect("spawn forwarder");

    let backing_base = format!("http://127.0.0.1:{port_backing}");
    let forwarder_base = format!("http://127.0.0.1:{port_forwarder}");
    wait_for_health(&client, &backing_base).await;
    wait_for_health(&client, &forwarder_base).await;

    // PUT to the forwarder, which stores it in the backing instance; GET reads it back through both.
    let put = client
        .put(format!("{forwarder_base}/items/k"))
        .body("v")
        .send()
        .await
        .expect("put");
    assert_eq!(put.status(), reqwest::StatusCode::NO_CONTENT);
    let got = client
        .get(format!("{forwarder_base}/items/k"))
        .send()
        .await
        .expect("get");
    assert_eq!(got.status(), reqwest::StatusCode::OK);
    assert_eq!(got.text().await.unwrap(), "v");

    // Telemetry rolls to disk: both files should appear with content (writers flush async).
    let deadline = Instant::now() + Duration::from_secs(3);
    let (mut logs, mut metrics) = (false, false);
    while Instant::now() < deadline && !(logs && metrics) {
        let mut entries = tokio::fs::read_dir(telemetry.path()).await.expect("read telemetry dir");
        while let Some(entry) = entries.next_entry().await.expect("read telemetry entry") {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Read the bytes rather than trust the dir entry: on Windows the entry size lags behind
            // writes while the appender holds the file open, so metadata().len() reads 0 mid-run.
            let len = tokio::fs::read(entry.path()).await.map(|b| b.len()).unwrap_or(0);
            logs |= name.starts_with("application") && len > 0;
            metrics |= name.starts_with("metrics") && len > 0;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // List what did land so a CI-only flake (slow flush, unexpected name) is diagnosable from the log.
    let listing = dir_listing(telemetry.path()).await;
    assert!(logs, "no application log in telemetry dir after 3s; contents: [{listing}]");
    assert!(metrics, "no metrics in telemetry dir after 3s; contents: [{listing}]");
}

/// SIGTERM should drain in-flight work and exit cleanly (code 0).
#[cfg(unix)]
#[tokio::test]
async fn exits_cleanly_on_sigterm() {
    let bin = env!("CARGO_BIN_EXE_{{ project_name }}");
    let port = free_port().await;
    let client = reqwest::Client::new();
    let mut child = Command::new(bin)
        .args(["--port", &port.to_string()])
        .kill_on_drop(true)
        .spawn()
        .expect("spawn instance");
    wait_for_health(&client, &format!("http://127.0.0.1:{port}")).await;

    let pid = child.id().expect("child has a pid");
    let killed = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .await
        .expect("run kill");
    assert!(killed.success(), "kill -TERM failed");

    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("did not exit within 5s")
        .expect("await child");
    assert!(status.success(), "expected a clean exit, got {status}");
}
