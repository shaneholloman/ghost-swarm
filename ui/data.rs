use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};
use swarm::{
    SwarmError,
    forges::github::{self, PullRequestStatus, PullRequestStatusState},
    repos::{Repository, RepositoryStore},
    sessions::{SessionStore, default_session_command},
    workspaces::{Workspace, WorkspaceStore},
};

#[derive(Debug, Clone)]
pub struct WorkspaceGroup {
    pub repo_label: String,
    pub repo_canonical: String,
    pub repo_status: Option<String>,
    pub collapsed: bool,
    pub workspace_count: usize,
    pub workspaces: Vec<WorkspaceEntry>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    pub repo_label: String,
    pub repo_canonical: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub pull_request: Option<WorkspacePullRequest>,
    pub sessions: Vec<SessionEntry>,
}

#[derive(Debug, Clone)]
pub struct WorkspacePullRequest {
    pub state: WorkspacePullRequestState,
    pub summary: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspacePullRequestState {
    Success,
    Pending,
    Failure,
    Merged,
}

#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub id: String,
    pub status: String,
    pub command: String,
    pub log_path: String,
    pub socket_path: String,
}

pub fn load_workspace_groups() -> Result<Vec<WorkspaceGroup>, SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let repo_store = RepositoryStore::open().await?;
        let workspace_store = WorkspaceStore::open().await?;
        let session_store = SessionStore::open().await?;
        let repos = repo_store.list().await?;
        let mut groups = Vec::with_capacity(repos.len());

        for repo in repos {
            let repo_label = repo.alias.clone().unwrap_or_else(|| repo.name.clone());
            let repo_canonical = repo.canonical();
            let repo_status = repo_sync_status(&repo_store, &repo);
            let workspaces = workspace_store.list(&repo_canonical).await?;
            let workspaces = workspaces.into_iter().map(|workspace| async {
                let workspace_ref = format!("{}:{}", workspace.repository, workspace.name);
                let sessions = session_store
                    .list(Some(&workspace_ref))
                    .await?
                    .into_iter()
                    .map(|session| SessionEntry {
                        id: session.id,
                        status: session.status,
                        command: session.command.join(" "),
                        log_path: session.log_path.display().to_string(),
                        socket_path: session.socket_path.display().to_string(),
                    })
                    .collect::<Vec<_>>();

                Ok::<WorkspaceEntry, SwarmError>(map_workspace(
                    &repo_label,
                    &repo_canonical,
                    workspace,
                    sessions,
                ))
            });
            let mut workspace_entries = Vec::new();
            for workspace in workspaces {
                workspace_entries.push(workspace.await?);
            }
            let workspace_count = workspace_entries.len();

            groups.push(WorkspaceGroup {
                repo_label,
                repo_canonical,
                repo_status,
                collapsed: repo.collapsed,
                workspace_count,
                workspaces: workspace_entries,
            });
        }

        Ok(groups)
    })
}

pub fn create_workspace(
    repository: &str,
    name: Option<&str>,
) -> Result<WorkspaceEntry, SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let repo_store = RepositoryStore::open().await?;
        let workspace_store = WorkspaceStore::open().await?;
        let session_store = SessionStore::open().await?;

        let repo = repo_store
            .resolve_repository(repository)
            .await?
            .ok_or_else(|| SwarmError::RepositoryNotFound(repository.to_string()))?;
        let workspace = workspace_store.create(repository, name).await?;
        let workspace_ref = format!("{}:{}", workspace.repository, workspace.name);
        if let Err(err) = session_store
            .create(&workspace_ref, &default_session_command())
            .await
        {
            eprintln!("failed to create default session for {workspace_ref}: {err}");
        }
        let sessions = session_store
            .list(Some(&workspace_ref))
            .await?
            .into_iter()
            .map(|session| SessionEntry {
                id: session.id,
                status: session.status,
                command: session.command.join(" "),
                log_path: session.log_path.display().to_string(),
                socket_path: session.socket_path.display().to_string(),
            })
            .collect::<Vec<_>>();

        Ok(map_workspace(
            repo.alias.as_deref().unwrap_or(&repo.name),
            &repo.canonical(),
            workspace,
            sessions,
        ))
    })
}

pub fn add_repository(repository: &str, alias: Option<&str>) -> Result<Repository, SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let repo_store = RepositoryStore::open().await?;
        repo_store.add(repository, alias).await
    })
}

pub fn sync_repository(repository: &str) -> Result<(), SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let repo_store = RepositoryStore::open().await?;
        repo_store.sync(repository).await?;
        Ok(())
    })
}

pub fn collapse_repository(repository: &str) -> Result<(), SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let repo_store = RepositoryStore::open().await?;
        repo_store.collapse(repository).await?;
        Ok(())
    })
}

pub fn expand_repository(repository: &str) -> Result<(), SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let repo_store = RepositoryStore::open().await?;
        repo_store.expand(repository).await?;
        Ok(())
    })
}

pub fn rename_workspace(workspace_ref: &str, name: &str) -> Result<WorkspaceEntry, SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let repo_store = RepositoryStore::open().await?;
        let workspace_store = WorkspaceStore::open().await?;
        let session_store = SessionStore::open().await?;
        let workspace = workspace_store.rename(workspace_ref, name).await?;
        let repo = repo_store
            .resolve_repository(&workspace.repository)
            .await?
            .ok_or_else(|| SwarmError::RepositoryNotFound(workspace.repository.clone()))?;
        let workspace_ref = format!("{}:{}", workspace.repository, workspace.name);
        let sessions = session_store
            .list(Some(&workspace_ref))
            .await?
            .into_iter()
            .map(|session| SessionEntry {
                id: session.id,
                status: session.status,
                command: session.command.join(" "),
                log_path: session.log_path.display().to_string(),
                socket_path: session.socket_path.display().to_string(),
            })
            .collect::<Vec<_>>();

        Ok(map_workspace(
            repo.alias.as_deref().unwrap_or(&repo.name),
            &repo.canonical(),
            workspace,
            sessions,
        ))
    })
}

pub fn clone_workspace(workspace_ref: &str, name: &str) -> Result<WorkspaceEntry, SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let repo_store = RepositoryStore::open().await?;
        let workspace_store = WorkspaceStore::open().await?;
        let session_store = SessionStore::open().await?;
        let workspace = workspace_store.clone(workspace_ref, name).await?;
        let repo = repo_store
            .resolve_repository(&workspace.repository)
            .await?
            .ok_or_else(|| SwarmError::RepositoryNotFound(workspace.repository.clone()))?;
        let workspace_ref = format!("{}:{}", workspace.repository, workspace.name);
        if let Err(err) = session_store
            .create(&workspace_ref, &default_session_command())
            .await
        {
            eprintln!("failed to create default session for {workspace_ref}: {err}");
        }
        let sessions = session_store
            .list(Some(&workspace_ref))
            .await?
            .into_iter()
            .map(|session| SessionEntry {
                id: session.id,
                status: session.status,
                command: session.command.join(" "),
                log_path: session.log_path.display().to_string(),
                socket_path: session.socket_path.display().to_string(),
            })
            .collect::<Vec<_>>();

        Ok(map_workspace(
            repo.alias.as_deref().unwrap_or(&repo.name),
            &repo.canonical(),
            workspace,
            sessions,
        ))
    })
}

pub fn remove_workspace(workspace_ref: &str) -> Result<WorkspaceEntry, SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let repo_store = RepositoryStore::open().await?;
        let session_store = SessionStore::open().await?;
        let workspace_store = WorkspaceStore::open().await?;
        let sessions = session_store.list(Some(workspace_ref)).await?;

        for session in sessions {
            session_store.stop(&session.id).await?;
            session_store.remove(&session.id).await?;
        }

        let workspace = workspace_store.remove(workspace_ref).await?;
        let repo = repo_store
            .resolve_repository(&workspace.repository)
            .await?
            .ok_or_else(|| SwarmError::RepositoryNotFound(workspace.repository.clone()))?;

        Ok(map_workspace(
            repo.alias.as_deref().unwrap_or(&repo.name),
            &repo.canonical(),
            workspace,
            Vec::new(),
        ))
    })
}

pub fn create_session(workspace_ref: &str) -> Result<SessionEntry, SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let session_store = SessionStore::open().await?;
        let session = session_store
            .create(workspace_ref, &default_session_command())
            .await?;

        Ok(SessionEntry {
            id: session.id,
            status: session.status,
            command: session.command.join(" "),
            log_path: session.log_path.display().to_string(),
            socket_path: session.socket_path.display().to_string(),
        })
    })
}

pub fn close_session(session_id: &str) -> Result<SessionEntry, SwarmError> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let session_store = SessionStore::open().await?;
        session_store.stop(session_id).await?;
        let session = session_store.remove(session_id).await?;

        Ok(SessionEntry {
            id: session.id,
            status: session.status,
            command: session.command.join(" "),
            log_path: session.log_path.display().to_string(),
            socket_path: session.socket_path.display().to_string(),
        })
    })
}

pub fn current_workspace_branch(path: &str) -> Result<String, SwarmError> {
    let path = Path::new(path);
    let branch = run_git(path, ["branch", "--show-current"])?;
    if !branch.is_empty() {
        return Ok(branch);
    }

    run_git(path, ["rev-parse", "--short", "HEAD"])
}

pub fn workspace_head_path(path: &str) -> Result<PathBuf, SwarmError> {
    let workspace_path = Path::new(path);
    let git_path = workspace_path.join(".git");
    let git_dir = if git_path.is_dir() {
        git_path
    } else {
        resolve_gitdir_file(&git_path, workspace_path)?
    };

    Ok(git_dir.join("HEAD"))
}

fn map_workspace(
    repo_label: &str,
    repo_canonical: &str,
    workspace: Workspace,
    sessions: Vec<SessionEntry>,
) -> WorkspaceEntry {
    let pull_request = github::workspace_pull_request_status(&workspace.path).map(map_pull_request);

    WorkspaceEntry {
        repo_label: repo_label.to_string(),
        repo_canonical: repo_canonical.to_string(),
        name: workspace.name,
        branch: workspace.branch,
        path: workspace.path.display().to_string(),
        pull_request,
        sessions,
    }
}

fn map_pull_request(status: PullRequestStatus) -> WorkspacePullRequest {
    WorkspacePullRequest {
        state: match status.state {
            PullRequestStatusState::Success => WorkspacePullRequestState::Success,
            PullRequestStatusState::Pending => WorkspacePullRequestState::Pending,
            PullRequestStatusState::Failure => WorkspacePullRequestState::Failure,
            PullRequestStatusState::Merged => WorkspacePullRequestState::Merged,
        },
        summary: status.summary,
        url: status.url,
    }
}

fn repo_sync_status(repo_store: &RepositoryStore, repo: &Repository) -> Option<String> {
    let bare_repo_path = repo_store.bare_repo_path(repo);
    let timestamp = [
        bare_repo_path.join("FETCH_HEAD"),
        bare_repo_path.join("HEAD"),
        bare_repo_path
            .join("refs")
            .join("remotes")
            .join("origin")
            .join("HEAD"),
    ]
    .into_iter()
    .find_map(file_modified_time)?;

    Some(format_relative_sync_time(timestamp))
}

fn file_modified_time(path: PathBuf) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn format_relative_sync_time(timestamp: SystemTime) -> String {
    let Ok(elapsed) = SystemTime::now().duration_since(timestamp) else {
        return "just now".to_string();
    };

    format_elapsed(elapsed)
}

fn format_elapsed(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        return "just now".to_string();
    }

    let minutes = seconds / 60;
    if minutes < 60 {
        return pluralize(minutes, "minute");
    }

    let hours = minutes / 60;
    if hours < 24 {
        return pluralize(hours, "hour");
    }

    let days = hours / 24;
    if days < 7 {
        return pluralize(days, "day");
    }

    let weeks = days / 7;
    if weeks < 5 {
        return pluralize(weeks, "week");
    }

    let months = days / 30;
    if months < 12 {
        return pluralize(months.max(1), "month");
    }

    pluralize((days / 365).max(1), "year")
}

fn pluralize(value: u64, unit: &str) -> String {
    if value == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{value} {unit}s ago")
    }
}

fn resolve_gitdir_file(git_path: &Path, workspace_path: &Path) -> Result<PathBuf, SwarmError> {
    let contents = fs::read_to_string(git_path)?;
    let git_dir = contents
        .strip_prefix("gitdir: ")
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| SwarmError::Git(format!("invalid gitdir file at {}", git_path.display())))?;

    let git_dir = Path::new(git_dir);
    if git_dir.is_absolute() {
        return Ok(git_dir.to_path_buf());
    }

    Ok(workspace_path.join(git_dir))
}

fn run_git<I, S>(cwd: &Path, args: I) -> Result<String, SwarmError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd);

    for arg in args {
        cmd.arg(arg.as_ref());
    }

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(SwarmError::Git(render_git_failure(output)));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn render_git_failure(output: std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if !stderr.is_empty() {
        return stderr;
    }

    if !stdout.is_empty() {
        return stdout;
    }

    format!("exit status {}", output.status)
}
