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
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
const ONE_MB: usize = 1048576;
const MAX_PART_AMOUNT: u64 = 10000;

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

fn calculate_optimal_interval(file: &File) -> Result<u64, anyhow::Error> {
    let sizes = [
        1048576 * 16,
        1048576 * 32,
        1048576 * 64,
        1048576 * 128,
        1048576 * 256,
        1048576 * 562,
        1048576 * 1024,
        1048576 * 2048,
        1048576,
        1048576 * 2,
        1048576 * 4,
        1048576 * 8,
    ];

    let file_size = file.metadata().unwrap().size();
    let mut i = sizes.into_iter();

    while let Some(x) = i.next() {
        if file_size > x && file_size / x < MAX_PART_AMOUNT {
            println!("Splitting the archive in chunks of {} bytes", x);
            return Ok(x);
        }
    }
    Err(anyhow::anyhow!(
        "Ensure that archive is between 1 MB and 40,000 GB"
    ))
}

async fn split_file(
    input_filename: &str,
) -> Result<(u64, String, Vec<String>, u64), anyhow::Error> {
    let temp_dir = format!("{}/TMP/{}", basmati_directory(), digest(input_filename));
    println!("creating temporary directory");
    create_if_not_exists(&temp_dir).await;

    let mut file = File::open(input_filename)?;
    let chunk_size = calculate_optimal_interval(&file)?;
    println!(
        "Spliting a {} bytes archive into {:} parts",
        (file.metadata().unwrap().size()),
        file.metadata().unwrap().size() / chunk_size + 1
    );
    let mut buffer = vec![0; chunk_size.try_into()?];
    let mut i = InfiniteIndeces::new();
    let mut sha256_vec = Vec::new();

    loop {
        let ind = i.next();
        let bytes_read = file.read(&mut buffer)?;

        let chunks: Vec<String> = buffer
            // last itteration has zeroed data at end of buffer
            [0..buffer.iter().rposition(|&x| x != 0).map_or(0, |x| x + 1)]
            .chunks(ONE_MB)
            .to_owned()
            .map(|x| digest(x))
            .collect();

        if bytes_read == 0 {
            break;
        }

        sha256_vec = [&sha256_vec[..], &chunks[..]].concat();

        let mut output_file = File::create(format!("{}/part_{}.bin", temp_dir, ind))?;

        output_file.write_all(&buffer[..bytes_read])?;
        buffer.fill(0);
    }

    Ok((
        file.metadata().unwrap().len(),
        temp_dir,
        sha256_vec,
        chunk_size,
    ))
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
    chunk_size: u64,
) -> Result<InitiateMultipartUploadOutput, aws_sdk_glacier::Error> {
    let output = client
        .initiate_multipart_upload()
        .account_id("-")
        .vault_name(vault_name)
        .archive_description(description)
        .part_size(chunk_size.to_string())
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
                let size = entry.metadata().unwrap().len();
                let stream = ByteStream::from_path(&path).await;

                match client
                    .upload_multipart_part()
                    .account_id("-")
                    .range(format!(
                        "bytes {}-{}/*",
                        index as u64 * chunk_size,
                        (index as u64 * chunk_size as u64) + size - 1
                    ))
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
            Ok(output)
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
    match split_file(&file_path).await {
        Ok((archive_size, temp_dir, sha256_vec, chunk_size)) => {
            println!("Starting data upload");
            match send_files(
                &client,
                &vault_name,
                temp_dir.as_str(),
                description,
                chunk_size,
            )
            .await
            {
                Ok(glacier_output) => {
                    match complete_multipart_upload(
                        &glacier_output,
                        &vault_name,
                        &archive_size,
                        tree_hash(&VecDeque::from(sha256_vec)),
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
