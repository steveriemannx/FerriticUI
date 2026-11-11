//! The terminal application: PTY polling in `tick`, input forwarded in
//! `update`.

use ferritic_core::{App, Ctx};

use crate::session::Session;
use crate::terminal::{Msg, TerminalUi};

pub struct TermApp {
    session: Session,
    ui: TerminalUi,
    /// The shell or PTY is gone; keep the window and its last screen visible
    /// instead of making the window disappear under the user.
    exited: bool,
}

impl TermApp {
    pub fn new(session: Session, ui: TerminalUi) -> Self {
        Self {
            session,
            ui,
            exited: false,
        }
    }

    fn finish(&mut self, cx: &mut Ctx, reason: &str) {
        if self.exited {
            return;
        }
        self.exited = true;
        let message = format!("\r\n[{reason}; close the window]\r\n");
        self.ui.write(message.as_bytes());
        cx.request_redraw();
    }
}

impl App for TermApp {
    type Msg = Msg;
    type Ui = TerminalUi;

    fn update(&mut self, msg: Msg, _cx: &mut Ctx) {
        if self.exited {
            return;
        }
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
        if self.exited {
            return;
        }

        let mut output = Vec::new();
        if let Err(error) = self.session.read(&mut output) {
            eprintln!("ferritic-term: pty read failed: {error}");
            self.finish(cx, "pty read failed");
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
            self.finish(cx, "shell exited");
        }
    }
}
