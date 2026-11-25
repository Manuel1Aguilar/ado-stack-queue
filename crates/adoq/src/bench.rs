use crate::{build_engine, Settings};
use adoq_core::*;
use std::time::Instant;
use twin_ado::Twin;

const STACKS: usize = 10;
const DEPTH: usize = 5;
const TICKS: usize = 20;
const BATCH: usize = 4;

struct Quiet;

impl Events for Quiet {
    fn emit(&self, _: &Event) -> Result<()> {
        Ok(())
    }
}

fn rss_mb() -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kb: f64 = status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or(0.0);
    kb / 1024.0
}

pub fn run() -> Result<()> {
    let root = std::env::var_os("DATA_DIR")
        .map_or_else(|| std::path::PathBuf::from(".adoq-bench"), Into::into)
        .join(format!("bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let twin = Twin::start(&root.join("twin"));
    let repo = &twin.shared.repo;
    let main = repo.commit("main", None, &[("README", "bench\n")], "init");
    for s in 0..STACKS {
        for d in 0..DEPTH {
            let branch = format!("s{s}-{d}");
            let (base, target) = if d == 0 {
                (main.clone(), "main".to_string())
            } else {
                (
                    repo.rev_parse(&format!("s{s}-{}", d - 1))
                        .unwrap_or_default(),
                    format!("s{s}-{}", d - 1),
                )
            };
            repo.commit(
                &branch,
                Some(&base),
                &[(&format!("{branch}.txt"), &branch)],
                &format!("add {branch}"),
            );
            twin.shared.create_pr(&branch, &target, &branch);
        }
    }
    println!("fixture: {} stacked PRs in {STACKS} stacks", STACKS * DEPTH);
    let settings = Settings {
        url: twin.url(),
        org: "org".into(),
        project: "proj".into(),
        repo: "repo".into(),
        remote: twin.shared.repo.path.display().to_string(),
        data_dir: root.join("adoq"),
        batch_max: BATCH,
        pat: "bench-pat".into(),
    };
    let engine = build_engine(&settings, Box::new(Quiet))?;
    let mut ms = Vec::new();
    for _ in 0..TICKS {
        let start = Instant::now();
        engine.tick()?;
        ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    ms.sort_by(|a, b| a.total_cmp(b));
    let tick_p50_ms = ms[ms.len() / 2];
    println!("idle tick p50 over {TICKS} ticks: {tick_p50_ms:.2} ms");

    let builds_before = twin.shared.builds_created();
    let mut queued = Vec::new();
    for n in 0..BATCH {
        let branch = format!("q{n}");
        let base = repo.rev_parse("main").unwrap_or_default();
        repo.commit(
            &branch,
            Some(&base),
            &[(&format!("{branch}.txt"), &branch)],
            &format!("add {branch}"),
        );
        let id = twin.shared.create_pr(&branch, "main", &branch);
        engine.apply(Action::Enqueue { pr: PrId(id) })?;
        queued.push(id);
    }
    for _ in 0..30 {
        engine.tick()?;
        if twin
            .shared
            .log()
            .iter()
            .filter(|e| e["kind"] == "pr_completed")
            .count()
            >= BATCH
        {
            break;
        }
    }
    let merged = twin
        .shared
        .log()
        .iter()
        .filter(|e| e["kind"] == "pr_completed")
        .count();
    let builds = twin.shared.builds_created() - builds_before;
    if merged != BATCH {
        return Err(Error(format!("bench batch merged {merged} of {BATCH} PRs")));
    }
    let builds_per_merged_pr = builds as f64 / merged as f64;
    println!("batch of {BATCH} green PRs: {builds} build(s), {merged} merged");
    let rss_mb = rss_mb();
    drop(engine);
    drop(twin);
    let _ = std::fs::remove_dir_all(&root);
    println!(
        "{}",
        serde_json::json!({ "tick_p50_ms": tick_p50_ms, "rss_mb": rss_mb, "builds_per_merged_pr": builds_per_merged_pr })
    );
    Ok(())
}
