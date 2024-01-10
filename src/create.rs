use aws_sdk_glacier::Client;
use colored::Colorize;

pub async fn create_vault(client: &Client, name: &str) {
    let result = client
        .create_vault()
        .account_id('-')
        .vault_name(name)
        .send()
        .await;

    match result {
        Ok(_) => println!("The following vault has been created: {}", name.yellow()),
        Err(e) => println!("something went wrong  - {:?}", e),
    }
}
