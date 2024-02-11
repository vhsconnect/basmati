use crate::shared::{ArchiveItem, Events};
use anyhow::Result;
use aws_sdk_glacier::types::JobParameters;
use aws_sdk_glacier::Client;
use serde::Deserialize;
use std::io::Read;
use std::io::Write;
use std::time::Duration;
use std::{fs, thread};

#[derive(Debug, Deserialize)]
struct Vault<'a> {
    #[serde(rename = "VaultARN")]
    vault_arn: &'a str,
    #[serde(rename = "InventoryDate")]
    inventory_date: &'a str,
    #[serde(rename = "ArchiveList")]
    archive_list: Vec<ArchiveItem<'a>>,
}

async fn download_archive_by_id(
    client: &Client,
    vault_name: &String,
    archive_id: &str,
) -> Result<(), anyhow::Error> {
    println!("download_archive_by_id gonna init, {}", archive_id);

    let init = client
        .initiate_job()
        .account_id("-")
        .vault_name(vault_name)
        .job_parameters(
            JobParameters::builder()
                .r#type("archive-retrieval")
                .archive_id(archive_id)
                .build(),
        )
        .send()
        .await;

    match init {
        Ok(init_ouput) => {
            println!("initiate success! - {:?}", init_ouput);

            let job = client
                .describe_job()
                .account_id("-")
                .vault_name(vault_name)
                .job_id(init_ouput.job_id().unwrap());

            loop {
                match job.clone().send().await {
                    Ok(describe_output) => {
                        if describe_output.completed() {
                            println!("describe success jobid : {:?}", describe_output.job_id);

                            if let Ok(mut file) = fs::File::create("archive") {
                                match client
                                    .get_job_output()
                                    .account_id("-")
                                    .vault_name(vault_name)
                                    .job_id(describe_output.job_id().unwrap())
                                    .send()
                                    .await
                                {
                                    Ok(archive_output) => {
                                        let mut stream = archive_output.body;
                                        while let Some(bytes) = stream.try_next().await? {
                                            file.write_all(&bytes).expect("Failed to write bytes");
                                        }
                                        println!("Writing complete!");

                                        break Ok(());
                                    }

                                    Err(reason) => {
                                        println!("failed to get archive output {}", reason);
                                        break Ok(());
                                    }
                                }
                            }
                        } else {
                            println!(
                                "job has not completed - going to sleep and will try again {:?}",
                                describe_output
                            );
                            thread::sleep(Duration::from_secs(60 * 60))
                        }
                    }
                    Err(reason) => {
                        println!("describe fail - {:?}", reason);
                        break Ok(());
                    }
                }
            }
        }
        Err(reason) => {
            println!("initation failed! - {}", reason);
            Ok(())
        }
    }
}

pub async fn do_download(client: &Client, vault_name: &String) -> Result<(), anyhow::Error> {
    let mut file_handle = fs::File::open(format!("./vault/{}/inventory.json", &vault_name)).expect(
        "Failed to read the inventory file - have you pulled down the inventory of the vault yet?",
    );
    let mut json_data = String::new();
    file_handle
        .read_to_string(&mut json_data)
        .expect("IO error reading the file");

    let inventory: Vault = serde_json::from_str(&json_data).expect("error parsing JSON");
    let items = inventory.archive_list.iter().collect::<Vec<_>>();
    let mut events = Events::<ArchiveItem>::new(items);

    match crate::shared::select_archive(events) {
        Ok(archive) => {
            match download_archive_by_id(&client, vault_name, archive.archive_id).await {
                Ok(success_op) => {
                    println!("{:?}", success_op)
                }
                Err(reason) => {
                    println!("{:?}", reason)
                }
            }
        }
        Err(reason) => {
            println!("{:?}", reason)
        }
    }

    Ok(())
}
