use colored::Colorize;
use crossterm::ExecutableCommand;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use home::home_dir;
use ratatui::{prelude::*, widgets::*};
use serde::Deserialize;
use std::io::stdout;
use std::io::Read;

#[derive(Debug, Deserialize, Clone)]
pub struct ArchiveItem {
    #[serde(rename = "ArchiveId")]
    pub archive_id: String,
    #[serde(rename = "ArchiveDescription")]
    archive_description: String,
    #[serde(rename = "CreationDate")]
    creation_date: String,
    #[serde(rename = "Size")]
    size: i64,
    #[serde(rename = "SHA256TreeHash")]
    sha256_tree_hash: String,
}

#[derive(Debug, Deserialize)]
struct Vault<'a> {
    #[serde(rename = "VaultARN")]
    vault_arn: &'a str,
    #[serde(rename = "InventoryDate")]
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

pub async fn create_if_not_exists(path: &str) {
    match std::fs::create_dir_all(&path) {
        Ok(_) => println!("Created the following directory successfully - {}", &path),
        Err(_) => clean_splits(path).await,
    }
}

pub async fn clean_splits(temp_dir: &str) {
    match std::fs::read_dir(temp_dir) {
        Ok(dir) => {
            for entry in dir {
                std::fs::remove_file(entry.unwrap().path()).unwrap();
            }
            println!("{}", Colorize::yellow("Temporary files deleted"))
        }
        Err(reason) => {
            eprintln!("{:?}", reason)
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

pub async fn get_archive_from_tui(vault_name: &String) -> Result<ArchiveItem, anyhow::Error> {
    let mut file_handle = std::fs::File::open(format!(
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

    crate::shared::select_archive(events)
}

pub fn select_archive(mut events: Events<ArchiveItem>) -> Result<ArchiveItem, anyhow::Error> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut should_quit = false;
    let mut return_value = None;
    while !should_quit {
        terminal.draw(|frame| {
            let area = frame.size();
            let list_items: Vec<&str> = events
                .items
                .iter()
                .map(|x| x.archive_description.as_str())
                .collect();

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
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Esc {
                    should_quit = true;
                }
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Enter {
                    match events.choose() {
                        Some(value) => {
                            should_quit = true;
                            disable_raw_mode()?;
                            stdout().execute(LeaveAlternateScreen)?;
                            disable_raw_mode()?;
                            stdout()
                                .execute(LeaveAlternateScreen)
                                .expect("failed releasing terminal");
                            return_value = Some(value);
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
    Ok(return_value.unwrap())
}
