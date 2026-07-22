//! Pulse CLI — prefers IPC when the service is up; otherwise direct SQLite.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use pulse_core::ipc::pid::{live_service_pid, process_is_live, read_pid_file};
use pulse_core::{
    load_config, open_db, try_connect, write_config, IpcClient, NewTask, PulseError, PulsePaths,
    Store, Task, TaskStatus, TaskUpdate,
};

/// Exit codes: 0 ok, 1 user/logic, 2 service unreachable, 3 DB.
const EXIT_OK: u8 = 0;
const EXIT_USER: u8 = 1;
const EXIT_SERVICE: u8 = 2;
const EXIT_DB: u8 = 3;

#[derive(Parser, Debug)]
#[command(
    name = "pulse",
    version,
    about = "Pulse — local-first todo that stays current"
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
    Tasks {
        #[command(subcommand)]
        command: TasksCmd,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCmd,
    },
    Service {
        #[command(subcommand)]
        command: ServiceCmd,
    },
    /// Enable/disable work-signal sources
    Sources {
        #[command(subcommand)]
        command: SourcesCmd,
    },
    Version,
}

#[derive(Subcommand, Debug)]
enum TasksCmd {
    List {
        #[arg(long, value_enum)]
        status: Option<StatusArg>,
        #[arg(long)]
        json: bool,
    },
    Show {
        id: String,
    },
    Add {
        title: Vec<String>,
        #[arg(long)]
        today: bool,
        #[arg(long)]
        notes: Option<String>,
    },
    Done {
        id: String,
    },
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, value_enum)]
        status: Option<StatusArg>,
        #[arg(long)]
        notes: Option<String>,
    },
    Move {
        id: String,
        #[arg(value_enum)]
        status: StatusArg,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    Show,
    Path,
    /// Reload config in a running service (no-op offline)
    Reload,
}

#[derive(Subcommand, Debug)]
enum ServiceCmd {
    /// Run service in foreground (delegates to pulse-service)
    Run {
        #[arg(long)]
        quiet: bool,
    },
    /// Start service in background and wait until ping succeeds
    Start,
    /// Stop running service
    Stop {
        #[arg(long)]
        force: bool,
    },
    /// Show service status
    Status,
}

#[derive(Subcommand, Debug)]
enum SourcesCmd {
    /// List sources and enabled flags
    List,
    /// Enable a source (claude|codex)
    Enable {
        id: String,
    },
    /// Disable a source
    Disable {
        id: String,
    },
    /// Trigger one inference scan now (service must be running)
    Scan,
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

enum Backend {
    Ipc(IpcClient),
    Direct(Store),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::from(EXIT_OK),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

struct CliError {
    msg: String,
    code: u8,
}

impl CliError {
    fn user(m: impl Into<String>) -> Self {
        Self {
            msg: m.into(),
            code: EXIT_USER,
        }
    }
    fn service(m: impl Into<String>) -> Self {
        Self {
            msg: m.into(),
            code: EXIT_SERVICE,
        }
    }
    fn exit_code(&self) -> u8 {
        self.code
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl From<PulseError> for CliError {
    fn from(e: PulseError) -> Self {
        let code = match &e {
            PulseError::Database(_) | PulseError::SchemaTooNew { .. } | PulseError::Io(_) => {
                EXIT_DB
            }
            PulseError::ServiceUnreachable => EXIT_SERVICE,
            PulseError::Ipc(_) => EXIT_SERVICE,
            _ => EXIT_USER,
        };
        Self {
            msg: e.to_string(),
            code,
        }
    }
}

fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    let paths = resolve_paths(cli.data_dir)?;
    paths.ensure_layout().map_err(PulseError::from)?;

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
            ConfigCmd::Reload => {
                let mut be = open_backend(&paths, true)?;
                if let Backend::Ipc(c) = &mut be {
                    c.config_reload()?;
                    println!("config reloaded");
                } else {
                    println!("service not running; config will load on next start");
                }
                Ok(())
            }
        },
        Commands::Service { command } => match command {
            ServiceCmd::Run { quiet } => service_run(&paths, quiet),
            ServiceCmd::Start => service_start(&paths),
            ServiceCmd::Stop { force } => service_stop(&paths, force),
            ServiceCmd::Status => service_status(&paths),
        },
        Commands::Sources { command } => match command {
            SourcesCmd::List => sources_list(&paths),
            SourcesCmd::Enable { id } => sources_set(&paths, &id, true),
            SourcesCmd::Disable { id } => sources_set(&paths, &id, false),
            SourcesCmd::Scan => sources_scan(&paths),
        },
        Commands::Tasks { command } => {
            let mut backend = open_backend(&paths, true)?;
            match command {
                TasksCmd::List { status, json } => {
                    let filter = status.map(TaskStatus::from);
                    let tasks = tasks_list(&mut backend, filter)?;
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&tasks).map_err(|e| {
                                CliError::user(format!("json encode: {e}"))
                            })?
                        );
                    } else {
                        print_task_table(&tasks);
                    }
                    Ok(())
                }
                TasksCmd::Show { id } => {
                    let (task, evidence) = tasks_get(&mut backend, &id)?;
                    print_task_detail(&task);
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
                        return Err(CliError::user("title must not be empty"));
                    }
                    let status = if today {
                        Some(TaskStatus::Today)
                    } else {
                        None
                    };
                    let task = tasks_create(&mut backend, &title, status, notes)?;
                    println!("{}  {}", short_id(&task.id), task.title);
                    Ok(())
                }
                TasksCmd::Done { id } => {
                    let task = tasks_done(&mut backend, &id)?;
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
                        return Err(CliError::user(
                            "provide at least one of --title, --status, --notes",
                        ));
                    }
                    let task = tasks_update(
                        &mut backend,
                        &id,
                        title,
                        status.map(TaskStatus::from),
                        notes,
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
                    let task =
                        tasks_update(&mut backend, &id, None, Some(status.into()), None)?;
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

fn resolve_paths(data_dir: Option<PathBuf>) -> Result<PulsePaths, CliError> {
    match data_dir {
        Some(dir) => Ok(PulsePaths::new(dir)),
        None => Ok(PulsePaths::default()?),
    }
}

/// Offline write policy from design.
fn open_backend(paths: &PulsePaths, for_write: bool) -> Result<Backend, CliError> {
    let cfg = load_config(&paths.config_path())?;
    let pipe = &cfg.service.pipe_name;
    let live = live_service_pid(&paths.service_pid_path())?;

    match try_connect(pipe) {
        Ok(client) => Ok(Backend::Ipc(client)),
        Err(_) => {
            if live.is_some() {
                if for_write {
                    return Err(CliError::service(
                        "service PID is live but IPC is unreachable; try `pulse service stop` or check logs",
                    ));
                }
                // read-only direct
                eprintln!("warning: service unreachable; opening DB read-only path via direct store");
            } else {
                eprintln!("warning: service not running; using direct database access");
            }
            let conn = open_db(&paths.db_path())?;
            Ok(Backend::Direct(Store::new(conn)))
        }
    }
}

fn tasks_list(be: &mut Backend, status: Option<TaskStatus>) -> Result<Vec<Task>, CliError> {
    match be {
        Backend::Ipc(c) => Ok(c.tasks_list(status)?),
        Backend::Direct(s) => Ok(s.list_tasks(status)?),
    }
}

fn tasks_get(
    be: &mut Backend,
    id: &str,
) -> Result<(Task, Vec<pulse_core::Evidence>), CliError> {
    match be {
        Backend::Ipc(c) => Ok(c.tasks_get(id)?),
        Backend::Direct(s) => {
            let task = s.resolve_task(id)?;
            let evidence = s.list_evidence(task.id)?;
            Ok((task, evidence))
        }
    }
}

fn tasks_create(
    be: &mut Backend,
    title: &str,
    status: Option<TaskStatus>,
    notes: Option<String>,
) -> Result<Task, CliError> {
    match be {
        Backend::Ipc(c) => Ok(c.tasks_create(title, status, notes)?),
        Backend::Direct(s) => {
            let mut new = NewTask::manual(title);
            if let Some(st) = status {
                new.status = st;
            }
            new.notes = notes;
            Ok(s.create_task(new)?)
        }
    }
}

fn tasks_done(be: &mut Backend, id: &str) -> Result<Task, CliError> {
    match be {
        Backend::Ipc(c) => Ok(c.tasks_done(id)?),
        Backend::Direct(s) => {
            let task = s.resolve_task(id)?;
            Ok(s.mark_done(task.id)?)
        }
    }
}

fn tasks_update(
    be: &mut Backend,
    id: &str,
    title: Option<String>,
    status: Option<TaskStatus>,
    notes: Option<String>,
) -> Result<Task, CliError> {
    match be {
        Backend::Ipc(c) => Ok(c.tasks_update(id, title, status, notes)?),
        Backend::Direct(s) => {
            let task = s.resolve_task(id)?;
            Ok(s.update_task(
                task.id,
                TaskUpdate {
                    title,
                    status,
                    notes,
                    ..Default::default()
                },
            )?)
        }
    }
}

fn service_run(paths: &PulsePaths, quiet: bool) -> Result<(), CliError> {
    let mut cmd = service_command();
    cmd.arg("run");
    if quiet {
        cmd.arg("--quiet");
    }
    cmd.arg("--data-dir").arg(&paths.root);
    let status = cmd
        .status()
        .map_err(|e| CliError::user(format!("failed to launch pulse-service: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::user(format!(
            "pulse-service exited with {status}"
        )))
    }
}

fn service_start(paths: &PulsePaths) -> Result<(), CliError> {
    let cfg = load_config(&paths.config_path())?;
    if let Some(info) = live_service_pid(&paths.service_pid_path())? {
        if try_connect(&cfg.service.pipe_name).is_ok() {
            return Err(CliError::user(format!(
                "service already running (pid {})",
                info.pid
            )));
        }
        return Err(CliError::service(format!(
            "pid {} is live but pipe is dead; try `pulse service stop --force`",
            info.pid
        )));
    }

    let mut cmd = service_command();
    cmd.arg("run")
        .arg("--quiet")
        .arg("--data-dir")
        .arg(&paths.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    cmd.spawn()
        .map_err(|e| CliError::user(format!("failed to start pulse-service: {e}")))?;

    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
            if c.ping().is_ok() {
                println!("service started");
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(CliError::service(
        "service did not become ready within 15s (ping failed)",
    ))
}

fn service_stop(paths: &PulsePaths, force: bool) -> Result<(), CliError> {
    let cfg = load_config(&paths.config_path())?;
    if let Ok(mut c) = try_connect(&cfg.service.pipe_name) {
        let _ = c.service_shutdown();
        // wait for exit
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if live_service_pid(&paths.service_pid_path())?.is_none() {
                println!("service stopped");
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    if let Some(info) = read_pid_file(&paths.service_pid_path()).map_err(PulseError::from)? {
        if process_is_live(info.pid) {
            if force {
                force_kill(info.pid)?;
                let _ = std::fs::remove_file(paths.service_pid_path());
                println!("service force-stopped (pid {})", info.pid);
                return Ok(());
            }
            return Err(CliError::service(format!(
                "service pid {} still live; re-run with --force",
                info.pid
            )));
        }
        let _ = std::fs::remove_file(paths.service_pid_path());
    }
    println!("service not running");
    Ok(())
}

fn service_status(paths: &PulsePaths) -> Result<(), CliError> {
    let cfg = load_config(&paths.config_path())?;
    match try_connect(&cfg.service.pipe_name) {
        Ok(mut c) => {
            let status = c.service_status()?;
            println!("{}", serde_json::to_string_pretty(&status).unwrap());
            Ok(())
        }
        Err(_) => {
            if let Some(info) = live_service_pid(&paths.service_pid_path())? {
                println!(
                    "pid {} live but IPC unreachable (pipe {})",
                    info.pid, info.pipe_name
                );
                return Err(CliError::service("service unhealthy"));
            }
            println!("service not running");
            Ok(())
        }
    }
}

fn sources_list(paths: &PulsePaths) -> Result<(), CliError> {
    if let Ok(mut c) = try_connect_from_paths(paths) {
        let v = c.sources_list()?;
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return Ok(());
    }
    let cfg = load_config(&paths.config_path())?;
    println!(
        "claude: {}\ncodex:  {}",
        if cfg.sources.claude.enabled {
            "enabled"
        } else {
            "disabled"
        },
        if cfg.sources.codex.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    Ok(())
}

fn sources_set(paths: &PulsePaths, id: &str, enabled: bool) -> Result<(), CliError> {
    let id = id.to_ascii_lowercase();
    if id != "claude" && id != "codex" {
        return Err(CliError::user("source id must be 'claude' or 'codex'"));
    }
    // Prefer IPC so running service reloads atomically via sources.set_enabled
    if let Ok(mut c) = try_connect_from_paths(paths) {
        c.sources_set_enabled(&id, enabled)?;
        println!(
            "source {id} {}",
            if enabled { "enabled" } else { "disabled" }
        );
        return Ok(());
    }
    let mut cfg = load_config(&paths.config_path())?;
    match id.as_str() {
        "claude" => cfg.sources.claude.enabled = enabled,
        "codex" => cfg.sources.codex.enabled = enabled,
        _ => unreachable!(),
    }
    write_config(&paths.config_path(), &cfg)?;
    println!(
        "source {id} {} (service not running; will apply on next start)",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

fn sources_scan(paths: &PulsePaths) -> Result<(), CliError> {
    let mut c = try_connect_from_paths(paths).map_err(|_| {
        CliError::service("service must be running for `sources scan` (try `pulse service start`)")
    })?;
    let v = c.inference_run_once()?;
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
    Ok(())
}

fn try_connect_from_paths(paths: &PulsePaths) -> Result<IpcClient, PulseError> {
    let cfg = load_config(&paths.config_path())?;
    try_connect(&cfg.service.pipe_name)
}

fn service_command() -> Command {
    // Prefer pulse-service next to this binary, else PATH.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(if cfg!(windows) {
                "pulse-service.exe"
            } else {
                "pulse-service"
            });
            if candidate.exists() {
                return Command::new(candidate);
            }
        }
    }
    Command::new("pulse-service")
}

#[cfg(windows)]
fn force_kill(pid: u32) -> Result<(), CliError> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status()
        .map_err(|e| CliError::user(format!("taskkill failed: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::user(format!("taskkill exited {status}")))
    }
}

#[cfg(not(windows))]
fn force_kill(pid: u32) -> Result<(), CliError> {
    let status = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .map_err(|e| CliError::user(format!("kill failed: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::user(format!("kill exited {status}")))
    }
}

fn short_id(id: &uuid::Uuid) -> String {
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

// silence unused import on some cfgs
#[allow(dead_code)]
fn _path_ty(_: &Path) {}
