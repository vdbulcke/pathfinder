use ansi_term::{Colour::Fixed, Style};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use owo_colors::OwoColorize;

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

struct State {
    userspace_configuration: BTreeMap<String, String>,

    is_tab_vue_focussed: bool,
    focus_tab_pos: usize,
    result_index: usize,
    tab_infos: Vec<TabInfo>,
    pane_manifest: PaneManifest,
    input: String,
    input_cusror_index: usize,
    tab_match: Option<usize>,
    pane_match: Option<u32>,
    pane_title_match: String,
    fz_matcher: SkimMatcherV2,
}

impl Default for State {
    fn default() -> Self {
        Self {
            userspace_configuration: BTreeMap::default(),
            is_tab_vue_focussed: true,
            focus_tab_pos: 0,
            result_index: 0,
            tab_infos: Vec::default(),
            pane_manifest: PaneManifest::default(),
            input: String::default(),
            input_cusror_index: 0,
            tab_match: None,
            pane_match: None,
            pane_title_match: String::default(),
            fz_matcher: SkimMatcherV2::default(),
        }
    }
}

impl State {
    fn handle_key_event(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = true;
        match key.bare_key {
            BareKey::Enter => {
                if self.is_tab_vue_focussed {
                    if let Some(p) = self.tab_match {
                        close_focus();
                        switch_tab_to(p as u32 + 1);
                    }
                } else if let Some(pane_id) = self.pane_match {
                    close_focus();
                    focus_terminal_pane(pane_id, true);
                }
            }
            BareKey::Backspace => {
                if self.remove_input_at_index() {
                    if self.is_tab_vue_focussed {
                        self.fuzzy_find_tab();
                    } else {
                        self.fuzzy_find_pane();
                    }
                }
                should_render = true;
            }

            BareKey::Down => {
                if self.is_tab_vue_focussed {
                    self.move_down_tab();
                } else {
                    self.move_down_pane();
                }

                should_render = true;
            }
            BareKey::PageUp => {
                if self.is_tab_vue_focussed {
                    self.seek_tab(0);
                }
                should_render = true;
            }
            BareKey::Up => {
                if self.is_tab_vue_focussed {
                    self.move_up_tab();
                } else {
                    self.move_up_pane();
                }
                should_render = true;
            }
            BareKey::Left => {
                if self.input_cusror_index > 0 {
                    self.input_cusror_index -= 1;
                }
                should_render = true;
            }
            BareKey::Right => {
                if self.input_cusror_index < self.input.len() {
                    self.input_cusror_index += 1;
                }
                should_render = true;
            }

            BareKey::Esc => {
                self.close();
                should_render = true;
            }
            BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.close();
                should_render = true;
            }

            BareKey::Tab => {
                self.change_mode();
                should_render = true;
            }
            BareKey::Char(c) => {
                if self.insert_input_at_index(c) {
                    if self.is_tab_vue_focussed {
                        self.fuzzy_find_tab();
                    } else {
                        self.fuzzy_find_pane();
                    }
                }
                should_render = true;
            }
            _ => (),
        };

        should_render
    }

    /// close current plugins and its hepler pane
    /// get the focused tab position
    fn get_focused_tab(&mut self) {
        for (i, t) in self.tab_infos.iter().enumerate() {
            if t.active {
                self.focus_tab_pos = t.position;
                if self.tab_match.is_none() {
                    self.tab_match = Some(i);

                    if self.is_tab_vue_focussed {
                        self.result_index = i;
                    }
                }
            }
        }
    }

    fn close(&self) {
        close_plugin_pane(get_plugin_ids().plugin_id);
    }

    fn change_mode(&mut self) {
        // reset input
        self.input = String::default();
        self.input_cusror_index = 0;
        self.is_tab_vue_focussed = !self.is_tab_vue_focussed;

        if !self.is_tab_vue_focussed {
            // pane view
            self.pane_match = None;
            self.pane_title_match = String::default();
            self.result_index = 0;

            self.get_pane_at_index();
            return;
        }

        // tab view
        if let Some(i) = self.tab_match {
            self.result_index = i;
        }
    }

    fn fuzzy_find_tab(&mut self) {
        let mut best_score = 0;

        // reset match
        self.tab_match = None;
        self.result_index = 0;
        for (i, t) in self.tab_infos.iter().enumerate() {
            if let Some(score) = self.fz_matcher.fuzzy_match(t.name.as_str(), &self.input) {
                if score > best_score {
                    best_score = score;
                    self.tab_match = Some(i);
                    self.result_index = i;
                }
            }
        }

        // if no found default to focus tab
        if self.tab_match.is_none() {
            self.tab_match = Some(self.focus_tab_pos);
            self.result_index = self.focus_tab_pos;
        }
    }

    fn seek_tab(&mut self, idx: usize) {
        self.result_index = idx;
        for (i, t) in self.tab_infos.iter().enumerate() {
            if (self.input == String::default()
                || self
                    .fz_matcher
                    .fuzzy_match(t.name.as_str(), &self.input)
                    .is_some())
                && i == self.result_index
            {
                self.tab_match = Some(i);
                self.result_index = i;
                break;
            }
        }
    }

    fn move_down_tab(&mut self) {
        let mut first_match = None;
        let mut seek_result = false;
        let mut found_next = None;

        for (i, t) in self.tab_infos.iter().enumerate() {
            if self.input == String::default()
                || self
                    .fz_matcher
                    .fuzzy_match(t.name.as_str(), &self.input)
                    .is_some()
            {
                if first_match.is_none() {
                    first_match = Some(i);
                }

                if i == self.result_index {
                    seek_result = true;
                    continue;
                }

                if seek_result {
                    found_next = Some(i);
                    self.tab_match = Some(i);
                    self.result_index = i;
                    break;
                }
            }
        }

        if found_next.is_none() {
            if let Some(i) = first_match {
                self.tab_match = Some(i);
                self.result_index = i;
            }
        }
    }

    fn move_up_tab(&mut self) {
        let mut prev_match = None;
        let mut last_match = None;

        for (i, t) in self.tab_infos.iter().enumerate() {
            if self.input == String::default()
                || self
                    .fz_matcher
                    .fuzzy_match(t.name.as_str(), &self.input)
                    .is_some()
            {
                if i == self.result_index && prev_match.is_some() {
                    break;
                }
                prev_match = Some(i);
                last_match = Some(i);
            }
        }

        if let Some(i) = prev_match {
            self.tab_match = Some(i);
            self.result_index = i;
            return;
        }

        if let Some(i) = last_match {
            self.tab_match = Some(i);
            self.result_index = i;
        }
    }

    fn fuzzy_find_pane(&mut self) {
        let mut best_score = 0;

        // reset match
        self.pane_match = None;
        self.pane_title_match = String::default();
        if let Some(p) = self.tab_match {
            if let Some(panes) = self.pane_manifest.panes.get(&p) {
                for (i, pane) in panes.iter().enumerate() {
                    if pane.is_plugin {
                        continue;
                    }
                    if let Some(score) = self
                        .fz_matcher
                        .fuzzy_match(pane.title.as_str(), &self.input)
                    {
                        if score > best_score {
                            best_score = score;
                            self.pane_match = Some(pane.id);
                            self.pane_title_match = pane.title.to_owned();
                            self.result_index = i;
                        }
                    }
                }
            }
        }
    }

    fn get_pane_at_index(&mut self) {
        if let Some(p) = self.tab_match {
            if let Some(panes) = self.pane_manifest.panes.get(&p) {
                for (i, pane) in panes.iter().enumerate() {
                    if pane.is_plugin {
                        continue;
                    }
                    if (self.input == String::default()
                        || self
                            .fz_matcher
                            .fuzzy_match(pane.title.as_str(), &self.input)
                            .is_some())
                        && i == self.result_index
                    {
                        self.pane_match = Some(pane.id);
                        self.pane_title_match = pane.title.to_owned();
                        self.result_index = i;
                        break;
                    }
                }
            }
        }
    }

    fn move_down_pane(&mut self) {
        let mut first_match = None;
        let mut seek_result = false;
        let mut found_next = None;

        self.pane_match = None;
        self.pane_title_match = String::default();
        if let Some(p) = self.tab_match {
            if let Some(panes) = self.pane_manifest.panes.get(&p) {
                for (i, pane) in panes.iter().enumerate() {
                    if pane.is_plugin {
                        continue;
                    }
                    if self.input == String::default()
                        || self
                            .fz_matcher
                            .fuzzy_match(pane.title.as_str(), &self.input)
                            .is_some()
                    {
                        if first_match.is_none() {
                            first_match = Some(i);
                        }

                        if i == self.result_index {
                            seek_result = true;
                            continue;
                        }

                        if seek_result {
                            self.pane_match = Some(pane.id);
                            self.pane_title_match = pane.title.to_owned();
                            found_next = Some(i);
                            self.result_index = i;
                            break;
                        }
                    }
                }

                if found_next.is_none() {
                    if let Some(i) = first_match {
                        if let Some(pane) = panes.get(i) {
                            self.pane_match = Some(pane.id);
                            self.pane_title_match = pane.title.to_owned();
                            self.result_index = i;
                        }
                    }
                }
            }
        }
    }

    fn move_up_pane(&mut self) {
        let mut prev_match = None;
        let mut last_match = None;

        self.pane_match = None;
        self.pane_title_match = String::default();
        if let Some(p) = self.tab_match {
            if let Some(panes) = self.pane_manifest.panes.get(&p) {
                for (i, pane) in panes.iter().enumerate() {
                    if pane.is_plugin {
                        continue;
                    }
                    if self.input == String::default()
                        || self
                            .fz_matcher
                            .fuzzy_match(pane.title.as_str(), &self.input)
                            .is_some()
                    {
                        if i == self.result_index && prev_match.is_some() {
                            break;
                        }
                        prev_match = Some(i);
                        last_match = Some(i);
                    }
                }

                if let Some(i) = prev_match {
                    if let Some(pane) = panes.get(i) {
                        self.pane_match = Some(pane.id);
                        self.pane_title_match = pane.title.to_owned();
                        self.result_index = i;
                    }
                    return;
                }

                if let Some(i) = last_match {
                    if let Some(pane) = panes.get(i) {
                        self.pane_match = Some(pane.id);
                        self.pane_title_match = pane.title.to_owned();
                        self.result_index = i;
                    }
                }
            }
        }
    }

    /// remove_input_at_index  removes char at the
    /// cursor index and update input.
    /// Returns true if the input has change
    fn remove_input_at_index(&mut self) -> bool {
        if self.input.is_empty() {
            self.input.pop();
        } else if self.input_cusror_index > 0 && self.input_cusror_index <= self.input.len() {
            self.input.remove(self.input_cusror_index - 1);
            // update cursor index
            self.input_cusror_index -= 1;

            return true;
        } else if self.input_cusror_index == 0 {
            self.input.remove(0);
        }
        false
    }

    /// remove_input_at_index  removes char at the
    /// cursor index and update input.
    /// Returns true if the input has change
    fn insert_input_at_index(&mut self, c: char) -> bool {
        if self.input.is_empty() {
            self.input.push(c);

            // update cursor index
            self.input_cusror_index += 1;
            return true;
        } else if self.input_cusror_index > 0 && self.input_cusror_index <= self.input.len() {
            self.input.insert(self.input_cusror_index, c);
            // update cursor index
            self.input_cusror_index += 1;

            return true;
        } else if self.input_cusror_index == 0 {
            self.input.insert(0, c);
            self.input_cusror_index += 1;
        }
        false
    }

    /// print the input prompt
    fn print_prompt(&self, _rows: usize, _cols: usize) {
        // if not enough space in UI
        // input prompt
        let prompt = " > ".cyan().bold().to_string();
        if self.input.is_empty() {
            println!(
                "{} {}{}",
                prompt,
                "┃".bold().white(),
                "search pattern".dimmed().italic(),
            );
        } else {
            self.print_non_empty_input_prompt(prompt);
        }
    }

    fn print_non_empty_input_prompt(&self, prompt: String) {
        match self.input_cusror_index.cmp(&self.input.len()) {
            std::cmp::Ordering::Equal => {
                println!("{} {}{}", prompt, self.input.dimmed(), "┃".bold().white(),);
            }
            std::cmp::Ordering::Less => {
                let copy = self.input.clone();
                let (before_curs, after_curs) = copy.split_at(self.input_cusror_index);

                println!(
                    "{} {}{}{}",
                    prompt,
                    before_curs.dimmed(),
                    "┃".bold().white(),
                    after_curs.dimmed()
                );
            }

            std::cmp::Ordering::Greater => (),
        }
    }
}

register_plugin!(State);
impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.userspace_configuration = configuration;

        // Permission
        // - ReadApplicationState => for Tab and Pane update
        // - ChangeApplicationState => rename plugin pane, close managed paned
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);
        subscribe(&[
            EventType::ModeUpdate,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::Key,
        ]);

        rename_plugin_pane(get_plugin_ids().plugin_id, "PathFinder");
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = true;
        match event {
            Event::TabUpdate(tab_info) => {
                self.tab_infos = tab_info;
                self.get_focused_tab();
                should_render = true;
            }
            Event::PaneUpdate(pane_manifest) => {
                self.pane_manifest = pane_manifest;
                should_render = true;
            }
            Event::Key(key) => {
                should_render = self.handle_key_event(key);
            }
            _ => (),
        };

        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        // get the shell args from config

        let debug = self.userspace_configuration.get("debug");
        // count keep tracks of lines printed
        // 4 lines for CWD and keybinding views
        let mut count = 4;

        if self.is_tab_vue_focussed {
            print_ribbon_with_coordinates(Text::new("Tabs Selector").selected(), 1, 0, None, None);
            print_ribbon_with_coordinates(Text::new("Panes Selector"), 18, 0, None, None);
            println!();
            println!();
        } else {
            print_ribbon_with_coordinates(Text::new("Tabs Selector"), 1, 0, None, None);
            print_ribbon_with_coordinates(
                Text::new("Panes Selector").selected(),
                18,
                0,
                None,
                None,
            );
            println!();
            println!();
        }

        count += 1;

        self.print_prompt(rows, cols);
        count += 1;

        if self.is_tab_vue_focussed {
            println!("Tabs: ");

            count += 1;

            for (i, t) in self.tab_infos.iter().enumerate() {
                if self
                    .fz_matcher
                    .fuzzy_match(t.name.as_str(), &self.input)
                    .is_some()
                {
                    // limits display of completion
                    // based on available rows in pane
                    // with arbitrary buffer for safety
                    if count >= rows - 4 {
                        println!(" - {}", "...".dimmed());
                        break;
                    }

                    if i == self.result_index {
                        println!(" - {}", t.name.blue().bold());
                    } else {
                        println!(" - {}", t.name.dimmed());
                    }

                    count += 1;
                }
            }
            println!();
        } else {
            println!("Panes: ");
            if let Some(p) = self.tab_match {
                if let Some(panes) = self.pane_manifest.panes.get(&p) {
                    for (i, pane) in panes.iter().enumerate() {
                        if !pane.is_plugin
                            && self
                                .fz_matcher
                                .fuzzy_match(pane.title.as_str(), &self.input)
                                .is_some()
                        {
                            // limits display of completion
                            // based on available rows in pane
                            // with arbitrary buffer for safety
                            if count >= rows - 4 {
                                println!(" - {}", "...".dimmed());
                                break;
                            }
                            if i == self.result_index {
                                println!(" - {}", pane.title.blue().bold());
                            } else {
                                println!(" - {}", pane.title.dimmed());
                            }
                            count += 1;
                        }
                    }
                }
            }

            println!();
            if !self.pane_title_match.is_empty() {
                println!(
                    "{} {}",
                    color_bold(WHITE, "Selected Pane ->"),
                    self.pane_title_match.as_str().blue().bold()
                );
            } else {
                println!(
                    "{} {}",
                    color_bold(WHITE, "Selected Pane ->"),
                    "No matches found".dimmed()
                );
            }
        }

        if let Some(m) = self.tab_match {
            if let Some(t) = self.tab_infos.get(m) {
                println!(
                    "{} {}",
                    color_bold(WHITE, "Selected Tab ->"),
                    t.name.as_str().blue().bold()
                );
            }
        } else {
            println!(
                "{} {}",
                color_bold(WHITE, "Selected Tab ->"),
                "No matches found".dimmed()
            );
        }

        // Key binding view

        if debug.is_some_and(|x| x == "true") {
            println!("input: {}", self.input);

            println!("Cursor: {}", self.input_cusror_index);
            println!("len: {}", self.input.len());

            println!("tab match: {}", self.tab_match.unwrap_or(42));
            println!("pane match: {}", self.pane_match.unwrap_or(42));
            println!("focussed tab : {}", self.focus_tab_pos);
            println!("is tab vue: {}", self.is_tab_vue_focussed);
            println!("result_index: {}", self.result_index);

            println!(
                "{} {:#?}",
                color_bold(GREEN, "Runtime configuration:"),
                self.userspace_configuration
            );
        }
    }
}

pub const CYAN: u8 = 51;
pub const GRAY_LIGHT: u8 = 238;
pub const GRAY_DARK: u8 = 245;
pub const WHITE: u8 = 15;
pub const BLACK: u8 = 16;
pub const RED: u8 = 124;
pub const GREEN: u8 = 154;
pub const ORANGE: u8 = 166;

fn color_bold(color: u8, text: &str) -> String {
    format!("{}", Style::new().fg(Fixed(color)).bold().paint(text))
}
