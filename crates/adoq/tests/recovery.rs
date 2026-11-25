mod common;
use common::Env;
use serde_json::json;

#[test]
fn restart_mid_completion_never_double_completes_and_idles_quietly() {
    let env = Env::with_batch_max("recovery", 4);
    for i in 1..=4 {
        let name = format!("p{i}");
        env.branch(&name, "main", &format!("{name}.txt"), "ok\n");
        env.pr(&name, "main");
        env.enqueue(i);
    }
    env.twin
        .shared
        .set_flags(&json!({ "fail_complete_after": 2 }));

    let mut crashed = false;
    for _ in 0..10 {
        if !env.tick().ok {
            crashed = true;
            break;
        }
    }
    assert!(crashed, "the injected API failure interrupts the tick");
    assert_eq!(
        env.completions().len(),
        2,
        "two PRs landed before the interruption"
    );

    env.tick_until(|e| e.completions().len() == 4);

    let mut done = env.completions();
    done.sort();
    assert_eq!(done, [1, 2, 3, 4], "every PR completed exactly once");
    assert_eq!(
        env.twin
            .shared
            .repo
            .first_parent_subjects("main")
            .iter()
            .filter(|s| s.starts_with("Merged PR"))
            .count(),
        4
    );
    assert_eq!(
        env.twin.shared.builds_created(),
        1,
        "the green build was reused after the restart"
    );

    env.tick_ok();
    let (mutations, refs) = (env.twin.shared.mutations(), env.refs());
    let events = env.tick_ok();
    assert_eq!(
        env.twin.shared.mutations(),
        mutations,
        "second tick on unchanged state makes no mutating API call"
    );
    assert_eq!(env.refs(), refs, "and pushes nothing");
    assert_eq!(events.len(), 1);
}
