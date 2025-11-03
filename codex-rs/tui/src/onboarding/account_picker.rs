use codex_core::AuthManager;
use codex_core::auth::AccountKind;
use codex_core::auth::AccountSummary;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use std::sync::Arc;
use std::sync::RwLock;

use crate::onboarding::auth::SignInState;
use crate::onboarding::onboarding_screen::KeyboardHandler;
use crate::onboarding::onboarding_screen::StepState;
use crate::onboarding::onboarding_screen::StepStateProvider;
use crate::tui::FrameRequester;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccountPickerSelection {
    Existing(AccountKind),
    AddNew,
}

pub(crate) struct AccountPickerWidget {
    pub request_frame: FrameRequester,
    pub auth_manager: Arc<AuthManager>,
    pub show_login_form: Arc<RwLock<bool>>,
    pub sign_in_state: Arc<RwLock<SignInState>>,
    accounts: Vec<AccountSummary>,
    highlighted: usize,
    selection: Option<AccountPickerSelection>,
    pub error: Option<String>,
}

impl AccountPickerWidget {
    pub fn new(
        request_frame: FrameRequester,
        auth_manager: Arc<AuthManager>,
        show_login_form: Arc<RwLock<bool>>,
        sign_in_state: Arc<RwLock<SignInState>>,
    ) -> Self {
        let (accounts, error) = match auth_manager.list_accounts() {
            Ok(accounts) => (accounts, None),
            Err(err) => (Vec::new(), Some(err.to_string())),
        };
        let highlighted = accounts.iter().position(|acc| acc.is_active).unwrap_or(0);

        if accounts.is_empty()
            && let Ok(mut flag) = show_login_form.write() {
                *flag = true;
            }

        Self {
            request_frame,
            auth_manager,
            show_login_form,
            sign_in_state,
            accounts,
            highlighted,
            selection: None,
            error,
        }
    }

    fn total_entries(&self) -> usize {
        self.accounts.len().saturating_add(1)
    }

    fn current_highlight(&self) -> usize {
        self.highlighted.min(self.total_entries().saturating_sub(1))
    }

    fn highlight_next(&mut self) {
        let total = self.total_entries();
        if total == 0 {
            return;
        }
        self.highlighted = (self.current_highlight() + 1) % total;
    }

    fn highlight_prev(&mut self) {
        let total = self.total_entries();
        if total == 0 {
            return;
        }
        let current = self.current_highlight();
        self.highlighted = if current == 0 {
            total.saturating_sub(1)
        } else {
            current - 1
        };
    }

    fn select_current(&mut self) {
        if self.current_highlight() < self.accounts.len() {
            let account = self.accounts[self.current_highlight()].clone();
            match self.auth_manager.select_account(&account.id) {
                Ok(()) => {
                    self.error = None;
                    self.selection = Some(AccountPickerSelection::Existing(account.kind));
                    if let Ok(mut guard) = self.show_login_form.write() {
                        *guard = false;
                    }
                    if let Ok(mut state) = self.sign_in_state.write() {
                        *state = match account.kind {
                            AccountKind::ChatGpt => SignInState::ChatGptSuccess,
                            AccountKind::ApiKey => SignInState::ApiKeyConfigured,
                        };
                    }
                    match self.auth_manager.list_accounts() {
                        Ok(updated) => {
                            self.accounts = updated;
                            self.highlighted = self
                                .accounts
                                .iter()
                                .position(|acc| acc.is_active)
                                .unwrap_or(self.current_highlight());
                        }
                        Err(err) => {
                            self.error = Some(err.to_string());
                        }
                    }
                }
                Err(err) => {
                    self.error = Some(err.to_string());
                }
            }
        } else {
            self.error = None;
            self.selection = Some(AccountPickerSelection::AddNew);
            if let Ok(mut guard) = self.show_login_form.write() {
                *guard = true;
            }
            if let Ok(mut state) = self.sign_in_state.write() {
                *state = SignInState::PickMode;
            }
            self.highlighted = self.accounts.len();
        }
        self.request_frame.schedule_frame();
    }

    fn render_entry(&self, index: usize) -> Line<'static> {
        if index < self.accounts.len() {
            let account = &self.accounts[index];
            let indicator = if self.current_highlight() == index {
                ">"
            } else {
                " "
            };
            let mut label = account.label.clone();
            if account.is_active {
                label.push_str(" (current)");
            }
            let detail = match account.kind {
                AccountKind::ChatGpt => account
                    .email
                    .clone()
                    .unwrap_or_else(|| "ChatGPT account".to_string()),
                AccountKind::ApiKey => account
                    .masked_api_key
                    .clone()
                    .unwrap_or_else(|| "API key".to_string()),
            };
            if self.current_highlight() == index {
                Line::from(vec![
                    format!("{indicator} ").cyan(),
                    label.cyan().bold(),
                    " ".into(),
                    detail.dim(),
                ])
            } else {
                Line::from(vec![
                    format!("{indicator} ").into(),
                    label.into(),
                    " ".into(),
                    detail.dim(),
                ])
            }
        } else {
            let indicator = if self.current_highlight() == index {
                ">"
            } else {
                " "
            };
            let text = "Add another account";
            if self.current_highlight() == index {
                Line::from(vec![
                    format!("{indicator} ").cyan(),
                    text.cyan(),
                ])
            } else {
                Line::from(vec![format!("{indicator} ").into(), text.into()])
            }
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from("Choose which account to use for this session:"),
            "".into(),
        ];

        let total = self.total_entries();
        for index in 0..total {
            lines.push(self.render_entry(index));
        }

        if let Some(error) = &self.error {
            lines.push("".into());
            lines.push(Line::from(error.clone().red()));
        }

        lines
    }
}

impl KeyboardHandler for AccountPickerWidget {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up | KeyCode::Char('k') => self.highlight_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.highlight_next(),
            KeyCode::Enter => {
                self.select_current();
            }
            _ => {}
        }
        self.request_frame.schedule_frame();
    }
}

impl StepStateProvider for AccountPickerWidget {
    fn get_step_state(&self) -> StepState {
        if self.selection.is_some() {
            StepState::Complete
        } else {
            StepState::InProgress
        }
    }
}

impl WidgetRef for AccountPickerWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title("Accounts")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.height == 0 || inner.width == 0 {
            return;
        }
        let paragraph = Paragraph::new(self.lines()).wrap(ratatui::widgets::Wrap { trim: true });
        paragraph.render(inner, buf);
    }
}
