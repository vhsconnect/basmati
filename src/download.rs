use anyhow::Result;
use aws_sdk_glacier::types::JobParameters;
use aws_sdk_glacier::Client;
use crossterm::ExecutableCommand;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use serde::Deserialize;
use std::io::Read;
use std::io::Write;
use std::time::Duration;
use std::{fs, io::stdout, thread};

#[derive(Debug, Deserialize)]
struct ArchiveItem<'a> {
    #[serde(rename = "ArchiveId")]
    archive_id: &'a str,
    #[serde(rename = "ArchiveDescription")]
    archive_description: &'a str,
    #[serde(rename = "CreationDate")]
    creation_date: &'a str,
    #[serde(rename = "Size")]
    size: i64,
    #[serde(rename = "SHA256TreeHash")]
    sha256_tree_hash: &'a str,
}

#[derive(Debug, Deserialize)]
struct Vault<'a> {
    #[serde(rename = "VaultARN")]
    vault_arn: &'a str,
    #[serde(rename = "InventoryDate")]
    inventory_date: &'a str,
    #[serde(rename = "ArchiveList")]
    archive_list: Vec<ArchiveItem<'a>>,
}
struct Events<'a, T> {
    items: Vec<&'a T>,
    state: ListState,
}

impl<'a, T> Events<'a, T> {
    fn new(items: Vec<&'a T>) -> Events<'a, T> {
        Events {
            items,
            state: ListState::default(),
        }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn choose(&mut self) -> Option<&'a T> {
        match self.state.selected() {
            Some(i) => Some(&self.items[i]),
            None => None,
        }
    }
}

fn ui(frame: &mut Frame) {
    frame.render_widget(
        Paragraph::new("Hello World!")
            .block(Block::default().title("Greeting").borders(Borders::ALL)),
        frame.size(),
    );
}

async fn download_archive_by_id(
    client: &Client,
    vault_name: &String,
    archive_id: &str,
) -> Result<(), anyhow::Error> {
    println!("download_archive_by_id gonna init, {}", archive_id);

    let init = client
        .initiate_job()
        .account_id("-")
        .vault_name(vault_name)
        .job_parameters(
            JobParameters::builder()
                .r#type("archive-retrieval")
                .archive_id(archive_id)
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

                            if let Ok(mut file) = fs::File::create("archive") {
                                match client
                                    .get_job_output()
                                    .account_id("-")
                                    .vault_name(vault_name)
                                    .job_id(describe_output.job_id().unwrap())
                                    .send()
                                    .await
                                {
                                    Ok(archive_output) => {
                                        let mut stream = archive_output.body;
                                        while let Some(bytes) = stream.try_next().await? {
                                            file.write_all(&bytes).expect("Failed to write bytes");
                                        }
                                        println!("Writing complete!");

                                        break Ok(());
                                    }

                                    Err(reason) => {
                                        println!("failed to get archive output {}", reason);
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
            println!("initation failed! - {}", reason);
            Ok(())
        }
    }
}

pub async fn do_download(client: &Client, vault_name: &String) -> Result<(), anyhow::Error> {
    let mut file_handle = fs::File::open(format!("./vault/{}/inventory.json", &vault_name)).expect(
        "Failed to read the inventory file - have you pulled down the inventory of the vault yet?",
    );
    let mut json_data = String::new();
    file_handle
        .read_to_string(&mut json_data)
        .expect("IO error reading the file");

    let inventory: Vault = serde_json::from_str(&json_data).expect("error parsing JSON");

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut should_quit = false;
    let items = inventory.archive_list.iter().map(|x| x).collect::<Vec<_>>();

    let mut events = Events::<ArchiveItem>::new(items);
    while !should_quit {
        terminal.draw(|frame| {
            let area = frame.size();
            let list_items = events.items.iter().map(|x| x.archive_description);

            let block = Block::default()
                .title("Archives")
                .green()
                .borders(Borders::ALL);

            let list = List::new(list_items)
                .bold()
                .red()
                .block(block)
                .highlight_style(Style::new().italic())
                .highlight_symbol("->")
                .repeat_highlight_symbol(true);

            frame.render_stateful_widget(list, area, &mut events.state)
        })?;
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    should_quit = true;
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('j') {
                    events.next();
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('k') {
                    events.previous();
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Enter {
                    match events.choose() {
                        Some(value) => {
                            should_quit = true;
                            disable_raw_mode()?;
                            stdout().execute(LeaveAlternateScreen)?;
                            println!("{}", value.archive_id);
                            match download_archive_by_id(&client, vault_name, value.archive_id)
                                .await
                            {
                                Ok(success_op) => {
                                    println!("{:?}", success_op)
                                }
                                Err(reason) => {
                                    println!("{:?}", reason)
                                }
                            }
                        }
                        None => (),
                    }
                }
            }
        }
    }
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
