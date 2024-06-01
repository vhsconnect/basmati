use aws_sdk_glacier::Client;

pub async fn do_listing(client: &Client) -> Result<(), anyhow::Error> {
    //todo implement pagination
    let job = client
        .list_vaults()
        .account_id("-")
        .set_limit(Some(100))
        .send()
        .await;

    match job {
        Ok(success_listing) => {
            match success_listing.vault_list {
                Some(list) => {
                    list.iter()
                        .map(|x| x.vault_name())
                        .filter_map(Option::Some)
                        .map(|x| x.unwrap())
                        .for_each(|x| println!("{}", x));
                }
                None => {
                    println!("No vaults found for this account")
                }
            };
        }
        Err(reason) => {
            println!("Listing of vaults failed! - {}", reason);
        }
    };

    Ok(())
}
