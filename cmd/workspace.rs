use crate::{
    SwarmError,
    opts::{WorkspaceCommand, WorkspaceSubcommand},
    workspaces::WorkspaceStore,
};

pub async fn run(cmd: WorkspaceCommand) -> Result<(), SwarmError> {
    let store = WorkspaceStore::open().await?;

    match cmd.command {
        WorkspaceSubcommand::Create { repository, name } => {
            let workspace = store.create(&repository, name.as_deref()).await?;
            println!(
                "Created workspace {} for {}",
                workspace.name, workspace.repository_alias
            );
        }
        WorkspaceSubcommand::List { repository, json } => {
            let workspaces = store.list(&repository).await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&workspaces)?);
                return Ok(());
            }

            println!("{:<20} {:<20} PATH", "WORKSPACE", "BRANCH");
            for workspace in workspaces {
                println!(
                    "{:<20} {:<20} {}",
                    workspace.name,
                    workspace.branch,
                    workspace.path.display()
                );
            }
        }
        WorkspaceSubcommand::Info { workspace } => {
            let workspace = store.info(&workspace).await?;
            println!("Repository: {}", workspace.repository);
            println!("Workspace: {}", workspace.name);
            println!("Branch: {}", workspace.branch);
            println!("Path: {}", workspace.path.display());
            println!("Created: {}", workspace.created_at);
        }
        WorkspaceSubcommand::Remove { workspace } => {
            let workspace = store.remove(&workspace).await?;
            println!(
                "Removed workspace {} from {}",
                workspace.name, workspace.repository_alias
            );
        }
    }

    Ok(())
}
