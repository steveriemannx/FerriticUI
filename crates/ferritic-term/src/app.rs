//! The terminal application: PTY polling in `tick`, input forwarded in
//! `update`.

use ferritic_core::{App, Ctx};

use crate::session::Session;
use crate::terminal::{Msg, TerminalUi};

pub struct TermApp {
    session: Session,
    ui: TerminalUi,
}

impl TermApp {
    pub fn new(session: Session, ui: TerminalUi) -> Self {
        Self { session, ui }
    }
}

impl App for TermApp {
    type Msg = Msg;
    type Ui = TerminalUi;

    fn update(&mut self, msg: Msg, _cx: &mut Ctx) {
        match msg {
            Msg::Input(bytes) => {
                let _ = self.session.write(&bytes);
            }
        }
    }

    fn view(&self) -> &TerminalUi {
        &self.ui
    }

    fn view_mut(&mut self) -> &mut TerminalUi {
        &mut self.ui
    }

    fn tick(&mut self, cx: &mut Ctx) {
        let mut output = Vec::new();
        if let Err(error) = self.session.read(&mut output) {
            eprintln!("ferritic-term: pty read failed: {error}");
            cx.request_close();
            return;
        }
        if !output.is_empty() {
            self.ui.write(&output);
            cx.request_redraw();
        }

        let response = self.ui.take_input();
        if !response.is_empty() {
            let _ = self.session.write(&response);
        }

        if let Some((cols, rows)) = self.ui.sync_size() {
            self.session.resize(cols, rows);
            cx.request_redraw();
        }

        if self.session.closed() || self.session.exited() {
            cx.request_close();
        }
    }
}
