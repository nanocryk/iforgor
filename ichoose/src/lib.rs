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
            Block, HighlightSpacing, List, ListState, Padding, Paragraph, Wrap,
        },
        Terminal,
    },
    std::{collections::BTreeSet, io},
    tap::Tap,
};

// type Tui = Terminal<CrosstermBackend<io::Stdout>>;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct ListEntry<K> {
    pub key: K,
    pub name: String,
}

pub struct ListSearch<'a, K> {
    /// Title of the box.
    title: String,
    /// Text displayed at the top of the box, usually giving context for the selection.
    text: String,

    /// The main list we want to search into.
    main_list: &'a [ListEntry<K>],
    /// List showed if the search input is empty (history for iforgor).
    empty_search_list: Option<&'a [ListEntry<K>]>,

    /// List currently being displayed (filtered by search).
    /// Used to properly find which entry is selected when pressing Enter.
    displayed_list: Vec<&'a ListEntry<K>>,
    /// Content of the search input field.
    search_input: String,

    /// Is multiselection enabled.
    multi_select: bool,
    /// Set of selected items.
    selected_items: BTreeSet<K>,

    /// State of the TUI List.
    ui_list_state: ListState,

    /// Should the TUI exit?
    exit: bool,
}

impl<'a, K: Ord + Clone> ListSearch<'a, K> {
    pub fn new(list: &'a [ListEntry<K>]) -> Self {
        Self {
            title: String::new(),
            text: String::new(),
            main_list: list,
            empty_search_list: None,
            displayed_list: Vec::new(),
            search_input: String::new(),
            multi_select: false,
            selected_items: BTreeSet::new(),
            ui_list_state: ListState::default().tap_mut(|v| v.select(Some(0))),
            exit: false,
        }
    }

    pub fn title(self, title: String) -> Self {
        self.tap_mut(|v| v.title = title)
    }

    pub fn text(self, text: String) -> Self {
        self.tap_mut(|v| v.text = text)
    }

    pub fn multi_select(self, multi: bool) -> Self {
        self.tap_mut(|v| v.multi_select = multi)
    }

    pub fn empty_search_list(self, list: Option<&'a [ListEntry<K>]>) -> Self {
        self.tap_mut(|v| v.empty_search_list = list)
    }

    fn update_displayed_list(&mut self) {
        if let Some(alt_list) = self.empty_search_list {
            if self.search_input.is_empty() {
                self.displayed_list = alt_list.iter().collect();
                return;
            }
        }

        let search = self.search_input.to_lowercase();
        let search: Vec<_> = search.split(',').map(|s| s.trim()).collect();

        self.displayed_list = self
            .main_list
            .iter()
            .filter(|item| search_filter(&item.name, &search))
            .collect::<Vec<_>>()
            .tap_mut(|v| v.sort_by_key(|item| &item.name));
    }

    pub fn run(self) -> io::Result<BTreeSet<K>> {
        let mut stderr = io::stderr();

        execute!(stderr, EnterAlternateScreen)?;
        enable_raw_mode()?;

        let mut terminal = Terminal::new(CrosstermBackend::new(stderr.lock()))?;

        let output = self.run_inner(&mut terminal);

        execute!(stderr, LeaveAlternateScreen)?;
        disable_raw_mode()?;
        output
    }

    fn run_inner<T: ratatui::backend::Backend>(
        mut self,
        terminal: &mut Terminal<T>,
    ) -> io::Result<BTreeSet<K>> {
        self.update_displayed_list();

        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events()?;
        }

        Ok(self.selected_items)
    }

    fn render_frame(&mut self, frame: &mut Frame) {
        frame.render_widget(self, frame.size());
    }

    fn handle_events(&mut self) -> io::Result<()> {
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

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => {
                self.selected_items = BTreeSet::new();
                self.exit = true;
            }
            KeyCode::Enter => {
                self.exit = true;

                if self.multi_select {
                    return;
                }

                let Some(selected_index) = self.ui_list_state.selected() else {
                    return;
                };

                let Some(item) = self.displayed_list.get(selected_index) else {
                    return;
                };

                self.selected_items.insert(item.key.clone());
            }
            KeyCode::Char(c) => {
                self.search_input.push(c);
                self.ui_list_state.select(Some(0));
                self.update_displayed_list();
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                self.ui_list_state.select(Some(0));
                self.update_displayed_list();
            }
            KeyCode::Up => {
                self.ui_list_state.select_previous();
            }
            KeyCode::Down => {
                self.ui_list_state.select_next();
            }
            KeyCode::Left if self.multi_select => {
                if self
                    .displayed_list
                    .iter()
                    .any(|item| self.selected_items.contains(&item.key))
                {
                    for item in self.displayed_list.iter() {
                        self.selected_items.remove(&item.key);
                    }
                } else {
                    for item in self.displayed_list.iter() {
                        self.selected_items.insert(item.key.clone());
                    }
                }
            }
            KeyCode::Right if self.multi_select => {
                let Some(selected_index) = self.ui_list_state.selected() else {
                    return;
                };

                let Some(item) = self.displayed_list.get(selected_index) else {
                    return;
                };

                if self.selected_items.contains(&item.key) {
                    self.selected_items.remove(&item.key);
                } else {
                    self.selected_items.insert(item.key.clone());
                }
            }
            _ => {}
        }
    }
}

impl<'a, K: Ord + Clone> Widget for &mut ListSearch<'a, K> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Display outer block
        let title = Title::from(self.title.as_str().bold().magenta());

        let mut instructions = Vec::new();
        instructions.add_instruction("Change Line", "Up/Down");

        if self.multi_select {
            instructions.add_instruction("Toogle select", "Right");
            instructions.add_instruction("Toogle all", "Left");
        }

        instructions.add_instruction("Confirm", "Enter");
        instructions.add_instruction("Quit", "Esc");

        instructions.push(" ".into());

        let instructions = Title::from(instructions);
        let block = Block::bordered()
            .title(title.alignment(Alignment::Center))
            .title(
                instructions
                    .alignment(Alignment::Center)
                    .position(Position::Bottom),
            )
            .border_set(border::THICK)
            .padding(Padding::horizontal(1));

        // Layout
        let [search_bar, _padding1, list_area, _padding2, extra_text] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Max(5),
            ])
            .areas(block.inner(area));

        let [search_label, search_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(9), Constraint::Min(10)])
            .areas(search_bar);

        // Render search bar
        Line::from("Search :").render(search_label, buf);

        // Search input
        let search_input = if self.search_input.is_empty() {
            " "
        } else {
            self.search_input.as_str()
        };
        Line::from(search_input)
            .style(Style::new().underlined())
            .render(search_area, buf);

        // Render list
        let list: Vec<_> = self
            .displayed_list
            .iter()
            .map(|item| {
                if self.multi_select {
                    let c = if self.selected_items.contains(&item.key) {
                        "X"
                    } else {
                        " "
                    };
                    format!("[{c}] {}", item.name)
                } else {
                    item.name.clone()
                }
            })
            .collect();

        let list = List::new(list)
            .highlight_style(Style::new().bold().blue())
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always)
            .scroll_padding(1);
        StatefulWidget::render(&list, list_area, buf, &mut self.ui_list_state);

        // Render extra text
        Paragraph::new(self.text.as_str())
            .wrap(Wrap { trim: true })
            .style(Style::new().cyan().italic())
            .render(extra_text, buf);

        // Render block
        block.render(area, buf);
    }
}

fn search_filter(name: &str, search_items: &[&str]) -> bool {
    let name_lower = name.to_lowercase();
    for item in search_items {
        if !name_lower.contains(item) {
            return false;
        }
    }

    true
}

trait AddInstruction {
    fn add_instruction(&mut self, name: &str, keys: &str);
}

impl<'a> AddInstruction for Vec<Span<'a>> {
    fn add_instruction(&mut self, name: &str, keys: &str) {
        self.push(format!(" {name} ").into());
        self.push(format!("<{keys}>").blue().bold());
    }
}
