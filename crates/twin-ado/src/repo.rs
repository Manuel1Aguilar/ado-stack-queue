use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct Repo {
    pub path: PathBuf,
}

pub enum Merge {
    Done(String),
    Conflict,
}

impl Repo {
    pub fn init(path: &Path) -> Repo {
        std::fs::create_dir_all(path).expect("create repo dir");
        let repo = Repo {
            path: path.to_path_buf(),
        };
        repo.run(
            &["init", "--bare", "--quiet", "--initial-branch=main"],
            None,
        )
        .expect("git init");
        repo
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("--git-dir").arg(&self.path).args(args);
        cmd.env("GIT_AUTHOR_NAME", "adoq-twin")
            .env("GIT_AUTHOR_EMAIL", "twin@example.invalid")
            .env("GIT_COMMITTER_NAME", "adoq-twin")
            .env("GIT_COMMITTER_EMAIL", "twin@example.invalid");
        cmd
    }

    fn output(&self, args: &[&str], stdin: Option<&str>) -> std::io::Result<std::process::Output> {
        let mut cmd = self.command(args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        if stdin.is_some() {
            cmd.stdin(Stdio::piped());
        }
        let mut child = cmd.spawn()?;
        if let Some(input) = stdin {
            child
                .stdin
                .take()
                .expect("stdin")
                .write_all(input.as_bytes())?;
        }
        child.wait_with_output()
    }

    pub fn run(&self, args: &[&str], stdin: Option<&str>) -> Result<String, String> {
        let out = self.output(args, stdin).map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            Err(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    pub fn rev_parse(&self, refname: &str) -> Option<String> {
        self.run(
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{refname}^{{commit}}"),
            ],
            None,
        )
        .ok()
    }

    pub fn commit(
        &self,
        branch: &str,
        base: Option<&str>,
        files: &[(&str, &str)],
        message: &str,
    ) -> String {
        let mut stream = format!(
            "commit refs/heads/{branch}\ncommitter adoq-twin <twin@example.invalid> 1700000000 +0000\ndata {}\n{message}\n",
            message.len()
        );
        if let Some(base) = base {
            stream.push_str(&format!("from {base}\n"));
        }
        for (path, content) in files {
            stream.push_str(&format!(
                "M 100644 inline {path}\ndata {}\n{content}\n",
                content.len()
            ));
        }
        self.run(&["fast-import", "--quiet", "--force"], Some(&stream))
            .expect("git fast-import");
        self.rev_parse(branch).expect("branch exists after commit")
    }

    pub fn has_path(&self, commit: &str, path: &str) -> bool {
        self.run(&["cat-file", "-e", &format!("{commit}:{path}")], None)
            .is_ok()
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        self.output(&["merge-base", "--is-ancestor", ancestor, descendant], None)
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn merge_into(
        &self,
        target: &str,
        source_sha: &str,
        message: &str,
    ) -> Result<Merge, String> {
        let target_ref = format!("refs/heads/{target}");
        let tip = self
            .rev_parse(&target_ref)
            .ok_or_else(|| format!("target {target_ref} does not exist"))?;
        let out = self
            .output(
                &[
                    "merge-tree",
                    "--write-tree",
                    "--no-messages",
                    &tip,
                    source_sha,
                ],
                None,
            )
            .map_err(|e| e.to_string())?;
        if out.status.code() == Some(1) {
            return Ok(Merge::Conflict);
        }
        let tree = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        let merged = self.run(
            &[
                "commit-tree",
                &tree,
                "-p",
                &tip,
                "-p",
                source_sha,
                "-m",
                message,
            ],
            None,
        )?;
        self.run(&["update-ref", &target_ref, &merged, &tip], None)?;
        Ok(Merge::Done(merged))
    }

    pub fn first_parent_subjects(&self, branch: &str) -> Vec<String> {
        self.run(
            &[
                "log",
                "--first-parent",
                "--format=%s",
                &format!("refs/heads/{branch}"),
            ],
            None,
        )
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
    }
}
