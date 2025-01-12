mod create;
mod delete_archive;
mod download;
mod inventory;
mod list_vaults;
mod multipart_upload;
mod shared;
use aws_config::BehaviorVersion as version;
use clap::{Parser, Subcommand};

#[derive(Subcommand)]
enum Commands {
    /// Create a vault by supplying a name
    Create {
        #[arg(long, short)]
        vault_name: String,
    },
    ///  Upload an archive at path to a particular vault with a particular description
    Upload {
        #[arg(long, short)]
        file_path: String,
        #[arg(long, short)]
        vault_name: String,
        #[arg(long, short)]
        description: String,
    },
    ///  Get the inventory of a particular vault
    Inventory {
        #[arg(long, short)]
        vault_name: String,
    },
    ///  Download a job
    Download {
        #[arg(long, short, required_unless_present = "pending")]
        /// Required if not finishing pending jobs - you will be prompted to select an archive from
        /// a list. List will be empty if you have not queried for inventory first
        vault_name: Option<String>,
        #[arg(long, short, default_value = None)]
        /// Optional: Where to write out the archive to
        output_as: Option<String>,
        /// Pass this option to finish a job you started earlier
        #[arg(long, short, exclusive = true)]
        pending: bool,
    },

    ///  Delete a particular archive by selecting it from an archive.
    DeleteArchive {
        #[arg(long, short)]
        vault_name: String,
    },
    /// List vaults
    ListVaults {},
}

#[derive(Parser)]
#[command(
    author,
    version,
    about,
    long_about = "Get inventory, upload/download/delete an archive and more"
)]

struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[::tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = aws_config::load_defaults(version::v2024_03_28()).await;
    let client = aws_sdk_glacier::Client::new(&config);

    match &Cli::parse().command {
        Some(Commands::Create { vault_name }) => {
            create::create_vault(&client, vault_name).await;
            Ok(())
        }
        Some(Commands::Upload {
            file_path,
            vault_name,
            description,
        }) => {
            multipart_upload::do_multipart_upload(&client, file_path, vault_name, description)
                .await
                .expect("Operation Failed");
            Ok(())
        }
        Some(Commands::Inventory { vault_name }) => {
            inventory::do_inventory(&client, vault_name)
                .await
                .expect("Operation Failed");
            Ok(())
        }
        Some(Commands::Download {
            vault_name,
            output_as,
            pending,
        }) => {
            download::do_download(&client, vault_name, output_as, pending)
                .await
                .expect("Operation Failed");
            Ok(())
        }
        Some(Commands::DeleteArchive { vault_name }) => {
            delete_archive::do_deletion(&client, vault_name)
                .await
                .expect("Operation Failed");
            Ok(())
        }
        Some(Commands::ListVaults {}) => {
            list_vaults::do_listing(&client)
                .await
                .expect("Operation Failed");
            Ok(())
        }
        None => {
            println!("Nothing to do, exiting");
            Ok(())
        }
    }
}
