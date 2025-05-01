use anyhow::anyhow;
use aws_sdk_glacier::operation::describe_job::builders::DescribeJobFluentBuilder;
use aws_sdk_glacier::operation::describe_job::DescribeJobOutput;
use aws_sdk_glacier::operation::initiate_job::InitiateJobOutput;
use colored::Colorize;
use std::fs::File;
use std::time::Duration;

use aws_sdk_glacier::operation::get_job_output::builders::GetJobOutputFluentBuilder;
use crossterm::terminal::size;
use crossterm::ExecutableCommand;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use home::home_dir;
use ratatui::{prelude::*, widgets::*};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::io::{stdout, Write};
use std::time;
use std::{fs, thread};

pub const TWENTY_FOUR_HOURS: i64 = 86400;
const SLEEP_DURATION: u64 = 60 * 60;

pub enum Status {
    Failed = 1,
    Done = 2,
    Pending = 3,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum JobType {
    Inventory = 1,
    Retrieval = 2,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ArchiveItem {
    #[serde(rename = "ArchiveId")]
    pub archive_id: String,
    #[serde(rename = "ArchiveDescription")]
    pub archive_description: String,
    #[serde(rename = "CreationDate")]
    pub creation_date: String,
    #[serde(rename = "Size")]
    #[allow(dead_code)]
    size: i64,
    #[serde(rename = "SHA256TreeHash")]
    #[allow(dead_code)]
    sha256_tree_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitiatedJob {
    pub location: String,
    pub job_id: String,
    pub vault: String,
    pub timestamp: i64,
    pub job_type: JobType,
}

#[derive(Debug, Deserialize)]
struct Vault<'a> {
    #[serde(rename = "VaultARN")]
    #[allow(dead_code)]
    vault_arn: &'a str,
    #[serde(rename = "InventoryDate")]
    #[allow(dead_code)]
    inventory_date: &'a str,
    #[serde(rename = "ArchiveList")]
    archive_list: Vec<ArchiveItem>,
}

#[derive(Clone)]
pub struct Events<T: Clone> {
    items: Vec<T>,
    state: ListState,
}

impl<T: std::clone::Clone> Events<T> {
    pub fn new(items: Vec<T>) -> Events<T> {
        Events {
            items,
            state: ListState::default().with_selected(Some(0)),
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

    pub fn choose(&mut self) -> Option<T> {
        match self.state.selected() {
            Some(i) => Some(self.items[i].clone()),
            None => None,
        }
    }
}

pub fn basmati_directory() -> String {
    match home_dir() {
        Some(path) => format!("{}/.basmati", path.display()),
        None => panic!("Can not find home directory"),
    }
}

pub async fn delete_job_from_local(job_id: String) -> Result<(), anyhow::Error> {
    let jobs = get_jobs().await?;
    let jobs: Vec<InitiatedJob> = jobs.into_iter().filter(|x| x.job_id != job_id).collect();
    let buffer = serde_json::to_vec(&jobs)?;
    job_writer(buffer).await?;
    Ok(())
}

pub async fn delete_expired_jobs_from_local() -> Result<(), anyhow::Error> {
    let jobs = get_jobs().await?;
    let jobs: Vec<InitiatedJob> = jobs
        .into_iter()
        .filter(|x| chrono::Utc::now().timestamp() - x.timestamp < TWENTY_FOUR_HOURS)
        .collect();
    let buffer = serde_json::to_vec(&jobs)?;
    job_writer(buffer).await?;
    Ok(())
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
                println!(
                    "job is not ready - going to sleep and will try again in {} minutes",
                    SLEEP_DURATION / 60
                );
                thread::sleep(Duration::from_secs(SLEEP_DURATION))
            }
            _ => {
                println!("describe_job failed");
                break Err(anyhow!("describe job failed!"));
            }
        }
    }
}

pub async fn describe_job_output(
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

pub async fn get_jobs() -> Result<Vec<InitiatedJob>, anyhow::Error> {
    let job_dir = format!("{}/jobs", basmati_directory());
    let path = format!("{}/jobs.json", job_dir);
    let file_handle = fs::OpenOptions::new()
        .read(true) //
        .open(&path);
    match file_handle {
        Ok(mut file) => {
            let mut serialized_items = String::new();
            file.read_to_string(&mut serialized_items)?;
            let current_items: Vec<InitiatedJob> =
                serde_json::from_str(&serialized_items).unwrap_or_else(|_| vec![]);
            Ok(current_items)
        }
        Err(_) => Ok(vec![]),
    }
}

pub async fn job_writer(bytes: Vec<u8>) -> Result<(), anyhow::Error> {
    let job_dir = format!("{}/jobs", basmati_directory());
    create_if_not_exists(&job_dir).await;
    let path = format!("{}/jobs.json", job_dir);
    let mut file = fs::OpenOptions::new()
        .truncate(true)
        .write(true)
        .create(true)
        .open(&path)?;
    file.write_all(&bytes)?;
    Ok(())
}

pub async fn save_job_output(
    init_job_output: InitiateJobOutput,
    job_type: JobType,
) -> Result<(), anyhow::Error> {
    let location = String::from(init_job_output.location().unwrap());
    let job_id = String::from(init_job_output.job_id().unwrap());
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
        job_type,
    };

    let mut jobs = get_jobs().await.unwrap();
    jobs.push(output_struct);
    let buffer = serde_json::to_vec(&jobs)?;
    job_writer(buffer).await?;
    Ok(())
}

pub async fn create_if_not_exists(path: &str) {
    if let Err(err) = fs::create_dir_all(&path) {
        println!("{:?}", err);
        clean_splits(path).await
    }
}

pub async fn clean_splits(temp_dir: &str) {
    match fs::read_dir(temp_dir) {
        Ok(dir) => {
            for entry in dir {
                fs::remove_file(entry.unwrap().path()).unwrap();
            }
            println!("{}", Colorize::yellow("Temporary files deleted"))
        }
        Err(reason) => {
            eprintln!("{:?}", reason)
        }
    }
}

pub async fn get_archive_from_tui(vault_name: &String) -> Result<Vec<ArchiveItem>, anyhow::Error> {
    let mut file_handle = fs::File::open(format!(
        "{}/vault/{}/inventory.json",
        basmati_directory(),
        &vault_name
    ))
    .expect(
        "Failed to read the inventory file - have you pulled down the inventory of the vault yet?",
    );
    let mut json_data = String::new();
    file_handle
        .read_to_string(&mut json_data)
        .expect("IO error reading the file");

    let inventory: Vault = serde_json::from_str(&json_data).expect("error parsing JSON");
    let items = inventory.archive_list.into_iter().collect::<Vec<_>>();
    let events = Events::<ArchiveItem>::new(items);

    select_multiple_archives(events)
}

pub fn confirm(title: String, confirmation_items: Vec<String>) -> Result<bool, anyhow::Error> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut should_quit = false;
    let mut return_value = None;
    let confrim_options = vec![String::from("yes"), String::from("no")];
    let mut events = Events::new(confrim_options);
    let terminal_width = size()?.0;
    let terminal_height = size()?.1;
    let midpoint = terminal_width / 2;
    while !should_quit {
        let list_items = events.items.clone();
        terminal.draw(|frame| {
            let areas = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(midpoint), Constraint::Length(midpoint)])
                .split(Rect::new(0, 0, terminal_width, terminal_height));
            let block = Block::default()
                .title(title.clone())
                .green()
                .borders(Borders::ALL);
            let text: Vec<Line> = confirmation_items
                .clone()
                .into_iter()
                .map(|x| {
                    Line::from(Span::styled(
                        String::from(x),
                        Style::default().fg(Color::Red),
                    ))
                })
                .collect();
            let display_text = Paragraph::new(text).block(Block::default());

            let list = List::new(list_items)
                .bold()
                .red()
                .block(block)
                .highlight_style(Style::new().italic())
                .highlight_symbol("->")
                .repeat_highlight_symbol(true);

            frame.render_widget(display_text, areas[0]);
            frame.render_stateful_widget(list, areas[1], &mut events.state);
        })?;
        if event::poll(time::Duration::from_millis(50))? {
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
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Down {
                    events.next();
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Up {
                    events.previous();
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Esc {
                    should_quit = true;
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Enter {
                    match events.choose() {
                        Some(value) => {
                            should_quit = true;
                            match value.as_str() {
                                "yes" => {
                                    return_value = Some(true);
                                }
                                "no" => {
                                    return_value = Some(false);
                                }
                                _ => panic!("unexpected selection"),
                            }
                        }
                        None => {
                            should_quit = true;
                            println!("could not match user input")
                        }
                    }
                }
            }
        }
    }
    release_terminal().expect("Issue releasing terminal");
    Ok(return_value.unwrap())
}

pub fn select_multiple_archives(
    mut events: Events<ArchiveItem>,
) -> Result<Vec<ArchiveItem>, anyhow::Error> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut should_quit = false;
    let mut return_values: Vec<ArchiveItem> = vec![];
    while !should_quit {
        terminal.draw(|frame| {
            let area = frame.size();
            let list_items: Vec<ListItem> = events
                .items
                .iter()
                .map(|x| {
                    if return_values
                        .clone()
                        .iter()
                        .map(|y| y.archive_id.as_str())
                        .collect::<Vec<&str>>()
                        .contains(&x.archive_id.as_str())
                    {
                        ListItem::new(Span::styled(
                            format!("{} {} {}", x.archive_description, x.archive_id, " *"),
                            Style::default().fg(Color::Green),
                        ))
                    } else {
                        ListItem::new(Span::styled(
                            format!("{} {}", x.archive_description.clone(), x.archive_id.clone()),
                            Style::default().fg(Color::White),
                        ))
                    }
                })
                .collect();

            let block = Block::default()
                .title("Archives | <Space> to select <Enter> to confirm selection")
                .green()
                .borders(Borders::ALL);

            let list = List::new(list_items)
                .bold()
                // .green()
                .block(block)
                .highlight_style(Style::new().italic())
                .highlight_symbol("->")
                .repeat_highlight_symbol(true);

            frame.render_stateful_widget(list, area, &mut events.state)
        })?;
        if event::poll(time::Duration::from_millis(50))? {
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
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Down {
                    events.next();
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Up {
                    events.previous();
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Esc {
                    should_quit = true;
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char(' ') {
                    match events.choose() {
                        Some(value) => {
                            let result_includes_item = return_values
                                .clone()
                                .iter()
                                .map(|y| y.archive_id.as_str())
                                .collect::<Vec<&str>>()
                                .contains(&value.archive_id.as_str());
                            if result_includes_item {
                                return_values = return_values
                                    .clone()
                                    .into_iter()
                                    .filter(|x| value.archive_id != x.archive_id)
                                    .collect();
                            } else {
                                return_values.push(value);
                            }
                        }

                        None => {
                            should_quit = true;
                            println!("could not match user input")
                        }
                    }
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Enter {
                    should_quit = true;
                }
            }
        }
    }
    release_terminal().expect("Issue releasing the terminal");
    if return_values.len() >= 1 {
        return Ok(return_values);
    }
    Err(anyhow!("You must select an archive!"))
}

fn release_terminal() -> Result<(), anyhow::Error> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    stdout()
        .execute(LeaveAlternateScreen)
        .expect("failed releasing terminal");
    Ok(())
}

pub struct InfiniteIndeces {
    value: usize,
}

impl InfiniteIndeces {
    pub fn new() -> Self {
        InfiniteIndeces { value: 0 }
    }
    pub fn next(&mut self) -> usize {
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
