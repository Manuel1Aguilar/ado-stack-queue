mod common;
use common::Env;
use serde_json::json;

fn stack_of_three(env: &Env) -> (u64, u64, u64) {
    env.branch("a", "main", "a.txt", "a\n");
    env.branch("b", "a", "b.txt", "b\n");
    env.branch("c", "b", "c.txt", "c\n");
    (env.pr("a", "main"), env.pr("b", "a"), env.pr("c", "b"))
}

#[test]
fn chain_of_three_resolves_across_ticks() {
    let env = Env::new("stack-chain");
    let (a, b, c) = stack_of_three(&env);

    env.tick_ok();
    assert_eq!(
        env.pr_json(b)["targetRefName"],
        "refs/heads/a",
        "nothing happens while the parent is open"
    );

    env.twin.shared.human_complete(a).unwrap();
    let events = env.tick_ok();
    assert!(events
        .iter()
        .any(|e| e["kind"] == "retarget" && e["pr"] == b && e["ok"] == true));
    assert_eq!(env.pr_json(b)["targetRefName"], "refs/heads/main");
    assert_eq!(
        env.git(&["rev-parse", "refs/heads/b~1"]),
        env.sha("main"),
        "b is rebased onto the new main tip"
    );
    assert_eq!(
        env.git(&["log", "--format=%s", "refs/heads/main..refs/heads/b"]),
        "add b",
        "parent commit not duplicated"
    );
    assert_eq!(
        env.git(&[
            "rev-list",
            "--count",
            "--merges",
            "refs/heads/main..refs/heads/b"
        ])
        .parse::<usize>()
        .unwrap(),
        0
    );
    assert_eq!(
        env.git(&["rev-parse", "refs/heads/c~1"]),
        env.sha("b"),
        "c follows the rewritten b"
    );
    assert_eq!(env.pr_json(c)["targetRefName"], "refs/heads/b");
    assert_eq!(env.status_of(b, "stack").unwrap()["state"], "succeeded");

    env.twin.shared.human_complete(b).unwrap();
    env.tick_ok();
    assert_eq!(env.pr_json(c)["targetRefName"], "refs/heads/main");
    assert_eq!(
        env.git(&["log", "--format=%s", "refs/heads/main..refs/heads/c"]),
        "add c"
    );
    assert_eq!(env.git(&["rev-parse", "refs/heads/c~1"]), env.sha("main"));
}

#[test]
fn conflict_leaves_child_untouched_and_continues() {
    let env = Env::new("stack-conflict");
    env.branch("a", "main", "g.txt", "from a\n");
    env.branch("b", "a", "f.txt", "from b\n");
    env.branch("a2", "main", "h.txt", "from a2\n");
    env.branch("b2", "a2", "i.txt", "from b2\n");
    let (a, b) = (env.pr("a", "main"), env.pr("b", "a"));
    let (a2, b2) = (env.pr("a2", "main"), env.pr("b2", "a2"));
    env.git(&["update-ref", "refs/heads/other", &env.sha("main")]);
    env.branch("other", "main", "f.txt", "from someone else\n");
    env.twin
        .shared
        .human_complete(env.pr("other", "main"))
        .unwrap();
    env.twin.shared.human_complete(a).unwrap();
    env.twin.shared.human_complete(a2).unwrap();
    let before = env.sha("b");

    let events = env.tick_ok();

    assert_eq!(env.sha("b"), before, "child branch is not force-pushed");
    let status = env.status_of(b, "stack").unwrap();
    assert_eq!(status["state"], "failed");
    assert!(
        status["description"].as_str().unwrap().contains("conflict"),
        "{status}"
    );
    assert!(events
        .iter()
        .any(|e| e["kind"] == "rebase" && e["pr"] == b && e["conflict"] == true));
    assert_eq!(
        env.git(&["rev-parse", "refs/heads/b2~1"]),
        env.sha("main"),
        "the sibling stack is still rebased"
    );
    assert_eq!(env.status_of(b2, "stack").unwrap()["state"], "succeeded");
}

#[test]
fn retarget_off_posts_failed_and_does_not_rebase() {
    let env = Env::new("stack-retarget-off");
    let (a, b, _) = stack_of_three(&env);
    env.twin.shared.set_flags(&json!({ "retargeting": false }));
    env.twin.shared.human_complete(a).unwrap();
    let before = env.sha("b");

    let events = env.tick_ok();

    assert_eq!(
        env.pr_json(b)["targetRefName"],
        "refs/heads/a",
        "twin ignored the PATCH"
    );
    assert_eq!(env.sha("b"), before, "no rebase");
    assert_eq!(env.status_of(b, "stack").unwrap()["state"], "failed");
    assert!(events
        .iter()
        .any(|e| e["kind"] == "retarget" && e["pr"] == b && e["ok"] == false));
    let mutations = env.twin.shared.mutations();
    env.tick_ok();
    assert_eq!(
        env.twin.shared.mutations(),
        mutations,
        "a known failure is not retried until the branch changes"
    );
}

#[test]
fn first_run_against_empty_remote_still_rebases_later() {
    let env = Env::new("stack-empty-first");
    env.git(&["update-ref", "-d", "refs/heads/main"]);
    env.tick_ok();

    env.twin
        .shared
        .repo
        .commit("main", None, &[("README", "base\n")], "init");
    env.branch("a", "main", "a.txt", "a\n");
    env.branch("b", "a", "b.txt", "b\n");
    let (a, b) = (env.pr("a", "main"), env.pr("b", "a"));
    env.twin.shared.human_complete(a).unwrap();
    env.tick_ok();

    assert_eq!(env.pr_json(b)["targetRefName"], "refs/heads/main");
    assert_eq!(env.status_of(b, "stack").unwrap()["state"], "succeeded");
    assert_eq!(env.git(&["rev-parse", "refs/heads/b~1"]), env.sha("main"));
}
