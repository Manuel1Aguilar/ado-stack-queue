use adoq_core::*;
use std::path::PathBuf;
use std::process::{Command, Output};

pub struct GitCli {
    dir: PathBuf,
}

impl GitCli {
    pub fn new(dir: PathBuf, remote: &str) -> Result<GitCli> {
        let git = GitCli { dir };
        if !git.dir.join(".git").exists() {
            let out = Command::new("git")
                .arg("clone")
                .arg("--quiet")
                .arg(remote)
                .arg(&git.dir)
                .output()
                .map_err(io)?;
            check(&out, "clone")?;
        }
        Ok(git)
    }

    fn run(&self, args: &[&str]) -> Result<Output> {
        Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "adoq")
            .env("GIT_AUTHOR_EMAIL", "adoq@example.invalid")
            .env("GIT_COMMITTER_NAME", "adoq")
            .env("GIT_COMMITTER_EMAIL", "adoq@example.invalid")
            .output()
            .map_err(io)
    }

    fn ok(&self, args: &[&str]) -> Result<String> {
        let out = self.run(args)?;
        check(&out, args[0])?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn sync(&self) -> Result<()> {
        let _ = self.run(&["rebase", "--abort"]);
        self.ok(&[
            "fetch",
            "--quiet",
            "--prune",
            "origin",
            "+refs/heads/*:refs/remotes/origin/*",
        ])
        .map(drop)
    }
}

fn io(e: std::io::Error) -> Error {
    Error(format!("cannot run git: {e}"))
}

fn check(out: &Output, what: &str) -> Result<()> {
    if out.status.success() {
        Ok(())
    } else {
        Err(Error(format!(
            "git {what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

impl Git for GitCli {
    fn rebase_onto(
        &self,
        child: &Ref,
        new_base: &Ref,
        old_base: &Sha,
        expected_tip: &Sha,
    ) -> Result<Rebase> {
        self.sync()?;
        let child_ref = format!("origin/{}", child.short());
        let base_ref = format!("origin/{}", new_base.short());
        // A clone of an empty remote has an unborn HEAD; rebase cannot start from it.
        self.ok(&["checkout", "--quiet", "--detach", "--force", &child_ref])?;
        let out = self.run(&["rebase", "--onto", &base_ref, &old_base.0, &child_ref])?;
        if !out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let conflicts: Vec<&str> = stdout
                .lines()
                .filter(|l| l.starts_with("CONFLICT"))
                .collect();
            if conflicts.is_empty() {
                return Err(Error(format!(
                    "git rebase failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
            let _ = self.run(&["rebase", "--abort"]);
            return Ok(Rebase::Conflict {
                message: format!(
                    "rebasing {} onto {} hit a conflict: {}",
                    child.short(),
                    new_base.short(),
                    conflicts.join("; ")
                ),
            });
        }
        let new_sha = self.ok(&["rev-parse", "HEAD"])?;
        let lease = format!(
            "--force-with-lease=refs/heads/{}:{}",
            child.short(),
            expected_tip.0
        );
        self.ok(&[
            "push",
            "--quiet",
            &lease,
            "origin",
            &format!("HEAD:refs/heads/{}", child.short()),
        ])?;
        Ok(Rebase::Done {
            new_sha: Sha(new_sha),
        })
    }

    fn build_queue_ref(&self, target: &Ref, members: &[Member], name: &Ref) -> Result<QueueRef> {
        self.sync()?;
        let mut tip = self.ok(&["rev-parse", &format!("origin/{}", target.short())])?;
        for member in members {
            let out = self.run(&[
                "merge-tree",
                "--write-tree",
                "--no-messages",
                &tip,
                &member.sha.0,
            ])?;
            match out.status.code() {
                Some(0) => {}
                Some(1) => return Ok(QueueRef::Conflict { pr: member.pr }),
                _ => {
                    return Err(Error(format!(
                        "git merge-tree failed: {}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    )))
                }
            }
            let stdout = String::from_utf8_lossy(&out.stdout);
            let tree = stdout.lines().next().unwrap_or_default();
            tip = self.ok(&[
                "commit-tree",
                tree,
                "-p",
                &tip,
                "-p",
                &member.sha.0,
                "-m",
                &format!("queue: PR {}", member.pr),
            ])?;
        }
        self.ok(&[
            "push",
            "--quiet",
            "--force",
            "origin",
            &format!("{tip}:{}", name.0),
        ])?;
        Ok(QueueRef::Built)
    }

    fn delete_ref(&self, name: &Ref) -> Result<()> {
        self.ok(&["push", "--quiet", "origin", &format!(":{}", name.0)])
            .map(drop)
    }
}
