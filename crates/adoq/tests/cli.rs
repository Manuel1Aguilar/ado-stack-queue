mod common;
use common::Env;
use serde_json::Value;

#[test]
fn ls_lists_stacks_queue_and_batches() {
    let env = Env::new("cli-ls");
    env.branch("a", "main", "a.txt", "a\n");
    env.branch("b", "a", "b.txt", "b\n");
    env.branch("q", "main", "q.txt", "q\n");
    env.pr("a", "main");
    env.pr("b", "a");
    let q = env.pr("q", "main");

    let out = env.adoq(&["do", "Enqueue", &format!("{{\"pr\":{q}}}")]);
    assert!(out.status.success());
    assert_eq!(env.pr_json(q)["labels"][0]["name"], "queue");
    let ls = env.adoq(&["ls"]);
    assert!(ls.status.success());
    let view: Value = serde_json::from_slice(&ls.stdout).expect("ls prints JSON");

    assert_eq!(view["stacks"], serde_json::json!([[1, 2]]));
    assert_eq!(view["queue"][0]["pr"], q);
    assert_eq!(view["batches"], serde_json::json!([]));
    assert_eq!(view["paused"], false);
}

#[test]
fn invalid_action_exits_nonzero_listing_the_actions() {
    let env = Env::new("cli-invalid");
    for args in [
        &["do", "Frobnicate"][..],
        &["do", "Enqueue"][..],
        &["do", "Enqueue", "{\"pr\":\"x\"}"][..],
    ] {
        let out = env.adoq(args);
        assert!(!out.status.success(), "{args:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        for action in ["Tick", "Enqueue", "Dequeue", "Pause", "Resume"] {
            assert!(
                err.contains(action),
                "{args:?} stderr lacks {action}: {err}"
            );
        }
    }
}

#[test]
fn pause_dequeue_and_resume_drive_the_queue() {
    let env = Env::with_batch_max("cli-pause", 4);
    env.branch("p1", "main", "p1.txt", "1\n");
    env.branch("p2", "main", "p2.txt", "2\n");
    let (p1, p2) = (env.pr("p1", "main"), env.pr("p2", "main"));
    env.enqueue(p1);
    env.enqueue(p2);

    assert!(env.adoq(&["do", "Pause"]).status.success());
    env.tick_ok();
    assert_eq!(
        env.twin.shared.builds_created(),
        0,
        "a paused queue builds nothing"
    );

    assert!(env
        .adoq(&["do", "Dequeue", &format!("{{\"pr\":{p2}}}")])
        .status
        .success());
    assert!(env.adoq(&["do", "Resume"]).status.success());
    env.tick_until(|e| !e.completions().is_empty());

    assert_eq!(env.completions(), [p1]);
    assert!(env.pr_json(p2)["labels"].as_array().unwrap().is_empty());
    assert_eq!(env.pr_json(p2)["status"], "active");
}
