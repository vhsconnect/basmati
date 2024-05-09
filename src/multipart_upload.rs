use crate::shared::{basmati_directory, clean_splits, create_if_not_exists};
use anyhow::Result;
use aws_sdk_glacier::{
    operation::initiate_multipart_upload::InitiateMultipartUploadOutput, Client,
};
use aws_smithy_types::byte_stream::ByteStream;
use colored::Colorize;
use regex::Regex;
use sha256::digest;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::{self, DirEntry, File};
use std::io::Error as IOError;
use std::io::{Read, Write};
const CHUNK_SIZE: usize = 1048576;

struct InfiniteIndeces {
    value: usize,
}

impl InfiniteIndeces {
    fn new() -> Self {
        InfiniteIndeces { value: 0 }
    }
    fn next(&mut self) -> usize {
        self.value = self.value + 1;
        self.value
    }
}
#[test]
fn test_infinite_indeces() {
    let mut i = InfiniteIndeces::new();
    assert_eq!(i.next(), 1);
    assert_eq!(i.next(), 2);
    assert_eq!(i.next(), 3);
    assert_eq!(i.next(), 4);
    assert_eq!(i.next(), 5);
    assert_eq!(i.next(), 6);
    assert_eq!(i.next(), 7);
    assert_eq!(i.next(), 8);
    assert_eq!(i.next(), 9);
    assert_eq!(i.next(), 10);
    assert_eq!(i.next(), 11);
}

fn split_file(input_filename: &str) -> Result<(u64, String), IOError> {
    let temp_dir = format!("{}/TMP/{}", basmati_directory(), digest(input_filename));
    println!("creating directory");
    create_if_not_exists(&temp_dir);
    println!("created/cleaned up directory");

    let mut file = File::open(input_filename)?;
    println!("opened file");
    let mut buffer = [0; CHUNK_SIZE];
    let mut i = InfiniteIndeces::new();

    loop {
        let ind = i.next();
        println!("looping {}", ind);
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            println!("breaking!");
            break;
        }

        let output_filename = format!("{}/part_{}.bin", temp_dir, ind);
        let mut output_file = File::create(output_filename)?;

        output_file.write_all(&buffer[..bytes_read])?;
    }

    Ok((file.metadata().unwrap().len(), temp_dir))
}

fn get_index_from_filename(x: &OsStr) -> i32 {
    let re = Regex::new(r"\d+").unwrap();
    if let Some(value) = re.find(x.to_str().unwrap()) {
        let v: i32 = value.as_str().parse().unwrap();
        return v;
    }
    panic!("Unexepected File name")
}

#[test]
fn test_get_index_from_filename_valid() {
    let filename = OsStr::new("file_10.txt");
    let expected_index = 10;
    let actual_index = get_index_from_filename(filename);
    assert_eq!(expected_index, actual_index);
}

async fn send_files(
    client: &Client,
    vault_name: &String,
    output_dir: &str,
    description: &String,
) -> Result<(InitiateMultipartUploadOutput, String), aws_sdk_glacier::Error> {
    let mut sha256_vec = VecDeque::new();

    let output = client
        .initiate_multipart_upload()
        .account_id("-")
        .vault_name(vault_name)
        .archive_description(description)
        .part_size(CHUNK_SIZE.to_string())
        .send()
        .await?;

    match fs::read_dir(output_dir) {
        Ok(entries) => {
            let mut sorted: Vec<DirEntry> = entries.filter_map(Result::ok).collect();
            sorted.sort_by(|a, b| {
                get_index_from_filename(a.path().file_name().unwrap())
                    .cmp(&get_index_from_filename(b.path().file_name().unwrap()))
            });
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
                }
                if next.len() == 0 {
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
    description: &String,
) -> Result<()> {
    match split_file(&file_path) {
        Ok((archive_size, temp_dir)) => {
            println!("gonna send files");
            match send_files(&client, &vault_name, temp_dir.as_str(), description).await {
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
                            println!("{}", "upload confirmed".green());
                            clean_splits(&temp_dir).await;

                            Ok(())
                        }
                        Err(reason) => {
                            eprintln!("{}", reason);
                            clean_splits(&temp_dir).await;
                            Ok(())
                        }
                    }
                }
                Err(reason) => {
                    eprintln!("{}", reason);
                    Ok(())
                }
            }
        }
        Err(reason) => {
            eprintln!("{}", reason);
            Ok(())
        }
    }
}
