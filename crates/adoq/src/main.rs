mod ado_http;
mod bench;
mod git_cli;
mod journal;
mod store_sqlite;

use adoq_core::*;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "adoq",
    about = "Stacked PR keeper and merge queue for Azure DevOps"
)]
struct Cli {
    #[arg(long, env = "ADOQ_URL", global = true)]
    ado_url: Option<String>,
    #[arg(long, env = "ADOQ_ORG", default_value = "org", global = true)]
    org: String,
    #[arg(long, env = "ADOQ_PROJECT", default_value = "proj", global = true)]
    project: String,
    #[arg(long, env = "ADOQ_REPO", default_value = "repo", global = true)]
    repo: String,
    #[arg(long, env = "ADOQ_REMOTE", global = true)]
    remote: Option<String>,
    #[arg(long, env = "DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,
    #[arg(long, env = "ADOQ_BATCH_MAX", default_value_t = 8, global = true)]
    batch_max: usize,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run {
        #[arg(long)]
        once: bool,
        #[arg(long, default_value_t = 30)]
        interval_secs: u64,
    },
    Ls,
    Do {
        action: String,
        payload: Option<String>,
    },
    Bench,
}

pub struct Settings {
    pub url: String,
    pub org: String,
    pub project: String,
    pub repo: String,
    pub remote: String,
    pub data_dir: PathBuf,
    pub batch_max: usize,
    pub pat: String,
}

impl Settings {
    fn from_cli(cli: &Cli) -> std::result::Result<Settings, String> {
        let need =
            |v: &Option<String>, name: &str| v.clone().ok_or_else(|| format!("{name} is required"));
        Ok(Settings {
            url: need(&cli.ado_url, "--ado-url or ADOQ_URL")?,
            org: cli.org.clone(),
            project: cli.project.clone(),
            repo: cli.repo.clone(),
            remote: need(&cli.remote, "--remote or ADOQ_REMOTE")?,
            data_dir: cli
                .data_dir
                .clone()
                .ok_or("--data-dir or DATA_DIR is required")?,
            batch_max: cli.batch_max,
            pat: std::env::var("ADOQ_PAT").map_err(|_| "ADOQ_PAT is not set")?,
        })
    }
}

pub fn build_engine(s: &Settings, events: Box<dyn Events>) -> Result<Engine> {
    std::fs::create_dir_all(&s.data_dir)
        .map_err(|e| Error(format!("cannot create {}: {e}", s.data_dir.display())))?;
    let http = ado_http::AdoHttp::new(&s.url, &s.org, &s.project, &s.repo, &s.pat)?;
    let builds = ado_http::AdoHttp::new(&s.url, &s.org, &s.project, &s.repo, &s.pat)?;
    Ok(Engine {
        ado: Box::new(http),
        git: Box::new(git_cli::GitCli::new(s.data_dir.join("work"), &s.remote)?),
        builds: Box::new(builds),
        store: Box::new(store_sqlite::SqliteStore::new(&s.data_dir.join("adoq.db"))?),
        events,
        clock: Box::new(journal::SystemClock),
        config: Config {
            batch_max: s.batch_max,
        },
    })
}

fn engine(cli: &Cli) -> std::result::Result<Engine, String> {
    let settings = Settings::from_cli(cli)?;
    std::fs::create_dir_all(&settings.data_dir).map_err(|e| e.to_string())?;
    let journal = journal::Journal::new(&settings.data_dir).map_err(|e| e.to_string())?;
    build_engine(&settings, Box::new(journal)).map_err(|e| e.to_string())
}

fn run(cli: Cli) -> std::result::Result<(), String> {
    match &cli.command {
        Command::Bench => bench::run().map_err(|e| e.to_string()),
        Command::Do { action, payload } => {
            let action = Action::parse(action, payload.as_deref())?;
            engine(&cli)?.apply(action).map_err(|e| e.to_string())
        }
        Command::Ls => {
            let view = engine(&cli)?.view().map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string(&view).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        Command::Run {
            once,
            interval_secs,
        } => {
            let engine = engine(&cli)?;
            loop {
                let result = engine.tick();
                if *once {
                    return result.map_err(|e| e.to_string());
                }
                if let Err(e) = result {
                    eprintln!("tick failed: {e}");
                }
                std::thread::sleep(Duration::from_secs(*interval_secs));
            }
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("adoq: {message}");
            ExitCode::FAILURE
        }
    }
}
