#![allow(dead_code)]
use base64::Engine as _;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Output};
use twin_ado::Twin;

pub const PAT: &str = "pat-canary-8f3a91c2";

pub struct Env {
    pub twin: Twin,
    pub data: PathBuf,
    pub batch_max: usize,
    pub transcript: std::cell::RefCell<String>,
}

pub struct Tick {
    pub ok: bool,
    pub events: Vec<Value>,
}

impl Env {
    pub fn new(name: &str) -> Env {
        Env::with_batch_max(name, 8)
    }

    pub fn with_batch_max(name: &str, batch_max: usize) -> Env {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let twin = Twin::start(&root.join("twin"));
        twin.shared
            .repo
            .commit("main", None, &[("README", "base\n")], "init");
        Env {
            twin,
            data: root.join("data"),
            batch_max,
            transcript: Default::default(),
        }
    }

    pub fn adoq_with(&self, url: &str, args: &[&str]) -> Output {
        let out = Command::new(env!("CARGO_BIN_EXE_adoq"))
            .args(args)
            .env("ADOQ_URL", url)
            .env("ADOQ_REMOTE", &self.twin.shared.repo.path)
            .env("DATA_DIR", &self.data)
            .env("ADOQ_PAT", PAT)
            .env("ADOQ_BATCH_MAX", self.batch_max.to_string())
            .env("ADOQ_LOOPBACK_ONLY", "1")
            .output()
            .expect("run adoq");
        let mut log = self.transcript.borrow_mut();
        log.push_str(&String::from_utf8_lossy(&out.stdout));
        log.push_str(&String::from_utf8_lossy(&out.stderr));
        out
    }

    pub fn adoq(&self, args: &[&str]) -> Output {
        self.adoq_with(&self.twin.url(), args)
    }

    pub fn tick(&self) -> Tick {
        let out = self.adoq(&["do", "Tick"]);
        let events = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| serde_json::from_str(l).expect("NDJSON line"))
            .collect();
        Tick {
            ok: out.status.success(),
            events,
        }
    }

    pub fn tick_ok(&self) -> Vec<Value> {
        let tick = self.tick();
        assert!(tick.ok, "tick failed: {}", self.transcript.borrow());
        tick.events
    }

    pub fn enqueue(&self, pr: u64) {
        let out = self.adoq(&["do", "Enqueue", &json!({ "pr": pr }).to_string()]);
        assert!(
            out.status.success(),
            "enqueue failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    pub fn branch(&self, name: &str, base: &str, file: &str, content: &str) -> String {
        let base_sha = self.twin.shared.repo.rev_parse(base).expect("base branch");
        self.twin.shared.repo.commit(
            name,
            Some(&base_sha),
            &[(file, content)],
            &format!("add {name}"),
        )
    }

    pub fn pr(&self, source: &str, target: &str) -> u64 {
        self.twin.shared.create_pr(source, target, source)
    }

    pub fn git(&self, args: &[&str]) -> String {
        self.twin.shared.repo.run(args, None).expect("git")
    }

    pub fn sha(&self, branch: &str) -> String {
        self.git(&["rev-parse", &format!("refs/heads/{branch}")])
    }

    pub fn count(&self, range: &str) -> usize {
        self.git(&["rev-list", "--count", range])
            .parse()
            .expect("count")
    }

    pub fn api(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .into();
        let url = format!("{}/org/proj/_apis/{path}api-version=7.1", self.twin.url());
        let auth = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("test:{PAT}"))
        );
        let request = ureq::http::Request::builder()
            .method(method)
            .uri(url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .body(body.map_or_else(String::new, |b| b.to_string()))
            .expect("request");
        let mut response = agent.run(request).expect("twin reachable");
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().expect("body");
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    pub fn pr_json(&self, id: u64) -> Value {
        self.api(
            "GET",
            &format!("git/repositories/repo/pullrequests/{id}?"),
            None,
        )
        .1
    }

    pub fn statuses(&self, id: u64) -> Vec<Value> {
        self.api(
            "GET",
            &format!("git/repositories/repo/pullrequests/{id}/statuses?"),
            None,
        )
        .1["value"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    pub fn status_of(&self, id: u64, name: &str) -> Option<Value> {
        self.statuses(id)
            .into_iter()
            .find(|s| s["context"]["name"] == name)
    }

    pub fn completions(&self) -> Vec<u64> {
        self.twin
            .shared
            .log()
            .iter()
            .filter(|e| e["kind"] == "pr_completed")
            .map(|e| e["pr"].as_u64().unwrap())
            .collect()
    }

    pub fn refs(&self) -> String {
        self.git(&["for-each-ref", "--format=%(objectname) %(refname)"])
    }

    pub fn tick_until(&self, done: impl Fn(&Env) -> bool) {
        for _ in 0..40 {
            if done(self) {
                return;
            }
            self.tick();
        }
        panic!(
            "condition not reached after 40 ticks; transcript:\n{}",
            self.transcript.borrow()
        );
    }
}
