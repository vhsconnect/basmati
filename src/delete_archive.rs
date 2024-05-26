use crate::shared::get_archive_from_tui;
use aws_sdk_glacier::Client;
use colored::*;

pub async fn do_deletion(client: &Client, vault_name: &String) -> Result<(), anyhow::Error> {
    let archive = get_archive_from_tui(vault_name).await?;
    match crate::shared::confirm(format!(
        "delete archive: {} created on {}",
        archive.archive_description, archive.creation_date,
    )) {
        Ok(true) => {
            let job = client
                .delete_archive()
                .account_id("-")
                .vault_name(vault_name)
                .archive_id(&archive.archive_id)
                .send()
                .await;

            match job {
                Ok(_) => {
                    println!(
                        "The following archive has been deleted: {}",
                        &archive.archive_id.yellow()
                    );
                }
                Err(reason) => {
                    println!("archive deletion failed! - {}", reason);
                }
            };
        }
        Ok(false) => {
            println!("exiting");
        }
        _ => {
            println!("exiting");
        }
    }

    Ok(())
}
