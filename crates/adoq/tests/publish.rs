//! Release checks: README copy, license and the publishable-tree scan.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(root().join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

fn run_in(dir: &Path, program: &str, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("run {program}: {e}"))
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn readme_passes_copy_lint() {
    let out = run_in(
        &root(),
        "python3",
        &["-I", "scripts/copy-lint.py", "README.md"],
    );
    assert!(out.status.success(), "copy-lint hits:\n{}", text(&out));
}

#[test]
fn readme_has_required_sections() {
    let readme = read("README.md");
    let headings: Vec<String> = readme
        .lines()
        .filter(|l| l.starts_with('#'))
        .map(|l| l.to_lowercase())
        .collect();
    for want in ["install", "quick start", "configuration", "compares"] {
        assert!(
            headings.iter().any(|h| h.contains(want)),
            "README needs a heading containing {want:?}; has {headings:?}"
        );
    }
    let quick = readme
        .to_lowercase()
        .split("## quick start")
        .nth(1)
        .expect("quick start section")
        .split("\n## ")
        .next()
        .unwrap()
        .to_string();
    assert!(quick.contains("```"), "quick start needs a code fence");
    assert!(quick.contains("adoq"), "quick start must show an adoq command");
}

#[test]
fn readme_has_no_internal_jargon() {
    let readme = read("README.md").to_lowercase();
    let banned = [
        "c0008", "delivery", "deliveries", "factory", "tailnet", "server", "/mnt/",
    ];
    for word in banned {
        assert!(!readme.contains(word), "README must not mention {word:?}");
    }
}

#[test]
fn license_is_mit() {
    let license = read("LICENSE");
    assert!(license.starts_with("MIT License"), "LICENSE must start with 'MIT License'");
    assert!(license.contains("Permission is hereby granted, free of charge"));
    let cargo = read("Cargo.toml");
    assert!(
        cargo.lines().any(|l| l.trim() == "license = \"MIT\""),
        "workspace Cargo.toml must declare license = \"MIT\""
    );
}

#[test]
fn check_publishable_passes() {
    let out = run_in(&root(), "bash", &["scripts/check-publishable.sh"]);
    assert!(out.status.success(), "check-publishable failed:\n{}", text(&out));
}

#[test]
fn check_publishable_catches_leaks() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("leaks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = root().join("scripts/check-publishable.sh");
    let script = script.to_str().unwrap();

    let key = format!("{}BEGIN RSA PRIVATE KEY{}\n", "-".repeat(5), "-".repeat(5));
    std::fs::write(dir.join("key.txt"), key).unwrap();
    std::fs::write(
        dir.join("net.txt"),
        format!("host 100.{}.3.4\npath {}{}data\n", 64, "/mnt/HC", "_Volume_1/"),
    )
    .unwrap();
    std::fs::write(dir.join("mail.txt"), format!("ask jane.doe@{}.com\n", "gmail")).unwrap();
    std::fs::write(dir.join("ok.txt"), "bot noreply@example.com, 100.63.0.1\n").unwrap();

    for args in [&["init", "-q"][..], &["add", "."][..]] {
        let out = run_in(&dir, "git", args);
        assert!(out.status.success(), "git {args:?}: {}", text(&out));
    }
    let out = run_in(&dir, "bash", &[script]);
    let report = text(&out);
    assert!(!out.status.success(), "scan should fail on leaks:\n{report}");
    for file in ["key.txt", "net.txt", "mail.txt"] {
        assert!(report.contains(file), "report should name {file}:\n{report}");
    }
    assert!(!report.contains("ok.txt"), "allowed lines must not be flagged:\n{report}");
}
