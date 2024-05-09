mod create;
mod download;
mod inventory;
mod multipart_upload;
mod shared;
use aws_config::BehaviorVersion as version;
use clap::{Parser, Subcommand};

#[derive(Subcommand)]
enum Commands {
    Create {
        #[arg(long, short)]
        vault_name: String,
    },
    Upload {
        #[arg(long, short)]
        file_path: String,
        #[arg(long, short)]
        vault_name: String,
        #[arg(long, short)]
        description: String,
    },
    Inventory {
        #[arg(long, short)]
        vault_name: String,
    },
    Download {
        #[arg(long, short)]
        vault_name: String,
        #[arg(long, short)]
        output_as: String,
    },
}

#[derive(Parser)]
#[command(author, version,  about, long_about = None)]

struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[::tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = aws_config::load_defaults(version::v2023_11_09()).await;
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
        }) => {
            download::do_download(&client, vault_name, output_as)
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
