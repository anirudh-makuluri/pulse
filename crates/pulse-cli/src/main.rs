//! Pulse CLI — direct SQLite access for PR2 (service/IPC comes in PR3).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use pulse_core::{
    load_config, open_db, NewTask, PulseError, PulsePaths, Store, Task, TaskStatus, TaskUpdate,
};
use uuid::Uuid;

/// Exit codes per design: 0 ok, 1 user/logic, 2 service (unused until PR3), 3 DB.
const EXIT_OK: u8 = 0;
const EXIT_USER: u8 = 1;
const EXIT_DB: u8 = 3;

#[derive(Parser, Debug)]
#[command(
    name = "pulse",
    version,
    about = "Pulse — local-first todo that stays current",
    long_about = None
)]
struct Cli {
    /// Override data directory (default: %LOCALAPPDATA%\\Pulse)
    #[arg(long, global = true, value_name = "DIR")]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Manage tasks
    Tasks {
        #[command(subcommand)]
        command: TasksCmd,
    },
    /// Show or locate config
    Config {
        #[command(subcommand)]
        command: ConfigCmd,
    },
    /// Print version
    Version,
}

#[derive(Subcommand, Debug)]
enum TasksCmd {
    /// List tasks
    List {
        #[arg(long, value_enum)]
        status: Option<StatusArg>,
        /// Emit JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Show one task
    Show {
        /// Task id (full UUID or unique prefix)
        id: String,
    },
    /// Add a task
    Add {
        /// Task title
        title: Vec<String>,
        /// Place in Today instead of Inbox
        #[arg(long)]
        today: bool,
        #[arg(long)]
        notes: Option<String>,
    },
    /// Mark a task done
    Done {
        id: String,
    },
    /// Update task fields
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, value_enum)]
        status: Option<StatusArg>,
        #[arg(long)]
        notes: Option<String>,
    },
    /// Move a task to a status
    Move {
        id: String,
        #[arg(value_enum)]
        status: StatusArg,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    /// Print resolved config as TOML
    Show,
    /// Print config file path
    Path,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StatusArg {
    Inbox,
    Today,
    Next,
    Waiting,
    Done,
}

impl From<StatusArg> for TaskStatus {
    fn from(s: StatusArg) -> Self {
        match s {
            StatusArg::Inbox => TaskStatus::Inbox,
            StatusArg::Today => TaskStatus::Today,
            StatusArg::Next => TaskStatus::Next,
            StatusArg::Waiting => TaskStatus::Waiting,
            StatusArg::Done => TaskStatus::Done,
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(exit_code_for(&e))
        }
    }
}

fn exit_code_for(err: &anyhow::Error) -> u8 {
    if let Some(pe) = err.downcast_ref::<PulseError>() {
        return match pe {
            PulseError::Database(_) | PulseError::SchemaTooNew { .. } => EXIT_DB,
            PulseError::Io(_) => EXIT_DB,
            _ => EXIT_USER,
        };
    }
    EXIT_USER
}

/// Thin anyhow-like wrapper so we can attach context without a dep.
mod anyhow {
    use std::fmt;

    pub type Result<T> = std::result::Result<T, Error>;

    #[derive(Debug)]
    pub struct Error {
        msg: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    }

    impl Error {
        pub fn msg(m: impl Into<String>) -> Self {
            Self {
                msg: m.into(),
                source: None,
            }
        }

        pub fn downcast_ref<E: std::error::Error + 'static>(&self) -> Option<&E> {
            self.source.as_ref()?.downcast_ref::<E>()
        }
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.msg)?;
            if let Some(s) = &self.source {
                write!(f, ": {s}")?;
            }
            Ok(())
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.source
                .as_ref()
                .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
        }
    }

    impl From<PulseError> for Error {
        fn from(e: PulseError) -> Self {
            Self {
                msg: e.to_string(),
                source: Some(Box::new(e)),
            }
        }
    }

    impl From<std::io::Error> for Error {
        fn from(e: std::io::Error) -> Self {
            Self {
                msg: e.to_string(),
                source: Some(Box::new(e)),
            }
        }
    }

    use pulse_core::PulseError;
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let paths = resolve_paths(cli.data_dir)?;
    paths.ensure_layout()?;

    match cli.command {
        Commands::Version => {
            println!("pulse {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Commands::Config { command } => match command {
            ConfigCmd::Path => {
                println!("{}", paths.config_path().display());
                Ok(())
            }
            ConfigCmd::Show => {
                let cfg = load_config(&paths.config_path())?;
                print!("{}", cfg.to_toml_string()?);
                Ok(())
            }
        },
        Commands::Tasks { command } => {
            let store = open_store(&paths)?;
            match command {
                TasksCmd::List { status, json } => {
                    let filter = status.map(TaskStatus::from);
                    let tasks = store.list_tasks(filter)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&tasks).map_err(|e| {
                            anyhow::Error::msg(format!("json encode: {e}"))
                        })?);
                    } else {
                        print_task_table(&tasks);
                    }
                    Ok(())
                }
                TasksCmd::Show { id } => {
                    let task = resolve_task(&store, &id)?;
                    print_task_detail(&task);
                    let evidence = store.list_evidence(task.id)?;
                    if !evidence.is_empty() {
                        println!();
                        println!("Evidence:");
                        for ev in evidence {
                            println!(
                                "  - [{}] {} {}",
                                ev.kind,
                                ev.source_ref,
                                ev.snippet.as_deref().unwrap_or("")
                            );
                        }
                    }
                    Ok(())
                }
                TasksCmd::Add {
                    title,
                    today,
                    notes,
                } => {
                    let title = title.join(" ");
                    if title.trim().is_empty() {
                        return Err(anyhow::Error::msg("title must not be empty"));
                    }
                    let mut new = NewTask::manual(title);
                    if today {
                        new.status = TaskStatus::Today;
                    }
                    new.notes = notes;
                    let task = store.create_task(new)?;
                    println!("{}  {}", short_id(&task.id), task.title);
                    Ok(())
                }
                TasksCmd::Done { id } => {
                    let task = resolve_task(&store, &id)?;
                    let task = store.mark_done(task.id)?;
                    println!("done  {}  {}", short_id(&task.id), task.title);
                    Ok(())
                }
                TasksCmd::Update {
                    id,
                    title,
                    status,
                    notes,
                } => {
                    if title.is_none() && status.is_none() && notes.is_none() {
                        return Err(anyhow::Error::msg(
                            "provide at least one of --title, --status, --notes",
                        ));
                    }
                    let task = resolve_task(&store, &id)?;
                    let task = store.update_task(
                        task.id,
                        TaskUpdate {
                            title,
                            status: status.map(TaskStatus::from),
                            notes,
                            ..Default::default()
                        },
                    )?;
                    println!(
                        "updated  {}  [{}]  {}",
                        short_id(&task.id),
                        task.status,
                        task.title
                    );
                    Ok(())
                }
                TasksCmd::Move { id, status } => {
                    let task = resolve_task(&store, &id)?;
                    let task = store.set_status(task.id, status.into())?;
                    println!(
                        "moved  {}  -> {}  {}",
                        short_id(&task.id),
                        task.status,
                        task.title
                    );
                    Ok(())
                }
            }
        }
    }
}

fn resolve_paths(data_dir: Option<PathBuf>) -> anyhow::Result<PulsePaths> {
    match data_dir {
        Some(dir) => Ok(PulsePaths::new(dir)),
        None => Ok(PulsePaths::default()?),
    }
}

fn open_store(paths: &PulsePaths) -> anyhow::Result<Store> {
    let conn = open_db(&paths.db_path())?;
    Ok(Store::new(conn))
}

fn resolve_task(store: &Store, id_or_prefix: &str) -> anyhow::Result<Task> {
    let raw = id_or_prefix.trim();
    if raw.is_empty() {
        return Err(anyhow::Error::msg("task id is empty"));
    }

    if let Ok(uuid) = Uuid::parse_str(raw) {
        return store
            .get_task(uuid)?
            .ok_or_else(|| anyhow::Error::msg(format!("task not found: {raw}")));
    }

    let needle = raw.to_ascii_lowercase();
    let all = store.list_tasks(None)?;
    let matches: Vec<_> = all
        .into_iter()
        .filter(|t| {
            t.id.as_hyphenated()
                .to_string()
                .to_ascii_lowercase()
                .starts_with(&needle)
                || t.id.simple().to_string().to_ascii_lowercase().starts_with(&needle)
        })
        .collect();

    match matches.len() {
        0 => Err(anyhow::Error::msg(format!("task not found: {raw}"))),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => Err(anyhow::Error::msg(format!(
            "ambiguous id prefix '{raw}' matches {n} tasks; use a longer prefix"
        ))),
    }
}

fn short_id(id: &Uuid) -> String {
    id.as_hyphenated().to_string()[..8].to_string()
}

fn print_task_table(tasks: &[Task]) {
    if tasks.is_empty() {
        println!("(no tasks)");
        return;
    }
    println!(
        "{:<8}  {:<8}  {:<8}  {}",
        "ID", "STATUS", "SOURCE", "TITLE"
    );
    println!("{}", "-".repeat(60));
    for t in tasks {
        println!(
            "{:<8}  {:<8}  {:<8}  {}",
            short_id(&t.id),
            t.status,
            t.source,
            t.title
        );
    }
}

fn print_task_detail(task: &Task) {
    println!("id:         {}", task.id);
    println!("title:      {}", task.title);
    println!("status:     {}", task.status);
    println!("source:     {}", task.source);
    if let Some(c) = task.confidence {
        println!("confidence: {c:.2}");
    }
    if let Some(p) = &task.project {
        println!("project:    {p}");
    }
    if let Some(n) = &task.notes {
        println!("notes:      {n}");
    }
    if let Some(a) = &task.suggested_next_action {
        println!("next:       {a}");
    }
    println!("created:    {}", task.created_at.to_rfc3339());
    println!("updated:    {}", task.updated_at.to_rfc3339());
    if let Some(c) = task.completed_at {
        println!("completed:  {}", c.to_rfc3339());
    }
}
