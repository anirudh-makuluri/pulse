//! Pulse CLI — prefers IPC when the service is up; otherwise direct SQLite.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use pulse_core::ipc::pid::{live_service_pid, process_is_live, read_pid_file};
use pulse_core::{
    apply_checkin_answer, load_config, open_db, parse_answer_input, try_connect, write_config,
    IpcClient, NewTask, PulseError, PulsePaths, Store, Task, TaskStatus, TaskUpdate,
};
use pulse_llm::{llm_status, probe_preference};

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
    /// Privacy settings (remote LLM ack)
    Privacy {
        #[command(subcommand)]
        command: PrivacyCmd,
    },
    /// LLM backend status
    Llm {
        #[command(subcommand)]
        command: LlmCmd,
    },
    /// Daily summaries
    Summary {
        #[command(subcommand)]
        command: SummaryCmd,
    },
    /// Check-ins
    Checkin {
        #[command(subcommand)]
        command: CheckinCmd,
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

#[derive(Subcommand, Debug)]
enum PrivacyCmd {
    /// Acknowledge residual risk of sending redacted excerpts via agent CLIs
    Acknowledge,
}

#[derive(Subcommand, Debug)]
enum LlmCmd {
    /// Show resolved backend (heuristic vs grok/claude/codex)
    Status,
}

#[derive(Subcommand, Debug)]
enum SummaryCmd {
    /// Generate (or refresh) a daily summary
    Generate {
        #[arg(long)]
        date: Option<String>,
    },
    /// Show stored summary for a day
    Show {
        #[arg(long)]
        date: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum CheckinCmd {
    /// List open check-ins
    List {
        #[arg(long)]
        all: bool,
    },
    /// Answer a check-in: yes/no or JSON
    Answer {
        id: String,
        /// e.g. yes | no | {"done":true} | {"next_action":"..."}
        response: Vec<String>,
    },
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
        Commands::Privacy { command } => match command {
            PrivacyCmd::Acknowledge => privacy_ack(&paths),
        },
        Commands::Llm { command } => match command {
            LlmCmd::Status => llm_status_cmd(&paths),
        },
        Commands::Summary { command } => match command {
            SummaryCmd::Generate { date } => summary_generate(&paths, date),
            SummaryCmd::Show { date } => summary_show(&paths, date),
        },
        Commands::Checkin { command } => match command {
            CheckinCmd::List { all } => checkin_list(&paths, !all),
            CheckinCmd::Answer { id, response } => {
                checkin_answer(&paths, &id, &response.join(" "))
            }
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
    if enabled {
        let cfg = load_config(&paths.config_path())?;
        if !cfg.privacy.acknowledge_remote_llm {
            eprintln!(
                "Note: remote LLM is not acknowledged — inference stays heuristic until `pulse privacy acknowledge`."
            );
            eprintln!(
                "Agent CLIs may send redacted session excerpts to their providers once acknowledged."
            );
        }
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

fn privacy_ack(paths: &PulsePaths) -> Result<(), CliError> {
    eprintln!(
        "Pulse may call your installed agent CLI with redacted session excerpts (data can leave this machine via that CLI's provider)."
    );
    if let Ok(mut c) = try_connect_from_paths(paths) {
        c.call_raw("privacy.acknowledge", serde_json::json!({}))?;
        println!("acknowledged (service reloaded config)");
        return Ok(());
    }
    let mut cfg = load_config(&paths.config_path())?;
    cfg.privacy.acknowledge_remote_llm = true;
    write_config(&paths.config_path(), &cfg)?;
    println!("acknowledged (written to config.toml)");
    Ok(())
}

fn llm_status_cmd(paths: &PulsePaths) -> Result<(), CliError> {
    if let Ok(mut c) = try_connect_from_paths(paths) {
        let v = c.call_raw("llm.status", serde_json::json!({}))?;
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return Ok(());
    }
    let cfg = load_config(&paths.config_path())?;
    let st = llm_status(&cfg.llm, &cfg.privacy);
    let pref = probe_preference(&cfg.llm);
    println!("backend:   {}", st.backend_id);
    println!("path:      {}", st.path.as_deref().unwrap_or("(none)"));
    println!("privacy:   ack={}", st.privacy_ack);
    println!("provider:  {}", st.provider_setting);
    println!("reason:    {}", st.reason);
    println!("preference probes:");
    for (name, path) in pref {
        println!("  {name}: {}", path.unwrap_or_else(|| "(not found)".into()));
    }
    Ok(())
}

fn summary_generate(paths: &PulsePaths, date: Option<String>) -> Result<(), CliError> {
    let day = date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    if let Ok(mut c) = try_connect_from_paths(paths) {
        let v = c.call_raw(
            "summary.generate",
            serde_json::json!({ "date": day }),
        )?;
        if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
            println!("{t}");
        } else {
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        return Ok(());
    }
    // Offline: open store + generate via pipeline helper needs service crate — do heuristic in-process
    use pulse_llm::{resolve_llm_client, SummaryRequest};
    let cfg = load_config(&paths.config_path())?;
    let conn = open_db(&paths.db_path())?;
    let store = Store::new(conn);
    let tasks = store.list_tasks(None)?;
    let lines: Vec<String> = tasks
        .iter()
        .map(|t| format!("[{}] {}", t.status, t.title))
        .collect();
    let client = resolve_llm_client(&cfg.llm, &cfg.privacy);
    let out = client
        .summarize_day(&SummaryRequest {
            day: day.clone(),
            task_lines: lines,
            activity_notes: None,
        })
        .map_err(|e| CliError::user(e.to_string()))?;
    let offset = chrono::Local::now().offset().local_minus_utc() / 60;
    let highlights = serde_json::to_string(&out.highlights).unwrap_or_else(|_| "[]".into());
    store.upsert_summary(&day, offset, &out.text, &highlights, "[]")?;
    println!("{}", out.text);
    Ok(())
}

fn summary_show(paths: &PulsePaths, date: Option<String>) -> Result<(), CliError> {
    let day = date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    if let Ok(mut c) = try_connect_from_paths(paths) {
        let v = c.call_raw("summary.get", serde_json::json!({ "date": day }))?;
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return Ok(());
    }
    let conn = open_db(&paths.db_path())?;
    let store = Store::new(conn);
    match store.get_summary(&day)? {
        Some(s) => {
            println!("day: {}", s.day);
            println!("{}", s.text);
        }
        None => println!("(no summary for {day})"),
    }
    Ok(())
}

fn checkin_list(paths: &PulsePaths, open_only: bool) -> Result<(), CliError> {
    if let Ok(mut c) = try_connect_from_paths(paths) {
        let v = c.call_raw(
            "checkin.list",
            serde_json::json!({ "open_only": open_only }),
        )?;
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return Ok(());
    }
    let conn = open_db(&paths.db_path())?;
    let store = Store::new(conn);
    let items = store.list_checkins(open_only)?;
    if items.is_empty() {
        println!("(no check-ins)");
        return Ok(());
    }
    for c in items {
        println!(
            "{}  [{}]  {}  task={}",
            &c.id.to_string()[..8],
            c.kind.as_str(),
            c.question,
            c.task_id
                .map(|t| t.to_string()[..8].to_string())
                .unwrap_or_else(|| "-".into())
        );
    }
    Ok(())
}

fn checkin_answer(paths: &PulsePaths, id: &str, response: &str) -> Result<(), CliError> {
    let answer = parse_answer_input(response)?;
    if let Ok(mut c) = try_connect_from_paths(paths) {
        let v = c.call_raw(
            "checkin.answer",
            serde_json::json!({ "id": id, "answer": answer }),
        )?;
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return Ok(());
    }
    let conn = open_db(&paths.db_path())?;
    let store = Store::new(conn);
    let checkin = store.resolve_checkin(id)?;
    let patch = apply_checkin_answer(checkin.kind, &answer)?;
    if let Some(tid) = checkin.task_id {
        let t = store.update_task(tid, patch)?;
        println!("task {} -> {}", &t.id.to_string()[..8], t.status);
    }
    store.answer_checkin(checkin.id, &answer.to_string())?;
    println!("check-in answered");
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
