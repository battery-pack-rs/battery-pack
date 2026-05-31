//! End-to-end test against the real binary. Spawns two instances of this service: one with no
//! downstream URL (the in-memory backing store) and one pointed at it (the forwarder), then drives
//! item get/set over HTTP and checks that telemetry was emitted.

use std::time::{Duration, Instant};

use tokio::process::Command;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
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
    let port_backing = free_port();
    let port_forwarder = free_port();
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
        for entry in std::fs::read_dir(telemetry.path()).expect("read telemetry dir").flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            logs |= name.starts_with("application") && len > 0;
            metrics |= name.starts_with("metrics") && len > 0;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(logs, "no application log written to the telemetry dir");
    assert!(metrics, "no metrics written to the telemetry dir");
}

/// SIGTERM should drain in-flight work and exit cleanly (code 0).
#[cfg(unix)]
#[tokio::test]
async fn exits_cleanly_on_sigterm() {
    let bin = env!("CARGO_BIN_EXE_{{ project_name }}");
    let port = free_port();
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
