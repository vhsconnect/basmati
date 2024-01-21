use crate::consts::TEMP_DIR;
use anyhow::Result;
use aws_sdk_glacier::{
    operation::initiate_multipart_upload::InitiateMultipartUploadOutput, Client,
};
use aws_smithy_types::byte_stream::ByteStream;
use colored::Colorize;
use sha256::digest;
use std::collections::VecDeque;
use std::fs::{self, DirEntry, File};
use std::io::Error as IOError;
use std::io::{Read, Write};
const CHUNK_SIZE: usize = 1048576;

fn two_digit_index(x: usize) -> String {
    if x < 10 {
        format!("0{}", x)
    } else {
        x.to_string()
    }
}

fn split_file(input_filename: &str) -> Result<u64, IOError> {
    let mut file = File::open(input_filename)?;
    let mut buffer = [0; CHUNK_SIZE];
    let mut index = 0;

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        let output_filename = format!("{}/part_{}.bin", TEMP_DIR, two_digit_index(index));
        let mut output_file = File::create(output_filename)?;

        output_file.write_all(&buffer[..bytes_read])?;
        index += 1;
    }

    Ok(file.metadata().unwrap().len())
}

async fn clean_splits() {
    match fs::read_dir(TEMP_DIR) {
        Ok(dir) => {
            for entry in dir {
                fs::remove_file(entry.unwrap().path()).unwrap();
            }
        }
        Err(reason) => {
            eprintln!("{:?}", reason)
        }
    }
    println!("{}", Colorize::green("Cleaning up"))
}

async fn send_files(
    client: &Client,
    vault_name: &String,
    output_dir: &str,
) -> Result<(InitiateMultipartUploadOutput, String), aws_sdk_glacier::Error> {
    let mut sha256_vec = VecDeque::new();

    let output = client
        .initiate_multipart_upload()
        .account_id("-")
        .vault_name(vault_name)
        .archive_description("todo")
        .part_size(CHUNK_SIZE.to_string())
        .send()
        .await?;

    match fs::read_dir(output_dir) {
        Ok(entries) => {
            let mut sorted: Vec<DirEntry> = entries.filter_map(Result::ok).collect();
            sorted.sort_by(|a, b| a.path().file_name().cmp(&b.path().file_name()));
            for (index, entry) in sorted.into_iter().enumerate() {
                let path = entry.path();
                let buffer = fs::read(&path).unwrap();
                let size = entry.metadata().unwrap().len();
                let stream = ByteStream::from_path(&path).await;
                let hash = digest(buffer);
                let hash_clone = hash.clone();
                sha256_vec.push_back(hash);

                match client
                    .upload_multipart_part()
                    .account_id("-")
                    .range(format!(
                        "bytes {}-{}/*",
                        index * CHUNK_SIZE,
                        (index as u64 * CHUNK_SIZE as u64) + size - 1
                    ))
                    .checksum(hash_clone)
                    .upload_id(output.upload_id().unwrap())
                    .vault_name(vault_name)
                    .body(stream.unwrap())
                    .send()
                    .await
                {
                    Ok(output) => {
                        println!(
                            "success uploading part {}, {}",
                            path.to_str().unwrap().green(),
                            output.checksum().unwrap().yellow()
                        )
                    }
                    Err(reason) => eprintln!("{}", reason),
                }
            }
            Ok((output, tree_hash(&sha256_vec)))
        }
        Err(reason) => panic!("Unable to read files in specified directory - {}", reason),
    }
}

pub async fn complete_multipart_upload(
    multipart_output: &InitiateMultipartUploadOutput,
    vault_name: &String,
    archive_size: &u64,
    sha256: String,
    client: &Client,
) -> Result<(), aws_sdk_glacier::Error> {
    let client = client
        .complete_multipart_upload()
        .account_id("-")
        .vault_name(vault_name)
        .checksum(sha256)
        .upload_id(multipart_output.upload_id().unwrap())
        .archive_size(archive_size.to_string());

    match client.send().await {
        Ok(output) => {
            println!(
                "success! complete multipart upload with confirmation,\narchive id: {}\nlocation: {}\nchecksum: {}",
                output.archive_id().unwrap().green(),
                output.location().unwrap().yellow(),
                output.checksum().unwrap().yellow()
            );
            Ok(())
        }
        Err(reason) => Err(reason)?,
    }
}

fn tree_hash(vec_sha: &VecDeque<String>) -> String {
    let mut queue: VecDeque<String> = vec_sha.clone();
    let mut pairs: VecDeque<String> = VecDeque::new();
    let mut inter: VecDeque<String> = VecDeque::new();
    loop {
        // new pair to digest
        if pairs.len() == 2 {
            let concat_hex = [
                pairs.pop_front().unwrap().as_bytes(),
                pairs.pop_front().unwrap().as_bytes(),
            ]
            .concat();
            let bytes = hex::decode(concat_hex).unwrap();
            inter.push_back(digest(bytes));
            continue;
        }
        // queue is done
        if queue.len() == 0 {
            if pairs.len() == 0 {
                let next = inter.clone();
                if next.len() == 1 {
                    break next[0].to_string();
                }
                queue = inter.clone();
                inter.clear();
                continue;
            }
            if pairs.len() == 1 {
                let next = inter.clone();
                if next.len() == 1 {
                    let concat_hex =
                        [inter.clone()[0].to_string(), pairs.pop_front().unwrap()].concat();
                    let bytes = hex::decode(concat_hex).unwrap();

                    break digest(bytes);
                } else if next.len() == 0 {
                    let result = pairs.clone();
                    break result[0].to_string();
                }
                inter.extend(pairs.clone());
                queue = inter.clone();
                inter.clear();
                pairs.clear();
                continue;
            }
            // queue is still full
        } else {
            let next = queue.pop_front().unwrap();
            pairs.push_back(next);
            continue;
        }
    }
}

pub async fn do_multipart_upload(
    client: &Client,
    file_path: &String,
    vault_name: &String,
) -> Result<()> {
    match split_file(&file_path) {
        Ok(archive_size) => match send_files(&client, &vault_name, TEMP_DIR).await {
            Ok((glacier_output, hash)) => {
                match complete_multipart_upload(
                    &glacier_output,
                    &vault_name,
                    &archive_size,
                    hash,
                    &client,
                )
                .await
                {
                    Ok(_output) => {
                        clean_splits().await;
                        Ok(())
                    }
                    Err(reason) => {
                        eprintln!("{}", reason);
                        clean_splits().await;
                        Ok(())
                    }
                }
            }
            Err(reason) => {
                eprintln!("{}", reason);
                Ok(())
            }
        },
        Err(reason) => {
            eprintln!("{}", reason);
            Ok(())
        }
    }
}
