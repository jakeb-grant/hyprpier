use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
    Terminal,
};
use std::io;
use std::time::Duration;

use crate::profile::Profile;

use super::monitor_arrange::MonitorArrangeState;
use super::profile_editor::ProfileEditorState;
use super::profile_list::ProfileListState;
use super::thunderbolt::ThunderboltState;

// UI constants
const EVENT_POLL_MS: u64 = 100;
const REFRESH_INTERVAL_MS: u64 = 2000;
const DIALOG_WIDTH: u16 = 55;
const DIALOG_HEIGHT: u16 = 8;

/// Actions that can be triggered by key handlers
enum Action {
    None,
    Quit,
    NewScreen(Box<Screen>),
    /// Apply monitor arrangement changes and return to editor
    ArrangeApply,
    /// Cancel monitor arrangement and return to editor
    ArrangeCancel,
    /// Pause TUI, run sudo command, resume (args for hyprpier subcommand)
    RunSudo(Vec<String>),
}

/// The different screens/views in the TUI
pub enum Screen {
    ProfileList(ProfileListState),
    ProfileEditor(ProfileEditorState),
    MonitorArrange(MonitorArrangeState),
    Thunderbolt(ThunderboltState),
    Confirm(ConfirmDialog),
}

/// Generic confirmation dialog
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub style: ConfirmStyle,
    pub action: ConfirmAction,
}

/// Visual style for the confirmation dialog
#[derive(Clone, Copy)]
pub enum ConfirmStyle {
    Danger,  // Red border (delete, forget)
    Warning, // Yellow border (overwrite, unlink, etc.)
}

/// What to do when the user confirms
pub enum ConfirmAction {
    DeleteProfile {
        name: String,
    },
    OverwriteProfile {
        editor_state: ProfileEditorState,
    },
    UnlinkDock {
        uuid: String,
        tb_state: ThunderboltState,
    },
    SetUndocked {
        profile_name: String,
        dock_uuid: String,
    },
    LinkRemoveUndocked {
        editor_state: ProfileEditorState,
        dock_uuid: String,
    },
    LinkSteal {
        editor_state: ProfileEditorState,
        dock_uuid: String,
    },
}

/// Main application state
pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        Ok(Self {
            screen: Screen::ProfileList(ProfileListState::new()?),
            should_quit: false,
        })
    }

    /// Run the TUI application
    pub fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Main loop
        let result = self.main_loop(&mut terminal);

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    fn main_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        let mut last_refresh = std::time::Instant::now();

        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;

            // Auto-refresh every REFRESH_INTERVAL_MS
            if last_refresh.elapsed().as_millis() >= REFRESH_INTERVAL_MS as u128 {
                self.tick_refresh();
                last_refresh = std::time::Instant::now();
            }

            if let Some(action) = self.poll_events()? {
                match action {
                    Action::None => {}
                    Action::Quit => self.should_quit = true,
                    Action::NewScreen(screen) => self.screen = *screen,
                    Action::ArrangeApply => {
                        let placeholder = Screen::ProfileEditor(ProfileEditorState::new());
                        let screen = std::mem::replace(&mut self.screen, placeholder);
                        if let Screen::MonitorArrange(state) = screen {
                            self.screen = Screen::ProfileEditor(state.apply_to_editor());
                        }
                    }
                    Action::ArrangeCancel => {
                        let placeholder = Screen::ProfileEditor(ProfileEditorState::new());
                        let screen = std::mem::replace(&mut self.screen, placeholder);
                        if let Screen::MonitorArrange(state) = screen {
                            self.screen = Screen::ProfileEditor(state.cancel());
                        }
                    }
                    Action::RunSudo(args) => {
                        self.run_sudo_command(terminal, &args)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Pause TUI, run sudo hyprpier <args>, resume
    fn run_sudo_command(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        args: &[String],
    ) -> Result<()> {
        // Leave alternate screen and restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;

        // Get path to current executable
        let exe = std::env::current_exe()?;

        // Run sudo hyprpier <args>
        println!();
        let _ = std::process::Command::new("sudo")
            .arg(&exe)
            .args(args)
            .status();

        // Re-enter TUI mode
        enable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture
        )?;

        // Force full redraw
        terminal.clear()?;

        // Refresh the current screen state
        self.refresh_screen()?;

        Ok(())
    }

    /// Refresh the current screen's state after returning from sudo
    fn refresh_screen(&mut self) -> Result<()> {
        match &self.screen {
            Screen::Thunderbolt(_) => {
                self.screen = Screen::Thunderbolt(ThunderboltState::new()?);
            }
            Screen::Confirm(_) => {
                // After sudo from a confirm dialog, go back to Thunderbolt
                self.screen = Screen::Thunderbolt(ThunderboltState::new()?);
            }
            _ => {}
        }
        Ok(())
    }

    /// Periodic refresh for screens that need it (profile list, thunderbolt)
    fn tick_refresh(&mut self) {
        match &mut self.screen {
            Screen::ProfileList(state) => {
                state.refresh();
            }
            Screen::Thunderbolt(state) => {
                state.refresh();
            }
            // Other screens don't need periodic refresh
            _ => {}
        }
    }

    fn render(&mut self, frame: &mut ratatui::Frame) {
        match &mut self.screen {
            Screen::ProfileList(state) => super::profile_list::render(frame, state),
            Screen::ProfileEditor(state) => super::profile_editor::render(frame, state),
            Screen::MonitorArrange(state) => super::monitor_arrange::render(frame, state),
            Screen::Thunderbolt(state) => super::thunderbolt::render(frame, state),
            Screen::Confirm(dialog) => render_confirm_dialog(frame, dialog),
        }
    }

    fn poll_events(&mut self) -> Result<Option<Action>> {
        if event::poll(Duration::from_millis(EVENT_POLL_MS))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    return Ok(None);
                }

                let action = match &mut self.screen {
                    Screen::ProfileList(state) => handle_profile_list_keys(key.code, state)?,
                    Screen::ProfileEditor(state) => handle_profile_editor_keys(key.code, state)?,
                    Screen::MonitorArrange(state) => handle_monitor_arrange_keys(key.code, state)?,
                    Screen::Thunderbolt(state) => handle_thunderbolt_keys(key.code, state)?,
                    Screen::Confirm(dialog) => handle_confirm_keys(key.code, dialog)?,
                };

                return Ok(Some(action));
            }
        }
        Ok(None)
    }
}

/// Render the unified confirmation dialog
fn render_confirm_dialog(frame: &mut ratatui::Frame, dialog: &ConfirmDialog) {
    let area = frame.area();

    let x = (area.width.saturating_sub(DIALOG_WIDTH)) / 2;
    let y = (area.height.saturating_sub(DIALOG_HEIGHT)) / 2;
    let dialog_area = Rect::new(x, y, DIALOG_WIDTH, DIALOG_HEIGHT);

    frame.render_widget(Clear, dialog_area);

    let border_color = match dialog.style {
        ConfirmStyle::Danger => Color::Red,
        ConfirmStyle::Warning => Color::Yellow,
    };

    let block = Block::default()
        .title(format!(" {} ", dialog.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let text = format!("{}\n\n[y] Yes  [n] No", dialog.message);
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(paragraph, dialog_area);
}

/// Handle keys for the unified confirmation dialog
fn handle_confirm_keys(key: KeyCode, dialog: &mut ConfirmDialog) -> Result<Action> {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => execute_confirm_action(&dialog.action),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            cancel_confirm_action(&dialog.action)
        }
        _ => Ok(Action::None),
    }
}

/// Execute the confirmed action
fn execute_confirm_action(action: &ConfirmAction) -> Result<Action> {
    match action {
        ConfirmAction::DeleteProfile { name } => {
            Profile::delete(name)?;
            Ok(Action::NewScreen(Box::new(Screen::ProfileList(
                ProfileListState::new()?,
            ))))
        }
        ConfirmAction::OverwriteProfile { editor_state } => {
            let mut state = editor_state.clone();
            // Validate name (should already be valid, but double-check)
            if let Err(e) = crate::profile::validate_profile_name(&state.name_input) {
                state.error_message = Some(e.to_string());
                return Ok(Action::NewScreen(Box::new(Screen::ProfileEditor(state))));
            }
            state.sync_inputs_to_profile();
            state.profile.save()?;
            Ok(Action::NewScreen(Box::new(Screen::ProfileList(
                ProfileListState::new()?,
            ))))
        }
        ConfirmAction::UnlinkDock { uuid, .. } => {
            let mut metadata = crate::metadata::Metadata::load()?;
            metadata.unlink_dock(uuid);
            metadata.save()?;
            Ok(Action::NewScreen(Box::new(Screen::Thunderbolt(
                ThunderboltState::new()?,
            ))))
        }
        ConfirmAction::SetUndocked {
            profile_name,
            dock_uuid,
        } => {
            let mut metadata = crate::metadata::Metadata::load()?;
            metadata.unlink_dock(dock_uuid);
            metadata.undocked_profile = Some(profile_name.clone());
            metadata.save()?;
            Ok(Action::NewScreen(Box::new(Screen::ProfileList(
                ProfileListState::new()?,
            ))))
        }
        ConfirmAction::LinkRemoveUndocked {
            editor_state,
            dock_uuid,
        } => {
            let mut metadata = crate::metadata::Metadata::load()?;
            metadata.undocked_profile = None;
            metadata.link_dock(dock_uuid, &editor_state.name_input);
            metadata.save()?;
            let mut state = editor_state.clone();
            state.refresh_dock_status();
            Ok(Action::NewScreen(Box::new(Screen::ProfileEditor(state))))
        }
        ConfirmAction::LinkSteal {
            editor_state,
            dock_uuid,
        } => {
            let mut metadata = crate::metadata::Metadata::load()?;
            metadata.link_dock(dock_uuid, &editor_state.name_input);
            metadata.save()?;
            let mut state = editor_state.clone();
            state.refresh_dock_status();
            Ok(Action::NewScreen(Box::new(Screen::ProfileEditor(state))))
        }
    }
}

/// Cancel and return to the appropriate screen
fn cancel_confirm_action(action: &ConfirmAction) -> Result<Action> {
    match action {
        ConfirmAction::DeleteProfile { .. } => Ok(Action::NewScreen(Box::new(
            Screen::ProfileList(ProfileListState::new()?),
        ))),
        ConfirmAction::OverwriteProfile { editor_state } => Ok(Action::NewScreen(Box::new(
            Screen::ProfileEditor(editor_state.clone()),
        ))),
        ConfirmAction::UnlinkDock { tb_state, .. } => Ok(Action::NewScreen(Box::new(
            Screen::Thunderbolt(tb_state.clone()),
        ))),
        ConfirmAction::SetUndocked { .. } => Ok(Action::NewScreen(Box::new(Screen::ProfileList(
            ProfileListState::new()?,
        )))),
        ConfirmAction::LinkRemoveUndocked { editor_state, .. }
        | ConfirmAction::LinkSteal { editor_state, .. } => Ok(Action::NewScreen(Box::new(
            Screen::ProfileEditor(editor_state.clone()),
        ))),
    }
}

fn handle_profile_list_keys(key: KeyCode, state: &mut ProfileListState) -> Result<Action> {
    match key {
        KeyCode::Char('q') | KeyCode::Esc => Ok(Action::Quit),
        KeyCode::Char('n') => Ok(Action::NewScreen(Box::new(Screen::ProfileEditor(
            ProfileEditorState::new(),
        )))),
        KeyCode::Char('e') | KeyCode::Enter => {
            if let Some(name) = state.selected_profile() {
                if let Ok(profile) = Profile::load(&name) {
                    return Ok(Action::NewScreen(Box::new(Screen::ProfileEditor(
                        ProfileEditorState::from_profile(profile),
                    ))));
                }
            }
            Ok(Action::None)
        }
        KeyCode::Char('d') => {
            if let Some(name) = state.selected_profile() {
                return Ok(Action::NewScreen(Box::new(Screen::Confirm(
                    ConfirmDialog {
                        title: "Confirm Delete".to_string(),
                        message: format!("Delete profile '{}'?", name),
                        style: ConfirmStyle::Danger,
                        action: ConfirmAction::DeleteProfile { name },
                    },
                ))));
            }
            Ok(Action::None)
        }
        KeyCode::Char('a') => {
            if let Some(name) = state.selected_profile() {
                crate::apply::apply_profile_quiet(&name, false)?;
                *state = ProfileListState::new()?;
            }
            Ok(Action::None)
        }
        KeyCode::Char('u') => {
            // Set selected profile as undocked fallback
            if let Some(name) = state.selected_profile() {
                let metadata = crate::metadata::Metadata::load()?;

                // Toggle: if already undocked, clear it
                if metadata.undocked_profile.as_ref() == Some(&name) {
                    let mut metadata = metadata;
                    metadata.undocked_profile = None;
                    metadata.save()?;
                    *state = ProfileListState::new()?;
                } else {
                    // Check if profile is linked to a dock
                    if let Some(dock_uuid) = metadata.get_profile_dock(&name).cloned() {
                        // Get dock name for display
                        let dock_name = crate::dock::detect_docks()
                            .ok()
                            .and_then(|docks| docks.into_iter().find(|d| d.uuid == dock_uuid))
                            .map(|d| d.name)
                            .unwrap_or_else(|| {
                                format!("{}...", &dock_uuid[..8.min(dock_uuid.len())])
                            });

                        return Ok(Action::NewScreen(Box::new(Screen::Confirm(ConfirmDialog {
                            title: "Remove Dock Link?".to_string(),
                            message: format!(
                                "Profile '{}' is linked to dock '{}'.\nUnlink and set as undocked fallback?",
                                name, dock_name
                            ),
                            style: ConfirmStyle::Warning,
                            action: ConfirmAction::SetUndocked {
                                profile_name: name,
                                dock_uuid,
                            },
                        }))));
                    }

                    // Not linked to dock, just set as undocked
                    let mut metadata = metadata;
                    metadata.undocked_profile = Some(name);
                    metadata.save()?;
                    *state = ProfileListState::new()?;
                }
            }
            Ok(Action::None)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.previous();
            Ok(Action::None)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.next();
            Ok(Action::None)
        }
        KeyCode::Char('t') => {
            // Open Thunderbolt manager
            Ok(Action::NewScreen(Box::new(Screen::Thunderbolt(
                ThunderboltState::new()?,
            ))))
        }
        _ => Ok(Action::None),
    }
}

fn handle_thunderbolt_keys(key: KeyCode, state: &mut ThunderboltState) -> Result<Action> {
    use super::thunderbolt::Section;

    match key {
        KeyCode::Esc | KeyCode::Char('q') => Ok(Action::NewScreen(Box::new(Screen::ProfileList(
            ProfileListState::new()?,
        )))),
        KeyCode::Tab => {
            state.switch_section();
            Ok(Action::None)
        }
        KeyCode::Char('x') => {
            // Unlink dock from profile
            match state.section {
                Section::Connected => {
                    if let Some(info) = state.selected_device() {
                        if let Some(profile) = &info.linked_profile {
                            return Ok(Action::NewScreen(Box::new(Screen::Confirm(
                                ConfirmDialog {
                                    title: "Confirm Unlink".to_string(),
                                    message: format!("Unlink dock from profile '{}'?", profile),
                                    style: ConfirmStyle::Warning,
                                    action: ConfirmAction::UnlinkDock {
                                        uuid: info.device.uuid.clone(),
                                        tb_state: state.clone(),
                                    },
                                },
                            ))));
                        }
                    }
                }
                Section::Disconnected => {
                    if let Some(dock) = state.selected_disconnected() {
                        return Ok(Action::NewScreen(Box::new(Screen::Confirm(
                            ConfirmDialog {
                                title: "Confirm Unlink".to_string(),
                                message: format!("Unlink dock from profile '{}'?", dock.profile),
                                style: ConfirmStyle::Warning,
                                action: ConfirmAction::UnlinkDock {
                                    uuid: dock.uuid.clone(),
                                    tb_state: state.clone(),
                                },
                            },
                        ))));
                    }
                }
            }
            Ok(Action::None)
        }
        KeyCode::Char('s') => {
            // Setup or disable auto-switching
            if state.auto_switch_enabled {
                Ok(Action::RunSudo(vec!["setup".to_string(), "--uninstall".to_string()]))
            } else {
                Ok(Action::RunSudo(vec!["setup".to_string()]))
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.previous();
            Ok(Action::None)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.next();
            Ok(Action::None)
        }
        _ => Ok(Action::None),
    }
}

fn handle_profile_editor_keys(key: KeyCode, state: &mut ProfileEditorState) -> Result<Action> {
    // Handle input mode (typing in text fields)
    if state.input_mode {
        match key {
            KeyCode::Esc | KeyCode::Enter => {
                state.input_mode = false;
            }
            KeyCode::Backspace => {
                state.current_input_mut().pop();
                state.error_message = None; // Clear error on edit
            }
            KeyCode::Char(c) => {
                state.current_input_mut().push(c);
                state.error_message = None; // Clear error on edit
            }
            _ => {}
        }
        return Ok(Action::None);
    }

    // Not in input mode
    match key {
        KeyCode::Esc | KeyCode::Char('q') => Ok(Action::NewScreen(Box::new(Screen::ProfileList(
            ProfileListState::new()?,
        )))),
        KeyCode::Tab => {
            state.next_field();
            Ok(Action::None)
        }
        KeyCode::BackTab => {
            state.previous_field();
            Ok(Action::None)
        }
        KeyCode::Enter => {
            // Enter input mode on text fields
            if state.focused_field < 2 {
                state.input_mode = true;
            }
            Ok(Action::None)
        }
        KeyCode::Char('d') => {
            state.detect_monitors()?;
            Ok(Action::None)
        }
        KeyCode::Char('a') => Ok(Action::NewScreen(Box::new(Screen::MonitorArrange(
            MonitorArrangeState::new(state.clone()),
        )))),
        KeyCode::Char('s') => {
            let new_name = &state.name_input;

            // Validate profile name first
            if let Err(e) = crate::profile::validate_profile_name(new_name) {
                state.error_message = Some(e.to_string());
                return Ok(Action::None);
            }

            let is_rename = state.original_name.as_ref() != Some(new_name);
            let profile_exists = crate::config::profile_path(new_name)
                .map(|p| p.exists())
                .unwrap_or(false);

            // Show confirmation if overwriting a different profile
            if is_rename && profile_exists {
                Ok(Action::NewScreen(Box::new(Screen::Confirm(
                    ConfirmDialog {
                        title: "Confirm Overwrite".to_string(),
                        message: format!("Profile '{}' already exists.\nOverwrite?", new_name),
                        style: ConfirmStyle::Warning,
                        action: ConfirmAction::OverwriteProfile {
                            editor_state: state.clone(),
                        },
                    },
                ))))
            } else {
                state.sync_inputs_to_profile();
                state.profile.save()?;
                Ok(Action::NewScreen(Box::new(Screen::ProfileList(
                    ProfileListState::new()?,
                ))))
            }
        }
        KeyCode::Char('l') => {
            let metadata = crate::metadata::Metadata::load()?;
            let profile_name = &state.name_input;

            // Check if already linked to a dock
            if let Some(uuid) = metadata.get_profile_dock(profile_name).cloned() {
                // Unlink - no confirmation needed
                let mut metadata = metadata;
                metadata.unlink_dock(&uuid);
                metadata.save()?;
                state.refresh_dock_status();
                return Ok(Action::None);
            }

            // Not linked - try to link to first available dock
            let docks = crate::dock::detect_docks()?;
            if let Some(dock) = docks.first() {
                let dock_uuid = dock.uuid.clone();
                let dock_name = dock.name.clone();

                // Check if this profile is the undocked fallback
                if metadata.undocked_profile.as_ref() == Some(profile_name) {
                    return Ok(Action::NewScreen(Box::new(Screen::Confirm(ConfirmDialog {
                        title: "Remove Undocked Status?".to_string(),
                        message: format!(
                            "Profile '{}' is the undocked fallback.\nLink to '{}' and remove undocked status?",
                            profile_name, dock_name
                        ),
                        style: ConfirmStyle::Warning,
                        action: ConfirmAction::LinkRemoveUndocked {
                            editor_state: state.clone(),
                            dock_uuid,
                        },
                    }))));
                }

                // Check if dock is already linked to another profile
                if let Some(old_profile) = metadata.get_dock_profile(&dock_uuid) {
                    if old_profile != profile_name {
                        return Ok(Action::NewScreen(Box::new(Screen::Confirm(
                            ConfirmDialog {
                                title: "Reassign Dock?".to_string(),
                                message: format!(
                                    "Dock '{}' is linked to '{}'.\nReassign to '{}'?",
                                    dock_name, old_profile, profile_name
                                ),
                                style: ConfirmStyle::Warning,
                                action: ConfirmAction::LinkSteal {
                                    editor_state: state.clone(),
                                    dock_uuid,
                                },
                            },
                        ))));
                    }
                }

                // No conflicts, just link
                let mut metadata = metadata;
                metadata.link_dock(&dock_uuid, profile_name);
                metadata.save()?;
                state.refresh_dock_status();
            }
            Ok(Action::None)
        }
        _ => Ok(Action::None),
    }
}

fn handle_monitor_arrange_keys(key: KeyCode, state: &mut MonitorArrangeState) -> Result<Action> {
    match key {
        KeyCode::Esc => Ok(Action::ArrangeCancel),
        KeyCode::Char('s') => Ok(Action::ArrangeApply),
        KeyCode::Up | KeyCode::Char('k') => {
            state.previous();
            Ok(Action::None)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.next();
            Ok(Action::None)
        }
        KeyCode::Left | KeyCode::Char('h') => {
            state.move_left();
            Ok(Action::None)
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.move_right();
            Ok(Action::None)
        }
        KeyCode::Char('d') => {
            state.toggle_disable();
            Ok(Action::None)
        }
        KeyCode::Char('x') | KeyCode::Delete => {
            state.remove_selected();
            Ok(Action::None)
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            let ws = if c == '0' {
                10
            } else if let Some(d) = c.to_digit(10) {
                d as u8
            } else {
                return Ok(Action::None);
            };
            state.toggle_workspace(ws);
            Ok(Action::None)
        }
        _ => Ok(Action::None),
    }
}
