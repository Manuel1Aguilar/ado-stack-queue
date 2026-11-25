pub mod repo;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use repo::{Merge, Repo};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};

const REPO_NAME: &str = "repo";
const DEFAULT_TOP: usize = 100;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Active,
    Completed,
    Abandoned,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::Completed => "completed",
            Status::Abandoned => "abandoned",
        }
    }
}

struct Pr {
    id: u64,
    title: String,
    source: String,
    target: String,
    status: Status,
    labels: Vec<String>,
    statuses: Vec<Value>,
    completed_sha: Option<String>,
}

struct Build {
    id: u64,
    branch: String,
    sha: String,
    polls: u32,
}

struct Flags {
    retargeting: bool,
    rate_limit: Option<u64>,
    fail_complete_after: Option<u64>,
}

struct State_ {
    prs: Vec<Pr>,
    builds: Vec<Build>,
    flags: Flags,
    requests: u64,
    mutations: u64,
    api_completions: u64,
    log: Vec<Value>,
}

pub struct Shared {
    pub repo: Repo,
    state: Mutex<State_>,
}

pub struct Twin {
    pub addr: SocketAddr,
    pub shared: Arc<Shared>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

fn full_ref(name: &str) -> String {
    if name.starts_with("refs/") {
        name.to_string()
    } else {
        format!("refs/heads/{name}")
    }
}

fn short(refname: &str) -> &str {
    refname.strip_prefix("refs/heads/").unwrap_or(refname)
}

fn reply(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "message": message }))).into_response()
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State_> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn create_pr(&self, source: &str, target: &str, title: &str) -> u64 {
        let mut st = self.lock();
        let id = st.prs.len() as u64 + 1;
        st.prs.push(Pr {
            id,
            title: title.to_string(),
            source: full_ref(source),
            target: full_ref(target),
            status: Status::Active,
            labels: Vec::new(),
            statuses: Vec::new(),
            completed_sha: None,
        });
        id
    }

    pub fn human_complete(&self, id: u64) -> Result<String, String> {
        let mut st = self.lock();
        self.complete(&mut st, id, None, "human")
    }

    fn complete(
        &self,
        st: &mut State_,
        id: u64,
        expect: Option<&str>,
        by: &str,
    ) -> Result<String, String> {
        let pr = st.prs.iter_mut().find(|p| p.id == id).ok_or("unknown PR")?;
        if pr.status != Status::Active {
            return Err("PR is not active".into());
        }
        let tip = self
            .repo
            .rev_parse(&pr.source)
            .ok_or("source branch is missing")?;
        if expect.is_some_and(|want| want != tip) {
            return Err("lastMergeSourceCommit does not match the source branch tip".into());
        }
        let message = format!("Merged PR {id}: {}", pr.title);
        match self.repo.merge_into(short(&pr.target), &tip, &message)? {
            Merge::Conflict => return Err("merge conflict".into()),
            Merge::Done(_) => {}
        }
        pr.status = Status::Completed;
        pr.completed_sha = Some(tip.clone());
        st.log
            .push(json!({ "kind": "pr_completed", "pr": id, "by": by, "source_sha": tip }));
        Ok(tip)
    }

    pub fn set_flags(&self, body: &Value) {
        let mut st = self.lock();
        if let Some(v) = body.get("retargeting").and_then(Value::as_bool) {
            st.flags.retargeting = v;
        }
        if let Some(v) = body.get("rate_limit") {
            st.flags.rate_limit = v.as_u64();
            st.requests = 0;
        }
        if let Some(v) = body.get("fail_complete_after") {
            st.flags.fail_complete_after = v.as_u64();
            st.api_completions = 0;
        }
    }

    pub fn log(&self) -> Vec<Value> {
        self.lock().log.clone()
    }

    pub fn mutations(&self) -> u64 {
        self.lock().mutations
    }

    pub fn builds_created(&self) -> usize {
        self.lock().builds.len()
    }

    fn pr_json(&self, pr: &Pr) -> Value {
        let commit = pr
            .completed_sha
            .clone()
            .or_else(|| self.repo.rev_parse(&pr.source))
            .unwrap_or_default();
        json!({
            "pullRequestId": pr.id,
            "title": pr.title,
            "status": pr.status.name(),
            "sourceRefName": pr.source,
            "targetRefName": pr.target,
            "lastMergeSourceCommit": { "commitId": commit },
            "labels": pr.labels.iter().map(|l| json!({ "name": l, "active": true })).collect::<Vec<_>>(),
            "repository": { "name": REPO_NAME },
        })
    }
}

impl Twin {
    pub fn start(dir: &std::path::Path) -> Twin {
        Twin::start_on(dir, 0)
    }

    pub fn start_on(dir: &std::path::Path, port: u16) -> Twin {
        let shared = Arc::new(Shared {
            repo: Repo::init(&dir.join("repo.git")),
            state: Mutex::new(State_ {
                prs: Vec::new(),
                builds: Vec::new(),
                flags: Flags {
                    retargeting: true,
                    rate_limit: None,
                    fail_complete_after: None,
                },
                requests: 0,
                mutations: 0,
                api_completions: 0,
                log: Vec::new(),
            }),
        });
        let app = router(shared.clone());
        let (addr_tx, addr_rx) = std::sync::mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("runtime");
            rt.block_on(async {
                let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
                    .await
                    .expect("bind 127.0.0.1");
                addr_tx
                    .send(listener.local_addr().expect("addr"))
                    .expect("send addr");
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = stopped.await;
                    })
                    .await
                    .expect("serve");
            });
        });
        let addr = addr_rx.recv().expect("twin address");
        Twin {
            addr,
            shared,
            stop: Some(stop),
            thread: Some(thread),
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for Twin {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

type Shr = State<Arc<Shared>>;
type Q = Query<HashMap<String, String>>;

fn router(shared: Arc<Shared>) -> Router {
    let api = Router::new()
        .route(
            "/{org}/{project}/_apis/git/repositories/{repo}/pullrequests",
            get(list_prs),
        )
        .route(
            "/{org}/{project}/_apis/git/repositories/{repo}/pullrequests/{id}",
            get(get_pr).patch(patch_pr),
        )
        .route(
            "/{org}/{project}/_apis/git/repositories/{repo}/pullrequests/{id}/statuses",
            get(list_statuses).post(post_status),
        )
        .route(
            "/{org}/{project}/_apis/git/repositories/{repo}/pullrequests/{id}/labels",
            post(add_label),
        )
        .route(
            "/{org}/{project}/_apis/git/repositories/{repo}/pullrequests/{id}/labels/{name}",
            axum::routing::delete(remove_label),
        )
        .route(
            "/{org}/{project}/_apis/git/repositories/{repo}/refs",
            get(list_refs),
        )
        .route("/{org}/{project}/_apis/build/builds", post(queue_build))
        .route("/{org}/{project}/_apis/build/builds/{id}", get(get_build))
        .layer(middleware::from_fn_with_state(shared.clone(), gate));
    let control = Router::new()
        .route("/_twin/prs", post(ctl_create_pr))
        .route("/_twin/prs/{id}/complete", post(ctl_complete))
        .route("/_twin/flags", post(ctl_flags))
        .route("/_twin/log", get(ctl_log))
        .route("/_twin/stats", get(ctl_stats))
        .route("/", get(|| async { Json(json!({ "twin": "ado" })) }));
    api.merge(control).with_state(shared)
}

async fn gate(State(shared): Shr, req: Request, next: Next) -> Response {
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
        .and_then(|raw| String::from_utf8(raw).ok())
        .is_some_and(|cred| cred.split_once(':').is_some_and(|(_, pat)| !pat.is_empty()));
    if !authorized {
        let mut resp = reply(
            StatusCode::UNAUTHORIZED,
            "TF400813: the user is not authorized to access this resource",
        );
        resp.headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Basic"));
        return resp;
    }
    let versioned = req
        .uri()
        .query()
        .is_some_and(|q| q.split('&').any(|kv| kv.starts_with("api-version=")));
    if !versioned {
        return reply(StatusCode::BAD_REQUEST, "api-version is required");
    }
    {
        let mut st = shared.lock();
        if let Some(limit) = st.flags.rate_limit {
            st.requests += 1;
            if st.requests > limit {
                st.requests = 0;
                let mut resp = reply(
                    StatusCode::TOO_MANY_REQUESTS,
                    "TF400733: request budget exceeded",
                );
                resp.headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
                return resp;
            }
        }
        if req.method() != axum::http::Method::GET {
            st.mutations += 1;
        }
    }
    next.run(req).await
}

fn known_repo(repo: &str) -> Option<Response> {
    (repo != REPO_NAME).then(|| {
        reply(
            StatusCode::NOT_FOUND,
            "TF401019: the Git repository does not exist",
        )
    })
}

fn no_pr() -> Response {
    reply(
        StatusCode::NOT_FOUND,
        "TF401180: the requested pull request was not found",
    )
}

async fn list_prs(
    State(shared): Shr,
    Path((_, _, repo)): Path<(String, String, String)>,
    Query(q): Q,
) -> Response {
    if let Some(resp) = known_repo(&repo) {
        return resp;
    }
    let wanted = q
        .get("searchCriteria.status")
        .map_or("active", String::as_str);
    let top = q
        .get("$top")
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TOP);
    let skip: usize = q.get("$skip").and_then(|v| v.parse().ok()).unwrap_or(0);
    let st = shared.lock();
    let page: Vec<Value> = st
        .prs
        .iter()
        .filter(|p| wanted == "all" || p.status.name() == wanted)
        .skip(skip)
        .take(top)
        .map(|p| shared.pr_json(p))
        .collect();
    Json(json!({ "count": page.len(), "value": page })).into_response()
}

async fn get_pr(
    State(shared): Shr,
    Path((_, _, repo, id)): Path<(String, String, String, u64)>,
) -> Response {
    if let Some(resp) = known_repo(&repo) {
        return resp;
    }
    let st = shared.lock();
    st.prs
        .iter()
        .find(|p| p.id == id)
        .map_or_else(no_pr, |p| Json(shared.pr_json(p)).into_response())
}

async fn patch_pr(
    State(shared): Shr,
    Path((_, _, repo, id)): Path<(String, String, String, u64)>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(resp) = known_repo(&repo) {
        return resp;
    }
    let mut st = shared.lock();
    if !st.prs.iter().any(|p| p.id == id) {
        return no_pr();
    }
    if let (Some(target), true) = (body["targetRefName"].as_str(), st.flags.retargeting) {
        st.prs
            .iter_mut()
            .find(|p| p.id == id)
            .expect("checked")
            .target = target.to_string();
    }
    match body["status"].as_str() {
        Some("completed") => {
            if st
                .flags
                .fail_complete_after
                .is_some_and(|n| st.api_completions >= n)
            {
                st.flags.fail_complete_after = None;
                return reply(StatusCode::INTERNAL_SERVER_ERROR, "injected failure");
            }
            let expect = body["lastMergeSourceCommit"]["commitId"]
                .as_str()
                .unwrap_or_default();
            if let Err(e) = shared.complete(&mut st, id, Some(expect), "api") {
                return reply(StatusCode::CONFLICT, &e);
            }
            st.api_completions += 1;
        }
        Some("abandoned") => {
            st.prs
                .iter_mut()
                .find(|p| p.id == id)
                .expect("checked")
                .status = Status::Abandoned
        }
        _ => {}
    }
    let pr = st.prs.iter().find(|p| p.id == id).expect("checked");
    Json(shared.pr_json(pr)).into_response()
}

async fn list_statuses(
    State(shared): Shr,
    Path((_, _, repo, id)): Path<(String, String, String, u64)>,
) -> Response {
    if let Some(resp) = known_repo(&repo) {
        return resp;
    }
    let st = shared.lock();
    st.prs.iter().find(|p| p.id == id).map_or_else(no_pr, |p| {
        Json(json!({ "count": p.statuses.len(), "value": p.statuses })).into_response()
    })
}

async fn post_status(
    State(shared): Shr,
    Path((_, _, repo, id)): Path<(String, String, String, u64)>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(resp) = known_repo(&repo) {
        return resp;
    }
    let mut st = shared.lock();
    let Some(pr) = st.prs.iter_mut().find(|p| p.id == id) else {
        return no_pr();
    };
    pr.statuses.retain(|s| s["context"] != body["context"]);
    pr.statuses.push(body.clone());
    Json(body).into_response()
}

async fn add_label(
    State(shared): Shr,
    Path((_, _, repo, id)): Path<(String, String, String, u64)>,
    Json(body): Json<Value>,
) -> Response {
    if let Some(resp) = known_repo(&repo) {
        return resp;
    }
    let mut st = shared.lock();
    let Some(pr) = st.prs.iter_mut().find(|p| p.id == id) else {
        return no_pr();
    };
    let name = body["name"].as_str().unwrap_or_default().to_string();
    if name.is_empty() {
        return reply(StatusCode::BAD_REQUEST, "label name is required");
    }
    if !pr.labels.contains(&name) {
        pr.labels.push(name.clone());
    }
    Json(json!({ "name": name, "active": true })).into_response()
}

async fn remove_label(
    State(shared): Shr,
    Path((_, _, repo, id, name)): Path<(String, String, String, u64, String)>,
) -> Response {
    if let Some(resp) = known_repo(&repo) {
        return resp;
    }
    let mut st = shared.lock();
    let Some(pr) = st.prs.iter_mut().find(|p| p.id == id) else {
        return no_pr();
    };
    pr.labels.retain(|l| *l != name);
    StatusCode::OK.into_response()
}

async fn list_refs(
    State(shared): Shr,
    Path((_, _, repo)): Path<(String, String, String)>,
    Query(q): Q,
) -> Response {
    if let Some(resp) = known_repo(&repo) {
        return resp;
    }
    let prefix = format!("refs/{}", q.get("filter").map_or("heads/", String::as_str));
    let listing = shared
        .repo
        .run(
            &["for-each-ref", "--format=%(objectname) %(refname)", "refs/"],
            None,
        )
        .unwrap_or_default();
    let value: Vec<Value> = listing
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter(|(_, name)| name.starts_with(&prefix))
        .map(|(sha, name)| json!({ "name": name, "objectId": sha }))
        .collect();
    Json(json!({ "count": value.len(), "value": value })).into_response()
}

async fn queue_build(State(shared): Shr, Json(body): Json<Value>) -> Response {
    let branch = body["sourceBranch"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let Some(sha) = shared.repo.rev_parse(&branch) else {
        return reply(StatusCode::BAD_REQUEST, "sourceBranch does not exist");
    };
    let mut st = shared.lock();
    let id = st.builds.len() as u64 + 1;
    st.builds.push(Build {
        id,
        branch: branch.clone(),
        sha: sha.clone(),
        polls: 0,
    });
    st.log
        .push(json!({ "kind": "build_created", "build": id, "branch": branch, "sha": sha }));
    Json(json!({ "id": id, "status": "notStarted", "sourceBranch": branch, "sourceVersion": sha }))
        .into_response()
}

async fn get_build(State(shared): Shr, Path((_, _, id)): Path<(String, String, u64)>) -> Response {
    let mut st = shared.lock();
    let Some(build) = st.builds.iter_mut().find(|b| b.id == id) else {
        return reply(StatusCode::NOT_FOUND, "the build does not exist");
    };
    build.polls += 1;
    let (branch, sha, polls) = (build.branch.clone(), build.sha.clone(), build.polls);
    if polls == 1 {
        return Json(json!({ "id": id, "status": "inProgress", "sourceBranch": branch, "sourceVersion": sha })).into_response();
    }
    let result = if shared.repo.has_path(&sha, "BUILD_FAILS") {
        "failed"
    } else {
        "succeeded"
    };
    if polls == 2 {
        st.log
            .push(json!({ "kind": "build_completed", "build": id, "result": result, "sha": sha }));
    }
    Json(json!({ "id": id, "status": "completed", "result": result, "sourceBranch": branch, "sourceVersion": sha })).into_response()
}

async fn ctl_create_pr(State(shared): Shr, Json(body): Json<Value>) -> Response {
    let id = shared.create_pr(
        body["source"].as_str().unwrap_or_default(),
        body["target"].as_str().unwrap_or("main"),
        body["title"].as_str().unwrap_or("change"),
    );
    Json(json!({ "id": id })).into_response()
}

async fn ctl_complete(State(shared): Shr, Path(id): Path<u64>) -> Response {
    match shared.human_complete(id) {
        Ok(sha) => Json(json!({ "source_sha": sha })).into_response(),
        Err(e) => reply(StatusCode::CONFLICT, &e),
    }
}

async fn ctl_flags(State(shared): Shr, Json(body): Json<Value>) -> Response {
    shared.set_flags(&body);
    StatusCode::OK.into_response()
}

async fn ctl_log(State(shared): Shr) -> Response {
    Json(shared.log()).into_response()
}

async fn ctl_stats(State(shared): Shr) -> Response {
    Json(json!({ "mutations": shared.mutations(), "builds": shared.builds_created() }))
        .into_response()
}
