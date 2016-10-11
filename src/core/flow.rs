use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::cell::RefCell;

use utils::settings::Settings;
use ui::frame::Frame;
use ui::event::{Event, Direction, SearchAction, Offset};
use ui::search::QueryState;
use core::runner::RUNNING;
use core::line::LineCollection;
use core::buffer::{Buffer, BufferCollection};
use ext::signal::{self, SIGQUIT};

pub struct Flow {
    frame: Frame,
    lines: LineCollection,
    buffer_collection: BufferCollection
}

impl Flow {
    pub fn new(settings: Settings) -> Flow {
        let menu_item_names = settings
            .config_file
            .filters
            .iter()
            .map(|tab| tab.name.clone())
            .collect();

        Flow {
            frame: Frame::new(&menu_item_names),
            lines: LineCollection::new(settings.values.max_lines_count),
            buffer_collection: BufferCollection::from_filters(settings.config_file.filters)
        }
    }

    pub fn render(&self) {
        self.frame.render();
    }

    pub fn terminate(&self) {
        self.frame.destroy();
    }

    pub fn process(&mut self, lines: Arc<Mutex<Vec<String>>>) {
        // TODO: move things into a dispatcher object
        while running!() {
            match self.frame.watch() {
                Event::SelectMenuItem(direction) => self.select_menu_item(direction),
                Event::ScrollContents(offset) => self.scroll(offset),
                Event::Navigation(state) => self.frame.navigation.change_state(state),
                Event::Search(action) => self.handle_search(action),
                Event::Resize => self.resize(),
                Event::Quit => self.quit(),
                _ => {
                    let mut mutex_guarded_lines = lines.lock().unwrap();
                    if !mutex_guarded_lines.is_empty() {
                        let pending_lines = mutex_guarded_lines.drain(..).collect();
                        self.append_incoming_lines(pending_lines);
                    }
                }
            };
        }
    }

    fn select_menu_item(&mut self, direction: Direction) {
        match direction {
            Direction::Left => {
                self.frame.select_left_menu_item();
                self.buffer_collection.select_previous();
            },
            Direction::Right => {
                self.frame.select_right_menu_item();
                self.buffer_collection.select_next();
            }
        };
        self.reset_view();
    }

    fn scroll(&mut self, offset: Offset) {
        let mut buffer = self.current_buffer().borrow_mut();
        let max_value = self.frame.plane.virtual_height() - self.frame.plane.height;

        match offset {
            Offset::Line(value) => {
                buffer.adjust_reverse_index(value, max_value);
            },
            Offset::Viewport(value) => {
                buffer.adjust_reverse_index(value * self.frame.plane.height - 4, max_value);
            },
            Offset::Top => {
                buffer.reverse_index = max_value as usize;
            },
            Offset::Bottom => {
                buffer.reset_reverse_index();
            }
        };

        self.frame.scroll(buffer.reverse_index as i32);
    }

    fn handle_search(&mut self, action: SearchAction) {
        match action {
            SearchAction::ReadInput(keys) => {
                if self.frame.navigation.search.input_field.read(keys) == QueryState::Changed {
                    self.perform_search();
                }
            },
            SearchAction::FindNextMatch => {
                self.frame.navigation.search.find_next_match();
            },
            SearchAction::FindPreviousMatch => {
                self.frame.navigation.search.find_previous_match();
            },
            SearchAction::ToggleFilterMode => {
                self.frame.navigation.search.toggle_filter_mode();
                self.perform_search();
            }
        }
    }

    fn resize(&mut self) {
        self.frame.resize();
        self.reset_scroll();
        self.reset_view();
    }

    fn append_incoming_lines(&mut self, pending_lines: Vec<String>) {
        let initial_height = self.frame.plane.virtual_height();

        self.lines.extend(pending_lines);

        self.reset_view();
        if self.current_buffer().borrow().is_scrolled() {
            let offset = self.frame.plane.virtual_height() - initial_height;
            self.scroll(Offset::Line(offset));
        }

        self.lines.clear_excess();
    }

    fn reset_view(&mut self) {
        let lines_iter = self.current_buffer().borrow().parse(&self.lines);
        self.frame.print(lines_iter, None);
        self.frame.scroll(self.current_buffer().borrow().reverse_index as i32);
    }

    fn reset_scroll(&self) {
        self.current_buffer().borrow_mut().reset_reverse_index();
    }

    fn current_buffer(&self) -> &RefCell<Buffer> {
        self.buffer_collection.selected_item()
    }

    fn perform_search(&mut self) {
        let query = self.frame.navigation.search.build_query();
        let lines_iter = self.current_buffer().borrow().parse(&self.lines);
        self.frame.print(lines_iter, query);
        self.frame.navigation.search.render();
    }

    fn quit(&self) {
        unsafe {
            signal::raise(SIGQUIT);
        }
    }
}
