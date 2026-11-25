mod common;
use common::Env;

fn independent_prs(env: &Env, n: usize, culprit: Option<usize>) -> Vec<u64> {
    (1..=n)
        .map(|i| {
            let name = format!("p{i}");
            let (file, body) = if culprit == Some(i) {
                ("BUILD_FAILS", "x\n")
            } else {
                ("ok.txt", "ok\n")
            };
            env.branch(&name, "main", &format!("{name}-{file}"), body);
            if culprit == Some(i) {
                env.branch(&name, &name, "BUILD_FAILS", "x\n");
            }
            env.pr(&name, "main")
        })
        .collect()
}

fn merged_order(env: &Env) -> Vec<String> {
    let mut subjects = env.twin.shared.repo.first_parent_subjects("main");
    subjects.reverse();
    subjects
        .into_iter()
        .filter(|s| s.starts_with("Merged PR"))
        .collect()
}

#[test]
fn serial_queue_merges_fifo_after_green_builds() {
    let env = Env::with_batch_max("queue-serial", 1);
    let prs = independent_prs(&env, 3, None);
    for pr in [prs[1], prs[0], prs[2]] {
        env.enqueue(pr);
    }

    env.tick_until(|e| e.completions().len() == 3);

    assert_eq!(
        merged_order(&env),
        ["Merged PR 2: p2", "Merged PR 1: p1", "Merged PR 3: p3"]
    );
    assert_eq!(env.twin.shared.builds_created(), 3);
    let log = env.twin.shared.log();
    for (i, entry) in log
        .iter()
        .enumerate()
        .filter(|(_, e)| e["kind"] == "pr_completed")
    {
        let source = env.sha(&format!("p{}", entry["pr"]));
        let proven = log[..i].iter().any(|b| {
            b["kind"] == "build_completed"
                && b["result"] == "succeeded"
                && env
                    .twin
                    .shared
                    .repo
                    .is_ancestor(&source, b["sha"].as_str().unwrap())
        });
        assert!(
            proven,
            "PR {} completed before a green build of its merge result",
            entry["pr"]
        );
    }
}

#[test]
fn batch_bisect_four_green_prs_share_one_build() {
    let env = Env::with_batch_max("queue-batch-green", 4);
    let prs = independent_prs(&env, 4, None);
    prs.iter().for_each(|pr| env.enqueue(*pr));

    env.tick_until(|e| e.completions().len() == 4);

    assert!(
        env.twin.shared.builds_created() <= 2,
        "builds: {}",
        env.twin.shared.builds_created()
    );
    assert_eq!(merged_order(&env).len(), 4);
}

#[test]
fn batch_bisect_one_red_pr_is_evicted_and_the_rest_merge() {
    for culprit in 1..=4 {
        let env = Env::with_batch_max(&format!("queue-batch-red{culprit}"), 4);
        let prs = independent_prs(&env, 4, Some(culprit));
        prs.iter().for_each(|pr| env.enqueue(*pr));

        env.tick_until(|e| {
            e.completions().len() == 3 && e.status_of(culprit as u64, "queue").is_some()
        });

        let mut done = env.completions();
        done.sort();
        let expected: Vec<u64> = (1..=4).filter(|i| *i != culprit as u64).collect();
        assert_eq!(done, expected, "culprit {culprit}");
        assert!(
            env.twin.shared.builds_created() <= 5,
            "culprit {culprit}: {} builds",
            env.twin.shared.builds_created()
        );
        assert_eq!(
            env.status_of(culprit as u64, "queue").unwrap()["state"],
            "failed"
        );
        assert!(
            env.pr_json(culprit as u64)["labels"]
                .as_array()
                .unwrap()
                .is_empty(),
            "culprit leaves the queue"
        );
        assert!(!env
            .twin
            .shared
            .repo
            .has_path("refs/heads/main", "BUILD_FAILS"));
    }
}
