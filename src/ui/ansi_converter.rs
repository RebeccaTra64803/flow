use std::collections::HashMap;
use std::sync::Mutex;
use regex::Regex;
use ncurses::*;

use flow::line::Line;

// A negative value is interpreted as the default (original) color.
// For color pair generation, when unsigned it must also be different than the 8 colors already defined.
static COLOR_DEFAULT: i16 = -9;

pub fn init_ansi_colors() {
    let colors = [
        COLOR_BLACK,
        COLOR_RED,
        COLOR_GREEN,
        COLOR_YELLOW,
        COLOR_BLUE,
        COLOR_MAGENTA,
        COLOR_CYAN,
        COLOR_WHITE,
        COLOR_DEFAULT
    ];

    for foreground_color in colors.iter() {
        for background_color in colors.iter() {
            let id = build_color_id(foreground_color, background_color);
            init_pair(id, *foreground_color, *background_color);
        }
    }
}

fn build_color_id(foreground_color: &i16, background_color: &i16) -> i16 {
    100 + foreground_color.abs() * 10 + background_color.abs()
}

lazy_static! {
    static ref ANSI_TO_NCURSES_MAPPING: HashMap<&'static str, CursesStyle> = {
        let mut codes = HashMap::new();

        codes.insert("[0m", CursesStyle::Reset);

        codes.insert("[1m", CursesStyle::Attribute(A_BOLD, true));
        codes.insert("[3m", CursesStyle::Attribute(A_STANDOUT, true)); // Italic
        codes.insert("[4m", CursesStyle::Attribute(A_UNDERLINE, true));
        codes.insert("[7m", CursesStyle::Attribute(A_REVERSE, true));
        codes.insert("[9m", CursesStyle::Attribute(A_DIM, true)); // Strikethrough

        codes.insert("[22m", CursesStyle::Attribute(A_BOLD, false));
        codes.insert("[23m", CursesStyle::Attribute(A_STANDOUT, false)); // Italic
        codes.insert("[24m", CursesStyle::Attribute(A_UNDERLINE, false));
        codes.insert("[27m", CursesStyle::Attribute(A_REVERSE, false));
        codes.insert("[29m", CursesStyle::Attribute(A_DIM, false)); // Strikethrough

        codes.insert("[30m", CursesStyle::Color(Some(COLOR_BLACK), None));
        codes.insert("[31m", CursesStyle::Color(Some(COLOR_RED), None));
        codes.insert("[32m", CursesStyle::Color(Some(COLOR_GREEN), None));
        codes.insert("[33m", CursesStyle::Color(Some(COLOR_YELLOW), None));
        codes.insert("[34m", CursesStyle::Color(Some(COLOR_BLUE), None));
        codes.insert("[35m", CursesStyle::Color(Some(COLOR_MAGENTA), None));
        codes.insert("[36m", CursesStyle::Color(Some(COLOR_CYAN), None));
        codes.insert("[37m", CursesStyle::Color(Some(COLOR_WHITE), None));
        codes.insert("[39m", CursesStyle::Color(Some(COLOR_DEFAULT), None));

        codes.insert("[40m", CursesStyle::Color(None, Some(COLOR_BLACK)));
        codes.insert("[41m", CursesStyle::Color(None, Some(COLOR_RED)));
        codes.insert("[42m", CursesStyle::Color(None, Some(COLOR_GREEN)));
        codes.insert("[43m", CursesStyle::Color(None, Some(COLOR_YELLOW)));
        codes.insert("[44m", CursesStyle::Color(None, Some(COLOR_BLUE)));
        codes.insert("[45m", CursesStyle::Color(None, Some(COLOR_MAGENTA)));
        codes.insert("[46m", CursesStyle::Color(None, Some(COLOR_CYAN)));
        codes.insert("[47m", CursesStyle::Color(None, Some(COLOR_WHITE)));
        codes.insert("[49m", CursesStyle::Color(None, Some(COLOR_DEFAULT)));

        codes
    };

    static ref ACTIVE_VALUES: Mutex<(Vec<fn() -> u64>, i16, i16)> = Mutex::new((vec![], COLOR_DEFAULT, COLOR_DEFAULT));
}


pub trait AnsiStr {
    fn has_ansi_escape_sequence<'a>(&'a self) -> bool;

    fn strip_ansi<'a>(&'a self) -> String;

    fn break_to_ncurses_components<'a>(&'a self) -> Vec<CursesComponent>;
}

impl AnsiStr for str {
    fn has_ansi_escape_sequence(&self) -> bool {
        self.contains("")
    }

    fn strip_ansi(&self) -> String {
        lazy_static! {
            static ref STRIP_ANSI_MATCHER: Regex = Regex::new(r"(\x1b\[\d+m)").unwrap();
        }
        STRIP_ANSI_MATCHER.replace_all(self, "")
    }

    fn break_to_ncurses_components(&self) -> Vec<CursesComponent> {
        let mut components = vec![];

        lazy_static! {
            static ref BREAK_ANSI_MATCHER: Regex = Regex::new(r"(\x1b\[\d+m)|([^\x1b]*)").unwrap();
        }

        for capture in BREAK_ANSI_MATCHER.captures_iter(self) {
            if capture.at(1).is_some() {
                match ANSI_TO_NCURSES_MAPPING.get(capture.at(1).unwrap()) {
                    Some(style) => components.push(CursesComponent::Style(style)),
                    _ => {}
                };
            }

            if capture.at(2).is_some() {
                let content = capture.at(2).unwrap().to_string();
                components.push(CursesComponent::Content(content));
            }
        }

        components
    }
}

pub trait AnsiLine {
    fn print<'a>(&'a self, window: WINDOW);
}

impl AnsiLine for Line {
    fn print(&self, window: WINDOW) {
        match self.components {
            Some(ref value) => {
                for component in value {
                    component.print(window);
                }
                waddch(window, '\n' as u64);
            },
            None => {
                wprintw(window, &format!("{}\n", self.content_without_ansi));
            }
        };
    }
}

#[derive(Debug)]
pub enum CursesStyle {
    Attribute(fn() -> u64, bool),
    Color(Option<i16>, Option<i16>),
    Reset
}

impl CursesStyle {
    pub fn print(&self, window: WINDOW) {
        let mut active_values = ACTIVE_VALUES.lock().unwrap();

        match self {
            &CursesStyle::Attribute(prop, active) => {
                if active {
                    active_values.0.push(prop);
                    wattron(window, prop() as i32);
                } else {
                    wattroff(window, prop() as i32);
                }
            },
            &CursesStyle::Color(foreground, background) => {
                let current_foreground = foreground.unwrap_or(active_values.1);
                let current_background = background.unwrap_or(active_values.2);

                let id = build_color_id(&current_foreground, &current_background);
                wattron(window, COLOR_PAIR(id) as i32);

                active_values.1 = current_foreground;
                active_values.2 = current_background;
            },
            &CursesStyle::Reset => {
                for prop in active_values.0.drain(..) {
                    wattroff(window, prop() as i32);
                }
                let id = build_color_id(&COLOR_DEFAULT, &COLOR_DEFAULT);
                wattron(window, COLOR_PAIR(id) as i32);
            }
        }
    }
}

#[derive(Debug)]
pub enum CursesComponent {
    Style(&'static CursesStyle),
    Content(String)
}

impl CursesComponent {
    pub fn print(&self, window: WINDOW) {
        match self {
            &CursesComponent::Style(value) => {
                value.print(window);
            },
            &CursesComponent::Content(ref value) => {
                wprintw(window, &value);
            }
        };
    }
}
