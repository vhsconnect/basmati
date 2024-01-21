mod create;
mod inventory;
mod multipart_upload;
use aws_config::BehaviorVersion as version;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::io::Read;

use std::{fs, io::stdout};

use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{prelude::*, widgets::*};

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

struct Events<'a> {
    items: Vec<ArchiveItem<'a>>,
    state: ListState,
}

impl<'a> Events<'a> {
    fn new(items: Vec<ArchiveItem>) -> Events {
        Events {
            items,
            state: ListState::default(),
        }
    }

    pub fn set_items(&mut self, items: Vec<ArchiveItem<'a>>) {
        self.items = items;
        self.state = ListState::default();
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

    pub fn unselect(&mut self) {
        self.state.select(None);
    }

    pub fn choose(&mut self) -> Option<&ArchiveItem<'a>> {
        match self.state.selected() {
            Some(i) => Some(&self.items[i]),
            None => None,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    Create {
        #[arg(long, short)]
        vault_name: String,
    },
    Upload {
        #[arg(long, short)]
        file_path: String,
        #[arg(long, short)]
        vault_name: String,
    },
    Inventory {
        #[arg(long, short)]
        vault_name: String,
        #[arg(long, short)]
        desc: String,
    },
    Download {},
}

pub mod consts {
    pub const TEMP_DIR: &str = "store";
}

#[derive(Parser)]
#[command(author, version,  about, long_about = None)]

struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

fn ui(frame: &mut Frame) {
    frame.render_widget(
        Paragraph::new("Hello World!")
            .block(Block::default().title("Greeting").borders(Borders::ALL)),
        frame.size(),
    );
}

#[::tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = aws_config::load_defaults(version::v2023_11_09()).await;
    let client = aws_sdk_glacier::Client::new(&config);

    match &Cli::parse().command {
        Some(Commands::Create { vault_name }) => {
            create::create_vault(&client, vault_name).await;
            Ok(())
        }
        Some(Commands::Upload {
            file_path,
            vault_name,
        }) => {
            multipart_upload::do_multipart_upload(&client, file_path, vault_name)
                .await
                .expect("Operation Failed");
            Ok(())
        }
        Some(Commands::Inventory { vault_name, desc }) => {
            inventory::do_inventory(&client, vault_name, desc)
                .await
                .expect("Operation Failed");
            Ok(())
        }
        Some(Commands::Download {}) => {
            let mut file_handle = fs::File::open("inventory.json").expect("Failed to read the inventory file - have you pulled down the inventory of the vault yet?");
            let mut json_data = String::new();
            file_handle
                .read_to_string(&mut json_data)
                .expect("IO error reading the file");

            let inventory: Vault = serde_json::from_str(&json_data).expect("error parsing JSON");
            println!("{:?}", inventory);

            enable_raw_mode()?;
            stdout().execute(EnterAlternateScreen)?;
            let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
            let mut should_quit = false;
            let items = inventory.archive_list;
            // .iter()
            // .map(|x| x.archive_description.to_string())
            // .collect();

            let mut events = Events::new(items);
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
                        if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q')
                        {
                            should_quit = true;
                        }
                        if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('j')
                        {
                            events.next();
                        }
                        if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('k')
                        {
                            events.previous();
                        }
                        if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Enter {
                            match events.choose() {
                                Some(value) => {
                                    should_quit = true;
                                    disable_raw_mode()?;
                                    stdout().execute(LeaveAlternateScreen)?;
                                    println!("{}", value.archive_id)
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
        None => {
            println!("Nothing to do, exiting");
            Ok(())
        }
    }
}
