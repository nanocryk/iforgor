use {
    ratatui::{
        crossterm::{
            event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
            execute,
            terminal::{
                disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
            },
        },
        prelude::*,
        symbols::border,
        widgets::{
            block::{Position, Title},
            Block, List, ListState, Padding, Paragraph, Wrap,
        },
    },
    std::{fmt::Display, io},
};

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

pub fn init() -> io::Result<Tui> {
    execute!(io::stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

/// Restore the terminal to its original state
pub fn restore() -> io::Result<()> {
    execute!(io::stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

pub fn tui_choose_in_list<'t, T: Display + Clone + Ord>(
    list: &'t [T],
    history: &'t [T],
) -> anyhow::Result<Option<&'t T>> {
    let mut terminal = init()?;
    let mut search = ListSearch::new(list, history);
    let output = search.run(&mut terminal);
    restore()?;
    output
}

enum Status {
    Continue,
    Exit,
    Selected,
}

struct ListSearch<'t, T: Display> {
    list: &'t [T],
    history: &'t [T],
    // displayed list, either history or list filtered by search
    displayed_list: Vec<&'t T>,
    list_state: ListState,
    status: Status,
    search_input: String,
}

impl<'t, T: Display + Clone + Ord> ListSearch<'t, T> {
    pub fn new(list: &'t [T], history: &'t [T]) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            list,
            history,
            displayed_list: history.iter().collect(),
            list_state,
            status: Status::Continue,
            search_input: String::new(),
        }
    }

    /// runs the application's main loop until the user quits
    pub fn run<'a>(&mut self, terminal: &'a mut Tui) -> anyhow::Result<Option<&'t T>> {
        loop {
            match self.status {
                Status::Exit => return Ok(None),
                Status::Selected => {
                    return Ok(self
                        .list_state
                        .selected()
                        .and_then(|index| self.displayed_list.get(index))
                        .map(|item| *item))
                }
                Status::Continue => (),
            }

            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events()?;
        }
    }

    fn render_frame(&mut self, frame: &mut Frame) {
        frame.render_widget(self, frame.size());
    }

    fn handle_events(&mut self) -> anyhow::Result<()> {
        match event::read()? {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };
        Ok(())
    }

    fn update_list(&mut self) {
        if self.search_input.is_empty() {
            self.displayed_list = self.history.iter().collect();
        } else {
            let mut filtered_list: Vec<_> = {
                let search = self.search_input.to_lowercase();
                let search: Vec<_> = search.split(",").map(|s| s.trim()).collect();
                self.list
                    .iter()
                    .filter(|item| search_filter(&item.to_string(), &search))
                    .collect()
            };

            filtered_list.sort();

            self.displayed_list = filtered_list;
        }
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => self.status = Status::Exit,
            KeyCode::Enter => self.status = Status::Selected,
            KeyCode::Char(c) => {
                self.search_input.push(c);
                self.list_state.select(Some(0));
                self.update_list();
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                self.list_state.select(Some(0));
                self.update_list();
            }
            KeyCode::Up => {
                self.list_state.select_previous();
            }
            KeyCode::Down => {
                self.list_state.select_next();
            }
            _ => {}
        }
    }
}

impl<'t, T: Display + Clone> Widget for &mut ListSearch<'t, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Outer border
        let title = Title::from(" iforgor ".bold());

        let instructions = Title::from(Line::from(vec![
            " Select item ".into(),
            "<Up/Down>".blue().bold(),
            " Execute ".into(),
            "<Enter>".blue().bold(),
            " Quit ".into(),
            "<Esc> ".blue().bold(),
        ]));
        let block = Block::bordered()
            .title(title.alignment(Alignment::Center))
            .title(
                instructions
                    .alignment(Alignment::Center)
                    .position(Position::Bottom),
            )
            .border_set(border::THICK)
            .padding(Padding::horizontal(1));

        // Box content layout
        let [search_bar, _padding1, list_area, _padding2, extra_text] = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(1),
                    Constraint::Max(5),
                ]
                .into_iter(),
            )
            .areas(block.inner(area));

        let [search_label, search_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(9), Constraint::Min(10)].into_iter())
            .areas(search_bar);

        // Render search bar
        Line::from("Search :").render(search_label, buf);

        // Render list

        let search_input = if self.search_input.is_empty() {
            " "
        } else {
            self.search_input.as_str()
        };

        let list: Vec<_> = self
            .displayed_list
            .iter()
            .map(|item| item.to_string())
            .collect();

        Line::from(search_input)
            .style(Style::new().bg(Color::White).fg(Color::Black))
            .render(search_area, buf);

        let list = List::new(list).highlight_symbol("> ");
        StatefulWidget::render(&list, list_area, buf, &mut self.list_state);

        // Render extra text
        Paragraph::new(
            "Run `iforgor help` to learn about subcommands. \
            Search for multiple search terms by separating them with commas `,` \
            Empty search displays history, type anything (including spaces) to \
            display the filtered full list of commands.",
        )
        .wrap(Wrap { trim: true })
        .style(Style::new().fg(Color::Cyan))
        .render(extra_text, buf);

        // Render outer border
        block.render(area, buf);
    }
}

fn search_filter(name: &str, search: &[&str]) -> bool {
    let command_name_lower = name.to_lowercase();
    for word in search {
        if !command_name_lower.contains(word) {
            return false;
        }
    }

    true
}
