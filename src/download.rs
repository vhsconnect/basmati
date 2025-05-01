use crate::inventory::resolve_all_pending;

use crate::shared::{
    describe_job_loop, get_archive_from_tui, get_job_output, save_job_output, JobType, Status,
};
use anyhow::{anyhow, Result};
use aws_sdk_glacier::types::JobParameters;
use aws_sdk_glacier::Client;
use std::fs;

async fn download_archive_by_id(
    client: &Client,
    vault_name: &String,
    archive_id: String,
    output_as: Option<String>,
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

            save_job_output(init_ouput.clone(), JobType::Retrieval)
                .await
                .expect("Was not able to save metadata");

            let describe_builder = client
                .describe_job()
                .account_id("-")
                .vault_name(vault_name)
                .job_id(init_ouput.job_id().unwrap());

            if let Ok(mut describe_output) = describe_job_loop(describe_builder.clone()).await {
                println!(
                    "job {} is ready, attempting to download",
                    describe_output.job_id.as_mut().unwrap()
                );

                let filename = match output_as {
                    Some(value) => value,
                    None => init_ouput.job_id().unwrap().to_owned(),
                };
                let file = fs::File::create(filename).expect("failed to create user defined file");
                let builder = client
                    .get_job_output()
                    .account_id("-")
                    .vault_name(vault_name)
                    .job_id(describe_output.job_id().unwrap());

                match get_job_output(builder, file).await {
                    Ok(Status::Done) => {
                        println!("Writing complete!");
                        return Ok(());
                    }
                    Err(err) => {
                        println!("failed to get archive output, {:?}", err);
                        return Ok(());
                    }
                    _ => {
                        return Ok(());
                    }
                }
            } else {
                Err(anyhow!(format!("retrieval describe job failed")))
            }
        }
        Err(reason) => {
            println!("initation failed: check your that your AWS secrets are set");
            Err(anyhow!(reason))
        }
    }
}

pub async fn do_download(
    client: &Client,
    vault_name: &Option<String>,
    output_as: &Option<String>,
    pending: &bool,
) -> Result<(), anyhow::Error> {
    if *pending {
        match resolve_all_pending(client, crate::shared::JobType::Retrieval).await {
            Ok(Status::Done) => {
                println!("Finished processing pending archive retrievals");
                return Ok(());
            }
            _ => {
                println!("Something went wrong");
                return Ok(());
            }
        }
    }
    let vault_name = String::from(
        &vault_name
            .clone()
            .expect("Expected vault_name to be defined"),
    );

    match get_archive_from_tui(&vault_name).await {
        Ok(archives) => {
            if archives.len() > 1 {
                return Err(anyhow!(
                    "You must select a single archive and no more than one archive to download"
                ));
            }

            let archive = archives.first().unwrap();
            match download_archive_by_id(
                &client,
                &vault_name,
                archive.archive_id.clone(),
                output_as.clone().to_owned(),
            )
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
