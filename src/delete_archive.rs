use crate::shared::get_archive_from_tui;
use aws_sdk_glacier::Client;

pub async fn do_deletion(client: &Client, vault_name: &String) -> Result<(), anyhow::Error> {
    let archives = get_archive_from_tui(vault_name).await?;
    match crate::shared::confirm(
        String::from("Do you want to delete these archives"),
        archives
            .iter()
            .map(|x| format!(" {} created on {}", x.archive_description, x.creation_date,))
            .collect(),
    ) {
        Ok(true) => {
            let mut jobs = archives.iter().map(|archive| {
                client
                    .delete_archive()
                    .account_id("-")
                    .vault_name(vault_name)
                    .archive_id(&archive.archive_id)
            });
            while let Some(next_job) = jobs.next() {
                match next_job.send().await {
                    Ok(_) => println!("Successfully deleted"),
                    Err(reason) => {
                        println!("archive deletion failed! - {}", reason);
                    }
                }
            }
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
