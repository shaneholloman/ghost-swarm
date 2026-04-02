use crate::{
    SwarmError,
    opts::{PruneCommand, PruneSubcommand},
    prune::PruneStore,
};

pub async fn run(cmd: PruneCommand) -> Result<(), SwarmError> {
    let store = PruneStore::open().await?;

    match cmd.command {
        PruneSubcommand::Sessions => {
            let pruned = store.sessions().await?;
            println!("Pruned {} sessions", pruned);
        }
    }

    Ok(())
}
