use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use swarm::{
    SwarmError,
    repos::RepositoryStore,
    sessions::{SessionStore, default_session_command},
    workspaces::{Workspace, WorkspaceStore},
};

#[derive(Debug, Clone)]
pub struct WorkspaceGroup {
    pub repo_label: String,
    pub repo_canonical: String,
    pub workspaces: Vec<WorkspaceEntry>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    pub repo_label: String,
    pub repo_canonical: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub sessions: Vec<SessionEntry>,
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
            let workspaces = workspace_store
                .list(&repo_canonical)
                .await?
                .into_iter()
                .map(|workspace| async {
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

            groups.push(WorkspaceGroup {
                repo_label,
                repo_canonical,
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
    WorkspaceEntry {
        repo_label: repo_label.to_string(),
        repo_canonical: repo_canonical.to_string(),
        name: workspace.name,
        branch: workspace.branch,
        path: workspace.path.display().to_string(),
        sessions,
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
