use anyhow::{anyhow, Result};
use aws_sdk_glacier::operation::get_job_output::builders::GetJobOutputFluentBuilder;
use aws_sdk_glacier::types::JobParameters;
use aws_sdk_glacier::Client;
use std::fs::File;
use std::io::Write;
use std::time::Duration;
use std::{fs, thread};

use crate::shared::{basmati_directory, save_job_output, InitiatedJob};

pub async fn do_inventory(client: &Client, vault_name: &String) -> Result<()> {
    let init_job = client
        .initiate_job()
        .account_id("-")
        .vault_name(vault_name)
        .job_parameters(
            JobParameters::builder()
                .r#type("inventory-retrieval")
                .description(vault_name)
                .format("JSON")
                .build(),
        )
        .send()
        .await;

    match init_job {
        Ok(init_ouput) => {
            println!("initiated inventory job successfuly...");
            let location = String::from(init_ouput.location().unwrap());
            let job_id = String::from(init_ouput.job_id().unwrap());
            let timestamp = chrono::Utc::now().timestamp();
            let url_parts: Vec<&str> = location.split("/").collect();
            if url_parts.len() < 3 {
                panic!("malformed url, exiting")
            }
            let vault = url_parts[3].to_owned();
            let output_struct = InitiatedJob {
                location,
                job_id,
                vault,
                timestamp,
            };
            match save_job_output(output_struct).await {
                Ok(_) => {
                    println!("This job is valid for 72 hours, saving...")
                }
                Err(err) => {
                    println!("{:?}", err)
                }
            }

            let describe_job = client
                .describe_job()
                .account_id("-")
                .vault_name(vault_name)
                .job_id(init_ouput.job_id().unwrap());

            if let Ok(mut describe_output) = loop {
                match describe_job.clone().send().await {
                    Ok(output) => {
                        if output.completed() {
                            break Ok(output);
                        } else {
                            println!(
                                "job is not ready - going to sleep and will try again in an hour",
                            );
                            thread::sleep(Duration::from_secs(60 * 60))
                        }
                    }
                    Err(reason) => break Err(anyhow!(format!("describe_job failed: {}", reason))),
                }
            } {
                println!("job {} completed", describe_output.job_id.as_mut().unwrap());
                let output_directory = format!("{}/vault/{}", basmati_directory(), &vault_name);

                fs::create_dir_all(&output_directory)
                    .expect("Could not write to file nor create directory");
                if let Ok(file) = fs::File::create(format!("{}/inventory.json", &output_directory))
                {
                    let builder = client
                        .get_job_output()
                        .account_id("-")
                        .vault_name(vault_name)
                        .job_id(describe_output.job_id().unwrap());

                    match get_job_output(builder, file).await {
                        Ok(Status::Failed) => Ok(()),
                        Ok(Status::Done) => Ok(()),
                        Err(reason) => {
                            println!("failed to get inventory output {}", reason);
                            Ok(())
                        }
                    }
                } else {
                    Err(anyhow!(format!(
                        "Could not create inventory file in {}",
                        &output_directory
                    )))
                }
            } else {
                Err(anyhow!(format!("inventory describe job failed")))
            }
        }
        Err(reason) => {
            eprintln!("{}", reason);
            Ok(())
        }
    }
}

enum Status {
    Failed = 1,
    Done = 2,
}

async fn get_job_output(
    builder: GetJobOutputFluentBuilder,
    mut file: File,
) -> Result<Status, anyhow::Error> {
    match builder.send().await {
        Ok(inventory_output) => {
            let bytes = inventory_output.body.collect().await?.to_vec();
            file.write_all(&bytes)?;
            println!("Writing complete!");
            Ok(Status::Done)
        }
        Err(reason) => {
            println!("failed to get inventory output: {}", reason);
            Ok(Status::Failed)
        }
    }
}
