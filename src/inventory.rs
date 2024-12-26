use anyhow::Result;
use aws_sdk_glacier::types::JobParameters;
use aws_sdk_glacier::Client;
use std::io::Write;
use std::time::Duration;
use std::{fs, thread};

use crate::shared::{basmati_directory, save_job_output, InitiatedJob};

pub async fn do_inventory(client: &Client, vault_name: &String) -> Result<()> {
    let init = client
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

    // store job_id from initiate_job.

    match init {
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
                    println!("operation succeded")
                }
                Err(err) => {
                    println!("{:?}", err)
                }
            }

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
                            let output_directory =
                                format!("{}/vault/{}", basmati_directory(), &vault_name);

                            if let Ok(mut file) =
                                fs::File::create(format!("{}/inventory.json", &output_directory))
                            {
                                match client
                                    .get_job_output()
                                    .account_id("-")
                                    .vault_name(vault_name)
                                    .job_id(describe_output.job_id().unwrap())
                                    .send()
                                    .await
                                {
                                    Ok(inventory_output) => {
                                        println!(
                                            "Writing inventory to \"{}\"",
                                            format!("{}/inventory.json", &output_directory)
                                        );
                                        let bytes = inventory_output.body.collect().await?.to_vec();
                                        file.write_all(&bytes)?;
                                        println!("Writing complete!");
                                        break Ok(());
                                    }

                                    Err(reason) => {
                                        println!("failed to get inventory output {}", reason);
                                        break Ok(());
                                    }
                                }
                            } else {
                                fs::create_dir_all(&output_directory)
                                    .expect("Could not write to file nor create directory");
                            }
                        } else {
                            println!(
                                "job is not ready - going to sleep and will try again in an hour",
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
            eprintln!("{}", reason);
            Ok(())
        }
    }
}
