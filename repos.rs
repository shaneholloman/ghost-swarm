use directories::ProjectDirs;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use turso::{Builder, Connection};

use crate::{SwarmError, database_error};

#[derive(Debug, Clone, Serialize)]
pub struct Repository {
    pub host: String,
    pub owner: String,
    pub name: String,
    pub alias: Option<String>,
}

impl Repository {
    pub fn parse(input: &str, alias: Option<&str>) -> Result<Self, SwarmError> {
        let mut parts = input.split('/');
        let host = parts.next().unwrap_or_default().trim();
        let owner = parts.next().unwrap_or_default().trim();
        let name = parts.next().unwrap_or_default().trim();

        if host.is_empty()
            || owner.is_empty()
            || name.is_empty()
            || parts.next().is_some()
            || [host, owner, name]
                .iter()
                .any(|part| part.contains(char::is_whitespace))
        {
            return Err(SwarmError::InvalidRepository(input.to_string()));
        }

        let default_alias = alias
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| name.to_string());

        Ok(Self {
            host: host.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
            alias: Some(default_alias),
        })
    }

    pub fn canonical(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.name)
    }

    pub fn remote_url(&self) -> String {
        format!(
            "https://{}/{}/{}.git",
            resolve_remote_host(&self.host),
            self.owner,
            self.name
        )
    }
}

#[derive(Debug, Clone)]
struct SwarmPaths {
    data_dir: PathBuf,
    repos_dir: PathBuf,
    index_db_path: PathBuf,
}

impl SwarmPaths {
    fn resolve() -> Result<Self, SwarmError> {
        let dirs =
            ProjectDirs::from("com", "penberg", "swarm").ok_or(SwarmError::PathResolution)?;
        let data_dir = dirs.data_dir().to_path_buf();
        let repos_dir = data_dir.join("repos");
        let index_db_path = data_dir.join("index.db");

        Ok(Self {
            data_dir,
            repos_dir,
            index_db_path,
        })
    }

    fn repo_dir(&self, repo: &Repository) -> PathBuf {
        self.repos_dir
            .join(&repo.host)
            .join(&repo.owner)
            .join(&repo.name)
    }

    fn repo_db_path(&self, repo: &Repository) -> PathBuf {
        self.repo_dir(repo).join("repo.db")
    }

    fn repo_meta_path(&self, repo: &Repository) -> PathBuf {
        self.repo_dir(repo).join("meta.toml")
    }
}

pub struct RepositoryStore {
    paths: SwarmPaths,
    conn: Connection,
}

impl RepositoryStore {
    pub async fn open() -> Result<Self, SwarmError> {
        let paths = SwarmPaths::resolve()?;
        fs::create_dir_all(&paths.data_dir)?;
        fs::create_dir_all(&paths.repos_dir)?;

        let index_db_path = paths.index_db_path.clone();
        let db = Builder::new_local(path_to_string(&paths.index_db_path)?)
            .build()
            .await
            .map_err(|err| database_error(&index_db_path, "open", err))?;
        let conn = db
            .connect()
            .map_err(|err| database_error(&index_db_path, "connect", err))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS repos (
                host TEXT NOT NULL,
                owner TEXT NOT NULL,
                name TEXT NOT NULL,
                alias TEXT,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (host, owner, name)
            );

            CREATE UNIQUE INDEX IF NOT EXISTS repos_alias_idx
            ON repos(alias)
            WHERE alias IS NOT NULL;
            ",
        )
        .await
        .map_err(|err| database_error(&index_db_path, "initialize schema", err))?;

        Ok(Self { paths, conn })
    }

    pub async fn add(
        &self,
        repository: &str,
        alias: Option<&str>,
    ) -> Result<Repository, SwarmError> {
        let repo = Repository::parse(repository, alias)?;

        if self.find_repository(&repo).await?.is_some() {
            return Err(SwarmError::DuplicateRepository(repo.canonical()));
        }

        if let Some(alias) = &repo.alias {
            if self.find_alias(alias).await? {
                return Err(SwarmError::DuplicateAlias(alias.clone()));
            }
        }

        let repo_dir = self.paths.repo_dir(&repo);
        fs::create_dir_all(&repo_dir)?;
        fs::write(self.paths.repo_meta_path(&repo), render_meta_toml(&repo))?;

        let repo_db_path = self.paths.repo_db_path(&repo);
        let repo_db = Builder::new_local(path_to_string(&repo_db_path)?)
            .build()
            .await
            .map_err(|err| database_error(&repo_db_path, "open", err))?;
        let repo_conn = repo_db
            .connect()
            .map_err(|err| database_error(&repo_db_path, "connect", err))?;
        repo_conn
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS workspaces (
                    name TEXT PRIMARY KEY,
                    branch TEXT NOT NULL,
                    path TEXT NOT NULL UNIQUE,
                    created_at INTEGER NOT NULL
                );
                ",
            )
            .await
            .map_err(|err| database_error(&repo_db_path, "initialize schema", err))?;

        self.conn
            .execute(
                "INSERT INTO repos (host, owner, name, alias, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                (
                    repo.host.as_str(),
                    repo.owner.as_str(),
                    repo.name.as_str(),
                    repo.alias.as_deref(),
                    unix_timestamp(),
                ),
            )
            .await?;

        Ok(repo)
    }

    pub async fn list(&self) -> Result<Vec<Repository>, SwarmError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT host, owner, name, alias
                 FROM repos
                 ORDER BY host, owner, name",
            )
            .await?;
        let mut rows = stmt.query(()).await?;
        let mut repos = Vec::new();

        while let Some(row) = rows.next().await? {
            repos.push(Repository {
                host: row.get::<String>(0)?,
                owner: row.get::<String>(1)?,
                name: row.get::<String>(2)?,
                alias: row.get::<Option<String>>(3)?,
            });
        }

        Ok(repos)
    }

    pub async fn resolve_repository(
        &self,
        reference: &str,
    ) -> Result<Option<Repository>, SwarmError> {
        self.resolve(reference).await
    }

    pub fn repo_dir(&self, repo: &Repository) -> PathBuf {
        self.paths.repo_dir(repo)
    }

    pub fn repo_db_path(&self, repo: &Repository) -> PathBuf {
        self.paths.repo_db_path(repo)
    }

    pub fn workspaces_dir(&self, repo: &Repository) -> PathBuf {
        self.paths.repo_dir(repo).join("workspaces")
    }

    pub fn bare_repo_path(&self, repo: &Repository) -> PathBuf {
        self.paths.repo_dir(repo).join("source.git")
    }

    pub fn sessions_dir(&self, repo: &Repository) -> PathBuf {
        self.paths.repo_dir(repo).join("sessions")
    }

    pub async fn remove(&self, repository: &str) -> Result<Repository, SwarmError> {
        let repo = self
            .resolve(repository)
            .await?
            .ok_or_else(|| SwarmError::RepositoryNotFound(repository.to_string()))?;

        self.conn
            .execute(
                "DELETE FROM repos WHERE host = ?1 AND owner = ?2 AND name = ?3",
                (repo.host.as_str(), repo.owner.as_str(), repo.name.as_str()),
            )
            .await?;

        let repo_dir = self.paths.repo_dir(&repo);
        if repo_dir.exists() {
            fs::remove_dir_all(repo_dir)?;
        }

        Ok(repo)
    }

    async fn find_repository(&self, repo: &Repository) -> Result<Option<Repository>, SwarmError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT host, owner, name, alias
                 FROM repos
                 WHERE host = ?1 AND owner = ?2 AND name = ?3",
            )
            .await?;
        let mut rows = stmt
            .query((repo.host.as_str(), repo.owner.as_str(), repo.name.as_str()))
            .await?;

        if let Some(row) = rows.next().await? {
            return Ok(Some(Repository {
                host: row.get::<String>(0)?,
                owner: row.get::<String>(1)?,
                name: row.get::<String>(2)?,
                alias: row.get::<Option<String>>(3)?,
            }));
        }

        Ok(None)
    }

    async fn find_alias(&self, alias: &str) -> Result<bool, SwarmError> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM repos WHERE alias = ?1 LIMIT 1")
            .await?;
        let mut rows = stmt.query([alias]).await?;
        Ok(rows.next().await?.is_some())
    }

    async fn resolve(&self, reference: &str) -> Result<Option<Repository>, SwarmError> {
        if let Some(repo) = self.find_by_alias(reference).await? {
            return Ok(Some(repo));
        }

        let parsed = Repository::parse(reference, Some("placeholder"));
        if let Ok(repo) = parsed {
            return self
                .find_repository(&Repository {
                    host: repo.host,
                    owner: repo.owner,
                    name: repo.name,
                    alias: None,
                })
                .await;
        }

        Ok(None)
    }

    async fn find_by_alias(&self, alias: &str) -> Result<Option<Repository>, SwarmError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT host, owner, name, alias
                 FROM repos
                 WHERE alias = ?1
                 LIMIT 1",
            )
            .await?;
        let mut rows = stmt.query([alias]).await?;

        if let Some(row) = rows.next().await? {
            return Ok(Some(Repository {
                host: row.get::<String>(0)?,
                owner: row.get::<String>(1)?,
                name: row.get::<String>(2)?,
                alias: row.get::<Option<String>>(3)?,
            }));
        }

        Ok(None)
    }
}

fn path_to_string(path: &Path) -> Result<&str, SwarmError> {
    path.to_str().ok_or(SwarmError::PathResolution)
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn render_meta_toml(repo: &Repository) -> String {
    let mut out = String::new();
    out.push_str(&format!("host = {:?}\n", repo.host));
    out.push_str(&format!("owner = {:?}\n", repo.owner));
    out.push_str(&format!("name = {:?}\n", repo.name));
    out.push_str(&format!("canonical = {:?}\n", repo.canonical()));

    match &repo.alias {
        Some(alias) => out.push_str(&format!("alias = {:?}\n", alias)),
        None => {}
    }

    out
}

fn resolve_remote_host(host: &str) -> &str {
    match host {
        "github" => "github.com",
        "gitlab" => "gitlab.com",
        "codeberg" => "codeberg.org",
        "bitbucket" => "bitbucket.org",
        _ => host,
    }
}
