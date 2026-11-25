use adoq_core::*;
use base64::Engine as _;
use serde_json::{json, Value};
use std::time::Duration;
use ureq::http;

const API_VERSION: &str = "7.1";
const PAGE: usize = 100;
const MAX_ATTEMPTS: u32 = 3;
const BUILD_DEFINITION_ID: u64 = 1;

pub struct AdoHttp {
    agent: ureq::Agent,
    api: String,
    repo: String,
    auth: String,
}

fn is_loopback(url: &str) -> bool {
    let host = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = host.split(['/', '?']).next().unwrap_or_default();
    let host = host.rsplit_once(':').map_or(host, |(h, _)| h);
    host == "localhost" || host == "[::1]" || host.starts_with("127.")
}

fn fail(what: &str, e: impl std::fmt::Display) -> Error {
    Error(format!("{what}: {e}"))
}

impl AdoHttp {
    pub fn new(url: &str, org: &str, project: &str, repo: &str, pat: &str) -> Result<AdoHttp> {
        if std::env::var_os("ADOQ_LOOPBACK_ONLY").is_some() && !is_loopback(url) {
            return Err(Error(format!(
                "ADOQ_LOOPBACK_ONLY is set; refusing to connect to non-loopback address {url}"
            )));
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Ok(AdoHttp {
            agent,
            api: format!("{}/{org}/{project}/_apis", url.trim_end_matches('/')),
            repo: repo.to_string(),
            auth: format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("adoq:{pat}"))
            ),
        })
    }

    fn call(&self, method: &str, path: &str, query: &str, body: Option<Value>) -> Result<Value> {
        let url = format!("{}/{path}?{query}api-version={API_VERSION}", self.api);
        let payload = body.map_or_else(String::new, |b| b.to_string());
        for attempt in 1..=MAX_ATTEMPTS {
            let request = http::Request::builder()
                .method(method)
                .uri(&url)
                .header("Authorization", &self.auth)
                .header("Content-Type", "application/json")
                .body(payload.clone())
                .map_err(|e| fail("build request", e))?;
            let mut response = self
                .agent
                .run(request)
                .map_err(|e| fail(&format!("{method} {path}"), e))?;
            let status = response.status().as_u16();
            if status == 429 && attempt < MAX_ATTEMPTS {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                std::thread::sleep(Duration::from_secs(wait));
                continue;
            }
            let text = response
                .body_mut()
                .read_to_string()
                .map_err(|e| fail("read response", e))?;
            if !(200..300).contains(&status) {
                return Err(Error(format!("{method} {path} returned {status}: {text}")));
            }
            return Ok(serde_json::from_str(&text).unwrap_or(Value::Null));
        }
        unreachable!("the last attempt always returns")
    }

    fn pr_path(&self, tail: &str) -> String {
        format!("git/repositories/{}/pullrequests{tail}", self.repo)
    }
}

fn parse_pr(v: &Value) -> Result<Pr> {
    let text = |p: &Value| {
        p.as_str()
            .map(str::to_string)
            .ok_or_else(|| Error(format!("malformed PR payload: {v}")))
    };
    let state = match v["status"].as_str() {
        Some("active") => PrState::Active,
        Some("completed") => PrState::Completed,
        Some("abandoned") => PrState::Abandoned,
        other => return Err(Error(format!("unknown PR status {other:?}"))),
    };
    Ok(Pr {
        id: PrId(
            v["pullRequestId"]
                .as_u64()
                .ok_or_else(|| Error(format!("malformed PR payload: {v}")))?,
        ),
        source: Ref(text(&v["sourceRefName"])?),
        target: Ref(text(&v["targetRefName"])?),
        state,
        labels: v["labels"].as_array().map_or_else(Vec::new, |l| {
            l.iter()
                .filter_map(|l| l["name"].as_str().map(str::to_string))
                .collect()
        }),
        source_sha: Sha(text(&v["lastMergeSourceCommit"]["commitId"])?),
    })
}

impl AdoApi for AdoHttp {
    fn list_prs(&self) -> Result<Vec<Pr>> {
        let mut prs = Vec::new();
        loop {
            let query = format!("searchCriteria.status=all&$top={PAGE}&$skip={}&", prs.len());
            let page = self.call("GET", &self.pr_path(""), &query, None)?;
            let items = page["value"]
                .as_array()
                .ok_or_else(|| Error("PR list has no value array".into()))?;
            for item in items {
                prs.push(parse_pr(item)?);
            }
            if items.len() < PAGE {
                return Ok(prs);
            }
        }
    }

    fn get_pr(&self, pr: PrId) -> Result<Pr> {
        parse_pr(&self.call("GET", &self.pr_path(&format!("/{pr}")), "", None)?)
    }

    fn retarget(&self, pr: PrId, target: &Ref) -> Result<()> {
        self.call(
            "PATCH",
            &self.pr_path(&format!("/{pr}")),
            "",
            Some(json!({ "targetRefName": target.0 })),
        )
        .map(drop)
    }

    fn complete(&self, pr: PrId, source_sha: &Sha) -> Result<()> {
        let body = json!({
            "status": "completed",
            "lastMergeSourceCommit": { "commitId": source_sha.0 },
            "completionOptions": { "mergeStrategy": "noFastForward", "deleteSourceBranch": false },
        });
        self.call("PATCH", &self.pr_path(&format!("/{pr}")), "", Some(body))
            .map(drop)
    }

    fn add_label(&self, pr: PrId, label: &str) -> Result<()> {
        self.call(
            "POST",
            &self.pr_path(&format!("/{pr}/labels")),
            "",
            Some(json!({ "name": label })),
        )
        .map(drop)
    }

    fn remove_label(&self, pr: PrId, label: &str) -> Result<()> {
        self.call(
            "DELETE",
            &self.pr_path(&format!("/{pr}/labels/{label}")),
            "",
            None,
        )
        .map(drop)
    }

    fn set_status(&self, pr: PrId, check: Check, verdict: &Verdict) -> Result<()> {
        let (state, description) = match verdict {
            Verdict::Succeeded => ("succeeded", "ok".to_string()),
            Verdict::Failed(why) => ("failed", why.clone()),
        };
        let name = match check {
            Check::Stack => "stack",
            Check::Queue => "queue",
        };
        let body = json!({ "state": state, "description": description, "context": { "name": name, "genre": "adoq" } });
        self.call(
            "POST",
            &self.pr_path(&format!("/{pr}/statuses")),
            "",
            Some(body),
        )
        .map(drop)
    }
}

impl Builds for AdoHttp {
    fn queue(&self, source: &Ref) -> Result<BuildId> {
        let body = json!({ "definition": { "id": BUILD_DEFINITION_ID }, "sourceBranch": source.0 });
        let created = self.call("POST", "build/builds", "", Some(body))?;
        created["id"]
            .as_u64()
            .map(BuildId)
            .ok_or_else(|| Error(format!("build response has no id: {created}")))
    }

    fn poll(&self, build: BuildId) -> Result<BuildOutcome> {
        let state = self.call("GET", &format!("build/builds/{}", build.0), "", None)?;
        Ok(match (state["status"].as_str(), state["result"].as_str()) {
            (Some("completed"), Some("succeeded")) => BuildOutcome::Succeeded,
            (Some("completed"), _) => BuildOutcome::Failed,
            _ => BuildOutcome::Pending,
        })
    }
}
