use anyhow::anyhow;
use aws_sdk_glacier::operation::describe_job::builders::DescribeJobFluentBuilder;
use aws_sdk_glacier::operation::describe_job::DescribeJobOutput;
use aws_sdk_glacier::operation::get_job_output::builders::GetJobOutputFluentBuilder;
use aws_sdk_glacier::types::JobParameters;
use aws_sdk_glacier::Client;
use colored::Colorize;
use std::fs::File;
use std::io::Write;
use std::time::Duration;
use std::{fs, thread};

use crate::shared::{
    basmati_directory, delete_invetory_job, get_jobs, save_job_output, InitiatedJob, JobType,
    FOURTY_EIGHT_HOURS,
};

enum Status {
    Failed = 1,
    Done = 2,
    Pending = 3,
}

async fn resolve_pending_inventory(
    client: &Client,
) -> Result<(Status, Option<String>), anyhow::Error> {
    let jobs = get_jobs().await?;
    let mut filtered = jobs
        .iter()
        .filter(|&x| chrono::Utc::now().timestamp() - x.timestamp < FOURTY_EIGHT_HOURS)
        .map(|x| {
            (
                client
                    .describe_job()
                    .account_id("-")
                    .vault_name(&x.vault)
                    .job_id(&x.job_id),
                &x.vault,
            )
        });

    while let Some((describe_builder, vault)) = filtered.next() {
        if let Ok((Status::Done, Some(output))) = describe_job_output(&describe_builder).await {
            let output_builder = client
                .get_job_output()
                .account_id("-")
                .vault_name(vault)
                .job_id(output.job_id().unwrap());

            let output_directory = format!("{}/vault/{}", basmati_directory(), &vault);

            fs::create_dir_all(&output_directory)
                .expect("Could not write to file nor create directory");
            let file = fs::File::create(format!("{}/inventory.json", &output_directory))?;
            if let Ok(Status::Done) = get_job_output(output_builder, file).await {
                delete_invetory_job(vault.to_owned()).await?;
                return Ok((Status::Done, Some(vault.to_owned())));
            } else {
                return Ok((Status::Failed, None));
            }
        }
    }

    Ok((Status::Pending, None))
}

pub async fn do_inventory(client: &Client, vault_name: &String) -> Result<(), anyhow::Error> {
    match resolve_pending_inventory(client).await {
        Ok((Status::Done, Some(vault))) => {
            println!("resolved previous job for vault {}, exiting", vault);
            return Ok(());
        }
        _ => {}
    };
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
                vault: vault.clone(),
                timestamp,
                job_type: JobType::Inventory,
            };
            match save_job_output(output_struct).await {
                Ok(_) => {
                    println!("This job is valid for 72 hours, saving...")
                }
                Err(err) => {
                    println!("{:?}", err)
                }
            }

            let describe_builder = client
                .describe_job()
                .account_id("-")
                .vault_name(vault_name)
                .job_id(init_ouput.job_id().unwrap());

            if let Ok(mut describe_output) = describe_job_loop(describe_builder.clone()).await {
                println!("job {} completed", describe_output.job_id.as_mut().unwrap());
                let output_directory = format!("{}/vault/{}", basmati_directory(), &vault_name);

                fs::create_dir_all(&output_directory)
                    .expect("Could not write to file nor create directory");
                if let Ok(file) = fs::File::create(format!("{}/inventory.json", &output_directory))
                {
                    let output_builder = client
                        .get_job_output()
                        .account_id("-")
                        .vault_name(vault_name)
                        .job_id(describe_output.job_id().unwrap());

                    match get_job_output(output_builder, file).await {
                        Ok(Status::Failed) => Ok(()),
                        Ok(Status::Pending) => Ok(()),
                        Ok(Status::Done) => {
                            println!(
                                "inventory job completed successfuly for vault {}",
                                vault.to_owned()
                            );
                            delete_invetory_job(vault.to_owned()).await?;
                            Ok(())
                        }
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

async fn get_job_output(
    builder: GetJobOutputFluentBuilder,
    mut file: File,
) -> Result<Status, anyhow::Error> {
    match builder.send().await {
        Ok(inventory_output) => {
            let bytes = inventory_output.body.collect().await?.to_vec();
            file.write_all(&bytes)?;
            println!("{}", Colorize::green("Writing complete!"));
            Ok(Status::Done)
        }
        Err(reason) => {
            println!("failed to get inventory output: {}", reason);
            Ok(Status::Failed)
        }
    }
}
async fn describe_job_loop(
    builder: DescribeJobFluentBuilder,
) -> Result<DescribeJobOutput, anyhow::Error> {
    loop {
        match describe_job_output(&builder).await {
            Ok((Status::Done, output)) => {
                break Ok(output.unwrap());
            }
            Ok((Status::Pending, _)) => {
                println!("job is not ready - going to sleep and will try again in an hour",);
                thread::sleep(Duration::from_secs(60 * 60))
            }
            _ => {
                println!("describe_job failed");
                break Err(anyhow!("describe error failed!"));
            }
        }
    }
}
async fn describe_job_output(
    builder: &DescribeJobFluentBuilder,
) -> Result<(Status, Option<DescribeJobOutput>), anyhow::Error> {
    match builder.clone().send().await {
        Ok(output) => {
            if output.completed() {
                Ok((Status::Done, Some(output)))
            } else {
                Ok((Status::Pending, None))
            }
        }
        Err(err) => Err(anyhow!(err)),
    }
}
