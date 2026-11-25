mod common;
use common::Env;
use serde_json::Value;
use std::collections::BTreeSet;

fn fields(kind: &str) -> BTreeSet<&'static str> {
    let names: &[&str] = match kind {
        "tick" => &["ms", "prs"],
        "retarget" => &["pr", "ok"],
        "rebase" => &["pr", "conflict", "ms"],
        "batch" => &["size", "result", "builds"],
        "merge" => &["pr", "wait_s"],
        other => panic!("undocumented telemetry kind {other}"),
    };
    ["kind", "ts"].iter().chain(names).copied().collect()
}

#[test]
fn scenario_runs_append_documented_json_lines() {
    let env = Env::with_batch_max("telemetry", 2);
    env.branch("a", "main", "a.txt", "a\n");
    env.branch("b", "a", "b.txt", "b\n");
    env.branch("q", "main", "q.txt", "q\n");
    let a = env.pr("a", "main");
    env.pr("b", "a");
    let q = env.pr("q", "main");
    env.twin.shared.human_complete(a).unwrap();
    env.enqueue(q);
    env.tick_until(|e| e.completions().len() == 2);

    let text = std::fs::read_to_string(env.data.join("telemetry.jsonl")).expect("telemetry file");
    let mut kinds = BTreeSet::new();
    for line in text.lines() {
        let v: Value = serde_json::from_str(line).expect("each line is JSON");
        let kind = v["kind"].as_str().expect("kind").to_string();
        let keys: BTreeSet<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, fields(&kind), "line {line}");
        kinds.insert(kind);
    }
    assert_eq!(
        kinds,
        ["batch", "merge", "rebase", "retarget", "tick"]
            .map(String::from)
            .into_iter()
            .collect()
    );
    assert!(!text.contains('@'), "no email-like personal data");
}
