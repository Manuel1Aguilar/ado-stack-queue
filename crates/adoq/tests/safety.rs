mod common;
use base64::Engine as _;
use common::{Env, PAT};
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<Vec<u8>>) {
    for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if let Ok(bytes) = std::fs::read(&path) {
            out.push(bytes);
        }
    }
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

#[test]
fn pat_never_reaches_logs_telemetry_or_database() {
    let env = Env::with_batch_max("safety-pat", 2);
    env.branch("a", "main", "a.txt", "a\n");
    env.branch("b", "a", "b.txt", "b\n");
    env.branch("bad", "main", "BUILD_FAILS", "x\n");
    let (a, _b, bad) = (env.pr("a", "main"), env.pr("b", "a"), env.pr("bad", "main"));
    env.enqueue(bad);
    env.twin.shared.human_complete(a).unwrap();
    env.tick_until(|e| e.status_of(bad, "queue").is_some());
    env.adoq(&["ls"]);

    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("adoq:{PAT}"));
    let mut files = Vec::new();
    walk(&env.data, &mut files);
    assert!(
        files.iter().any(|f| contains(f, "\"kind\"")),
        "telemetry was written"
    );
    for secret in [PAT, encoded.as_str()] {
        assert!(
            !env.transcript.borrow().contains(secret),
            "secret in stdout or stderr"
        );
        assert!(
            files.iter().all(|f| !contains(f, secret)),
            "secret in a file under DATA_DIR"
        );
    }
}

#[test]
fn twin_listens_on_loopback_only() {
    let env = Env::new("safety-bind");
    assert!(env.twin.addr.ip().is_loopback(), "{}", env.twin.addr);
}

#[test]
fn guard_refuses_non_loopback_connections() {
    let env = Env::new("safety-guard");
    let out = env.adoq_with("http://192.0.2.1:9", &["ls"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("non-loopback"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
