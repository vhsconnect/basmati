use anyhow::Result;
use aws_sdk_glacier::types::JobParameters;
use aws_sdk_glacier::Client;
use std::io::Write;
use std::time::Duration;
use std::{fs, thread};

pub async fn do_inventory(client: &Client, vault_name: &String, desc: &String) -> Result<()> {
    let init = client
        .initiate_job()
        .account_id("-")
        .vault_name(vault_name)
        .job_parameters(
            JobParameters::builder()
                .r#type("inventory-retrieval")
                .description(desc)
                .format("JSON")
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

                            if let Ok(mut file) = fs::File::create("inventory.json") {
                                match client
                                    .get_job_output()
                                    .account_id("-")
                                    .vault_name(vault_name)
                                    .job_id(describe_output.job_id().unwrap())
                                    .send()
                                    .await
                                {
                                    Ok(inventory_output) => {
                                        let bytes = inventory_output.body.collect().await?.to_vec();
                                        // let mut file = File::create("output.json").await?;
                                        file.write_all(&bytes)?;
                                        break Ok(());
                                    }

                                    Err(reason) => {
                                        println!("failed to get inventory output {}", reason);
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
            eprintln!("{}", reason);
            Ok(())
        }
    }
}
