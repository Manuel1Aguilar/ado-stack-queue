use base64::Engine as _;
use serde_json::{json, Value};
use twin_ado::Twin;

struct Client {
    twin: Twin,
    agent: ureq::Agent,
}

impl Client {
    fn new(name: &str) -> Client {
        let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let twin = Twin::start(&dir);
        twin.shared.repo.commit("main", None, &[("f", "1")], "init");
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .into();
        Client { twin, agent }
    }

    fn call(
        &self,
        method: &str,
        path: &str,
        auth: bool,
        body: Option<Value>,
    ) -> (u16, Option<String>, Value) {
        let mut req = ureq::http::Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.twin.url()));
        if auth {
            req = req.header(
                "Authorization",
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(":secret")
                ),
            );
        }
        let mut resp = self
            .agent
            .run(
                req.header("Content-Type", "application/json")
                    .body(body.map_or_else(String::new, |b| b.to_string()))
                    .unwrap(),
            )
            .unwrap();
        let retry = resp
            .headers()
            .get("retry-after")
            .map(|v| v.to_str().unwrap().to_string());
        let status = resp.status().as_u16();
        (
            status,
            retry,
            serde_json::from_str(&resp.body_mut().read_to_string().unwrap()).unwrap_or(Value::Null),
        )
    }

    fn get(&self, tail: &str) -> (u16, Value) {
        let (s, _, v) = self.call(
            "GET",
            &format!("/org/proj/_apis/git/repositories/repo/{tail}"),
            true,
            None,
        );
        (s, v)
    }
}

#[test]
fn rejects_missing_pat_missing_api_version_and_unknown_pr() {
    let c = Client::new("fidelity-errors");
    c.twin
        .shared
        .repo
        .commit("f", Some("main"), &[("g", "1")], "f");
    let id = c.twin.shared.create_pr("f", "main", "f");
    let base = "/org/proj/_apis/git/repositories/repo/pullrequests";

    assert_eq!(
        c.call("GET", &format!("{base}?api-version=7.1"), false, None)
            .0,
        401
    );
    assert_eq!(c.call("GET", &format!("{base}/{id}"), true, None).0, 400);
    assert_eq!(c.get(&format!("pullrequests/999?api-version=7.1")).0, 404);
    assert_eq!(
        c.call(
            "GET",
            "/org/proj/_apis/git/repositories/nope/pullrequests?api-version=7.1",
            true,
            None
        )
        .0,
        404
    );
    assert_eq!(c.get(&format!("pullrequests/{id}?api-version=7.1")).0, 200);
}

#[test]
fn paginates_with_top_and_skip() {
    let c = Client::new("fidelity-paging");
    for i in 0..5 {
        c.twin
            .shared
            .repo
            .commit(&format!("f{i}"), Some("main"), &[("g", "1")], "f");
        c.twin.shared.create_pr(&format!("f{i}"), "main", "f");
    }
    let page = |skip: usize| {
        c.get(&format!("pullrequests?$top=2&$skip={skip}&api-version=7.1"))
            .1
    };
    let ids = |v: Value| {
        v["value"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["pullRequestId"].as_u64().unwrap())
            .collect::<Vec<_>>()
    };

    assert_eq!(ids(page(0)), [1, 2]);
    assert_eq!(ids(page(2)), [3, 4]);
    assert_eq!(ids(page(4)), [5]);
}

#[test]
fn throttles_with_429_and_retry_after_past_the_budget() {
    let c = Client::new("fidelity-throttle");
    c.twin.shared.set_flags(&json!({ "rate_limit": 2 }));
    let path = "/org/proj/_apis/git/repositories/repo/pullrequests?api-version=7.1";

    assert_eq!(c.call("GET", path, true, None).0, 200);
    assert_eq!(c.call("GET", path, true, None).0, 200);
    let (status, retry, _) = c.call("GET", path, true, None);
    assert_eq!((status, retry.as_deref()), (429, Some("1")));
}

#[test]
fn completing_with_a_stale_source_commit_is_a_409() {
    let c = Client::new("fidelity-stale");
    let tip = c
        .twin
        .shared
        .repo
        .commit("f", Some("main"), &[("g", "1")], "f");
    let id = c.twin.shared.create_pr("f", "main", "f");
    let path = format!("/org/proj/_apis/git/repositories/repo/pullrequests/{id}?api-version=7.1");
    let complete =
        |sha: &str| json!({ "status": "completed", "lastMergeSourceCommit": { "commitId": sha } });

    assert_eq!(
        c.call("PATCH", &path, true, Some(complete(&"0".repeat(40))))
            .0,
        409
    );
    let (status, _, pr) = c.call("PATCH", &path, true, Some(complete(&tip)));
    assert_eq!((status, pr["status"].as_str()), (200, Some("completed")));
}

#[test]
fn ignores_target_changes_when_retargeting_is_off() {
    let c = Client::new("fidelity-retarget");
    c.twin
        .shared
        .repo
        .commit("f", Some("main"), &[("g", "1")], "f");
    let id = c.twin.shared.create_pr("f", "main", "f");
    let path = format!("/org/proj/_apis/git/repositories/repo/pullrequests/{id}?api-version=7.1");
    let retarget = json!({ "targetRefName": "refs/heads/other" });

    c.twin.shared.set_flags(&json!({ "retargeting": false }));
    assert_eq!(
        c.call("PATCH", &path, true, Some(retarget.clone())).2["targetRefName"],
        "refs/heads/main"
    );
    c.twin.shared.set_flags(&json!({ "retargeting": true }));
    assert_eq!(
        c.call("PATCH", &path, true, Some(retarget)).2["targetRefName"],
        "refs/heads/other"
    );
}
