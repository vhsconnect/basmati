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
    basmati_directory, delete_expired_jobs_from_local, delete_job_from_local, get_jobs,
    save_job_output, JobType, Status,
};

const SLEEP_DURATION: u64 = 60 * 60;

pub async fn resolve_all_pending(
    client: &Client,
    job_type: JobType,
) -> Result<Status, anyhow::Error> {
    delete_expired_jobs_from_local().await?;
    let jobs = get_jobs().await?;
    let mut pending_jobs = jobs.iter().filter(|&x| x.job_type == job_type).map(|x| {
        (
            client
                .describe_job()
                .account_id("-")
                .vault_name(&x.vault)
                .job_id(&x.job_id),
            &x.vault,
        )
    });

    while let Some((describe_builder, vault)) = pending_jobs.next() {
        if let Ok((Status::Done, Some(output))) = describe_job_output(&describe_builder).await {
            let job_id = output.job_id().unwrap();
            let output_builder = client
                .get_job_output()
                .account_id("-")
                .vault_name(vault)
                .job_id(output.job_id().unwrap());

            let write_file = match job_type {
                JobType::Inventory => {
                    let output_directory = format!("{}/vault/{}", basmati_directory(), &vault);
                    fs::create_dir_all(&output_directory)
                        .expect("Could not write to file nor create directory");
                    format!("{}/inventory.json", &output_directory)
                }
                JobType::Retrieval => String::from(job_id),
            };

            let file = fs::File::create(write_file)?;
            if let Ok(Status::Done) = get_job_output(output_builder, file).await {
                delete_job_from_local(job_id.to_owned()).await?;
            } else {
                return Ok(Status::Failed);
            }
        }
    }

    Ok(Status::Done)
}

pub async fn do_inventory(client: &Client, vault_name: &String) -> Result<(), anyhow::Error> {
    match resolve_all_pending(client, JobType::Inventory).await {
        Ok(Status::Done) => {
            println!("Finished processing pending inventory jobs");
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
            save_job_output(init_ouput.clone(), JobType::Inventory)
                .await
                .expect("Was not able to save metadata");

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
                    let job_id = describe_output.job_id().unwrap();
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
                                vault_name
                            );
                            delete_job_from_local(job_id.to_owned()).await?;
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

pub async fn get_job_output(
    builder: GetJobOutputFluentBuilder,
    mut file: File,
) -> Result<Status, anyhow::Error> {
    match builder.send().await {
        Ok(output) => {
            let desc = String::from(output.archive_description().unwrap_or_else(|| "inventory"));
            let mut buffer = output.body;
            println!("{}: {}", Colorize::green("downloading"), desc);
            while let Some(bytes) = buffer.try_next().await? {
                file.write(&bytes)?;
            }
            println!("{}: {}", Colorize::green("writing complete"), desc);
            Ok(Status::Done)
        }
        Err(reason) => {
            println!("failed to get inventory output: {}", reason);
            Ok(Status::Failed)
        }
    }
}
pub async fn describe_job_loop(
    builder: DescribeJobFluentBuilder,
) -> Result<DescribeJobOutput, anyhow::Error> {
    loop {
        match describe_job_output(&builder).await {
            Ok((Status::Done, output)) => {
                break Ok(output.unwrap());
            }
            Ok((Status::Pending, _)) => {
                println!("job is not ready - going to sleep and will try again in an hour",);
                thread::sleep(Duration::from_secs(SLEEP_DURATION))
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
