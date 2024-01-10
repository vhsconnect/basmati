mod create;
mod multipart_upload;

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
    },
}

pub mod consts {
    pub const TEMP_DIR: &str = "store";
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
async fn main() {
    let config = aws_config::load_defaults(version::v2023_11_09()).await;
    let client = aws_sdk_glacier::Client::new(&config);

    match &Cli::parse().command {
        Some(Commands::Create { vault_name }) => create::create_vault(&client, vault_name).await,
        Some(Commands::Upload {
            file_path,
            vault_name,
        }) => multipart_upload::do_multipart_upload(&client, file_path, vault_name).await,
        None => {
            println!("Nothing to do, exiting")
        }
    }
}
