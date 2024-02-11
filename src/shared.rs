use crossterm::ExecutableCommand;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use serde::Deserialize;
use std::io::stdout;

#[derive(Debug, Deserialize)]
pub struct ArchiveItem<'a> {
    #[serde(rename = "ArchiveId")]
    pub archive_id: &'a str,
    #[serde(rename = "ArchiveDescription")]
    archive_description: &'a str,
    #[serde(rename = "CreationDate")]
    creation_date: &'a str,
    #[serde(rename = "Size")]
    size: i64,
    #[serde(rename = "SHA256TreeHash")]
    sha256_tree_hash: &'a str,
}

pub struct Events<'a, T> {
    items: Vec<&'a T>,
    state: ListState,
}

impl<'a, T> Events<'a, T> {
    pub fn new(items: Vec<&'a T>) -> Events<'a, T> {
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

pub fn select_archive<'a>(
    mut events: Events<'a, ArchiveItem>,
) -> Result<&'a ArchiveItem<'a>, anyhow::Error> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut should_quit = false;
    let mut return_value = None;
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
