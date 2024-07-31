use crate::shared::get_archive_from_tui;
use anyhow::{anyhow, Result};
use aws_sdk_glacier::types::JobParameters;
use aws_sdk_glacier::Client;
use std::io::Write;
use std::time::Duration;
use std::{fs, thread};

const SLEEP_DURATION: u64 = 60 * 60;

async fn download_archive_by_id(
    client: &Client,
    vault_name: &String,
    archive_id: String,
    output_as: &String,
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
            println!("initiated retrieval job successfuly...");

            let job = client
                .describe_job()
                .account_id("-")
                .vault_name(vault_name)
                .job_id(init_ouput.job_id().unwrap());

            loop {
                match job.clone().send().await {
                    Ok(mut describe_output) => {
                        if describe_output.completed() {
                            println!("job {} completed", describe_output.job_id.as_mut().unwrap());

                            if let Ok(mut file) = fs::File::create(output_as) {
                                match client
                                    .get_job_output()
                                    .account_id("-")
                                    .vault_name(vault_name)
                                    .job_id(describe_output.job_id().unwrap())
                                    .send()
                                    .await
                                {
                                    Ok(archive_output) => {
                                        println!("Downloading {}", output_as);
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
                                "job is not ready - going to sleep and will try again in an hour",
                            );
                            thread::sleep(Duration::from_secs(SLEEP_DURATION))
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

pub async fn do_download(
    client: &Client,
    vault_name: &String,
    output_as: &String,
) -> Result<(), anyhow::Error> {
    match get_archive_from_tui(vault_name).await {
        Ok(archives) => {
            if archives.len() > 1 {
                return Err(anyhow!(
                    "You must select a single archive and no more than one archive to download"
                ));
            }
            let archive = archives.first().unwrap();
            match download_archive_by_id(&client, vault_name, archive.archive_id.clone(), output_as)
                .await
            {
                Ok(_success_op) => {
                    println!("Operation completed successfully")
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
