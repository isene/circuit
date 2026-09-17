//! circuit — build circuits in the terminal and watch them work.
//!
//! Parts and wires sit on a grid. Power on and a solver works out every
//! voltage and current, a millisecond at a time: LEDs glow, burn out, or
//! blink. Logic chips count and show digits. Challenges teach one idea
//! each. When the circuit settles the solver stops, and nothing runs
//! between key presses.

mod board;
mod lessons;
mod sim;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use board::{Board, Cell, Kind, Net, Part, E, JOIN, N, S, W};
use crust::cursor::Cursor;
use crust::{style, Crust, Input, Pane};
use lessons::{Progress, LESSONS};
use sim::{Dev, LedColor, Logic, Sim, LED_FULL};

/// Width of the panel on the right.
const INFO_W: usize = 40;
/// One solver step, in seconds.
const DT: f64 = 1e-3;
/// How often the screen follows a running circuit.
const TICK_MS: u64 = 100;
/// A node that moves less than this in a whole tick has settled.
const SETTLED: f64 = 1e-6;
const TRACE_N: usize = 36;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("circuit — build circuits in the terminal and watch them work (Fe2O3 suite)");
        println!();
        println!("Usage: circuit");
        println!();
        println!("Place batteries, resistors, capacitors, LEDs, switches, transistors, logic gates,");
        println!("clocks, counters, displays and 555 timers, wire them up and power on. Eight");
        println!("challenges run from lighting an LED to a stopwatch.");
        println!("Boards are kept in ~/.circuit/. Press ? inside for every key.");
        return;
    }
    if args.iter().any(|a| a == "-v" || a == "--version") {
        println!("circuit {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    Crust::init();
    Crust::set_app_identity("Circuit");
    Crust::clear_screen();
    let mut app = App::new();
    app.render();
    loop {
        let running = app.power && !app.steady;
        let key = if running { Input::getchr_ms(TICK_MS) } else { Input::getchr(None) };
        if let Some(k) = key {
            if app.key(&k) { break; }
        }
        if app.power && !app.steady { app.tick(); }
        app.render();
    }
    app.save();
    Crust::cleanup();
}

struct App {
    board: Board,
    /// 0 is free building; 1 and up are the challenges.
    mode: usize,
    cur: (i32, i32),
    view: (i32, i32),
    pen: bool,
    moving: Option<usize>,
    power: bool,
    net: Net,
    sim: Option<Sim>,
    steady: bool,
    last: Instant,
    trace: VecDeque<f64>,
    progress: Progress,
    done: Vec<bool>,
    note: Option<String>,
    reset_armed: bool,
    /// What the header and the panel last showed, so an unchanged one is
    /// not redrawn.
    shown: [String; 3],
    /// The board rows on screen; only rows that differ are written.
    rows: Vec<String>,
    /// Wire colours and LED brightness as last drawn. While a circuit
    /// runs, the board is only rebuilt when one of them changes.
    look: Vec<u8>,
    /// A key was pressed: the board must be rebuilt.
    dirty: bool,
    /// A pressed button and when it lets go.
    release: Option<(usize, Instant)>,
}

impl App {
    fn new() -> App {
        let mode = std::fs::read_to_string(dir().join("mode")).ok()
            .and_then(|s| s.trim().parse().ok()).filter(|&m| m <= LESSONS.len()).unwrap_or(1);
        let done = std::fs::read_to_string(dir().join("done")).unwrap_or_default();
        let board = load(mode);
        let net = board.net();
        let mut app = App {
            board, mode, cur: (2, 2), view: (0, 0), pen: false, moving: None, power: false,
            net, sim: None, steady: true, last: Instant::now(), trace: VecDeque::new(),
            progress: Progress::default(),
            done: (0..=LESSONS.len()).map(|i| done.split_whitespace().any(|d| d == i.to_string())).collect(),
            note: None, reset_armed: false, shown: Default::default(), rows: Vec::new(), look: Vec::new(), dirty: true,
            release: None,
        };
        app.changed();
        app
    }

    fn save(&self) {
        let _ = std::fs::create_dir_all(dir());
        let _ = std::fs::write(board_path(self.mode), self.board.to_text());
        let _ = std::fs::write(dir().join("mode"), self.mode.to_string());
        let done: Vec<String> = (1..self.done.len()).filter(|&i| self.done[i]).map(|i| i.to_string()).collect();
        let _ = std::fs::write(dir().join("done"), done.join(" "));
    }

    /// The board changed shape: rebuild the circuit, and restart the
    /// solver if the power is on.
    fn changed(&mut self) {
        self.net = self.board.net();
        self.progress.reset(self.net.circuit.devs.len());
        if self.power {
            self.sim = Some(Sim::new(&self.net.circuit));
            self.steady = false;
            self.last = Instant::now();
        }
    }

    fn set_mode(&mut self, mode: usize) {
        self.save();
        self.mode = mode;
        self.board = load(mode);
        self.power = false;
        self.sim = None;
        self.pen = false;
        self.moving = None;
        self.trace.clear();
        self.changed();
    }

    /// Run the solver for the time that passed since the last tick.
    fn tick(&mut self) {
        let Some(sim) = self.sim.as_mut() else { return };
        let now = Instant::now();
        let steps = now.duration_since(self.last).as_millis().clamp(1, 250) as usize;
        self.last = now;
        let mut moved = 0.0f64;
        let checking = self.mode > 0 && !self.done[self.mode];
        let mut met = false;
        for _ in 0..steps {
            moved = moved.max(sim.step(&self.net.circuit, DT));
            if checking && !met { met = self.progress.check(self.mode, &self.net.circuit, sim); }
        }
        for (d, &p) in self.net.dev_part.iter().enumerate() {
            if !sim.burnt[d] { continue; }
            if let Some(Some(part)) = self.board.parts.get_mut(p) {
                if !part.burnt {
                    part.burnt = true;
                    if let Dev::Led { burnt, .. } = &mut self.net.circuit.devs[d] { *burnt = true; }
                    self.note = Some("An LED burnt out: too much current. Put the cursor on it and press r for a new one.".into());
                }
            }
        }
        if met {
            self.done[self.mode] = true;
            self.note = Some(if self.mode < LESSONS.len() {
                "Well done! Press n for the next challenge.".into()
            } else {
                "Well done! That was the last challenge for now.".into()
            });
            self.save();
        }
        if let Some((i, at)) = self.release {
            if now >= at {
                self.release = None;
                self.set_closed(i, false);
            }
        }
        self.steady = moved < SETTLED && !self.net.circuit.has_clock() && self.release.is_none();
        if let Some(v) = self.probe() {
            self.trace.push_back(v);
            while self.trace.len() > TRACE_N { self.trace.pop_front(); }
        }
    }

    /// Handle a key. True means quit.
    fn key(&mut self, k: &str) -> bool {
        self.note = None;
        self.dirty = true;
        if k != "R" { self.reset_armed = false; }
        match k {
            "q" => return true,
            "UP" | "k" => self.go(N),
            "DOWN" | "j" => self.go(S),
            "LEFT" | "h" => self.go(W),
            "RIGHT" | "l" => self.go(E),
            "ESC" => { self.pen = false; self.moving = None; }
            "w" => { self.moving = None; self.pen = !self.pen; }
            "m" => {
                self.pen = false;
                self.moving = if self.moving.is_some() { None } else { self.board.part_at(self.cur) };
                if self.moving.is_none() && self.board.part_at(self.cur).is_none() {
                    self.note = Some("Put the cursor on a part to move it.".into());
                }
            }
            "1" | "2" | "3" | "4" | "5" | "6" => {
                let kind = Kind::ALL[k.parse::<usize>().unwrap_or(1) - 1];
                if self.board.place(Part::new(kind, self.cur.0 - 1, self.cur.1)).is_some() {
                    self.changed();
                } else {
                    self.note = Some("No room here: a part needs three cells, and a free cell at each end.".into());
                }
            }
            "x" | "DEL" => {
                if self.board.delete(self.cur) { self.changed(); }
            }
            "o" => {
                if self.board.part_at(self.cur).and_then(|i| self.board.part(i)).is_some_and(|p| p.kind.is_chip()) {
                    self.note = Some("Chips don't turn: inputs stay on the left, outputs on the right.".into());
                } else {
                    self.edit(|p| { p.turn(); }, true);
                }
            }
            "a" => {
                if let Some(kind) = self.pick() {
                    if self.board.place(Part::new(kind, self.cur.0 - 1, self.cur.1)).is_some() {
                        self.changed();
                    } else {
                        self.note = Some("No room here for that part.".into());
                    }
                }
            }
            "+" | "=" | "-" => {
                let up = k != "-";
                self.edit(move |p| match p.kind {
                    Kind::Led => p.color = board::next_color(p.color, up),
                    Kind::Switch => p.closed = !p.closed,
                    _ => p.value = board::step_value(p.kind, p.value, up),
                }, false);
            }
            " " => self.flip_switch(),
            "r" => self.edit(|p| p.burnt = false, false),
            "." => {
                if self.board.toggle_join(self.cur) {
                    self.changed();
                } else {
                    self.note = Some("A join only matters where four wires meet.".into());
                }
            }
            "p" => {
                self.power = !self.power;
                self.trace.clear();
                if self.power { self.changed(); } else { self.sim = None; self.steady = true; }
            }
            "n" => self.set_mode((self.mode + 1) % (LESSONS.len() + 1)),
            "N" => self.set_mode((self.mode + LESSONS.len()) % (LESSONS.len() + 1)),
            "R" => {
                if self.reset_armed {
                    self.board = if self.mode == 0 { Board::default() } else { lessons::starter(self.mode) };
                    self.reset_armed = false;
                    self.changed();
                } else {
                    self.reset_armed = true;
                    self.note = Some("Press R again to put this board back to its start.".into());
                }
            }
            "?" => self.help(),
            "RESIZE" => { self.shown = Default::default(); self.rows.clear(); }
            _ => {}
        }
        false
    }

    /// Move the cursor, laying a wire or carrying a part if one is active.
    fn go(&mut self, dir: u8) {
        let (dx, dy) = board::offset(dir);
        if let Some(i) = self.moving {
            if self.board.change(i, |p| { p.x += dx; p.y += dy; }) {
                self.cur = (self.cur.0 + dx, self.cur.1 + dy);
                self.changed();
            } else {
                self.note = Some("No room there.".into());
            }
            return;
        }
        if self.pen {
            if !self.board.wire(self.cur, dir) {
                self.note = Some("A wire can't go through a part. Join it at the pin at either end.".into());
                return;
            }
            self.changed();
        }
        self.cur = (self.cur.0 + dx, self.cur.1 + dy);
    }

    /// Change the part under the cursor. `shape` edits move its pins, so
    /// they need room.
    fn edit(&mut self, f: impl FnOnce(&mut Part), shape: bool) {
        let Some(i) = self.board.part_at(self.cur) else {
            self.note = Some("Put the cursor on a part first.".into());
            return;
        };
        if !self.board.change(i, f) {
            self.note = Some(if shape { "No room to turn it here." } else { "That can't change." }.into());
            return;
        }
        // A turned part may no longer cover the cursor: follow its body.
        if let Some(p) = self.board.part(i) {
            if self.board.part_at(self.cur) != Some(i) { self.cur = (p.x + 1, p.y); }
        }
        self.changed();
    }

    /// Space on a switch flips it; on a button it presses it for a moment.
    /// Neither restarts the circuit.
    fn flip_switch(&mut self) {
        let Some(i) = self.board.part_at(self.cur) else { return };
        match self.board.part(i).map(|p| (p.kind, p.closed)) {
            Some((Kind::Switch, closed)) => self.set_closed(i, !closed),
            Some((Kind::Button, _)) if self.power => {
                self.set_closed(i, true);
                self.release = Some((i, Instant::now() + Duration::from_millis(300)));
            }
            Some((Kind::Button, _)) => self.note = Some("Power on first: a button only holds while you press.".into()),
            _ => {}
        }
    }

    fn set_closed(&mut self, i: usize, now: bool) {
        if let Some(Some(p)) = self.board.parts.get_mut(i) { p.closed = now; }
        if let Some(d) = self.net.part_dev[i] {
            if let Dev::Switch { closed, .. } = &mut self.net.circuit.devs[d] { *closed = now; }
        }
        self.steady = false;
        self.last = Instant::now();
        self.dirty = true;
    }

    /// The part picker: every part with a line about it.
    fn pick(&mut self) -> Option<Kind> {
        let (cols, rows) = Crust::terminal_size();
        let n = Kind::ALL.len();
        let (w, h) = (52.min(cols.saturating_sub(4)), (n as u16 + 4).min(rows.saturating_sub(2)));
        let mut p = Pane::new((cols - w) / 2 + 1, (rows - h) / 2 + 1, w, h, 252, 235);
        p.border = true;
        p.scroll = false;
        p.wrap = false;
        let mut sel = 0usize;
        let chosen = loop {
            let mut text = vec![format!("  {}", style::fg("Add a part   ↑ ↓ choose   Enter place   Esc", 245)), String::new()];
            for (k, kind) in Kind::ALL.iter().enumerate() {
                let line = format!(" {:<11} {}", kind.name(), kind.blurb());
                text.push(if k == sel { style::styled(&format!("▶{line}"), Some(16), Some(214), "") } else { format!(" {line}") });
            }
            p.set_text(&text.join("\n"));
            p.full_refresh();
            match Input::getchr(None).as_deref() {
                Some("UP") | Some("k") => sel = (sel + n - 1) % n,
                Some("DOWN") | Some("j") => sel = (sel + 1) % n,
                Some("ENTER") => break Some(Kind::ALL[sel]),
                Some("ESC") | Some("a") | Some("q") => break None,
                _ => {}
            }
        };
        Crust::clear_screen();
        self.shown = Default::default();
        self.rows.clear();
        chosen
    }

    fn help(&mut self) {
        let (cols, rows) = Crust::terminal_size();
        let lines = HELP.lines().count() as u16 + 2;
        let (w, h) = (64.min(cols.saturating_sub(4)), lines.min(rows.saturating_sub(2)));
        let mut p = Pane::new((cols - w) / 2 + 1, (rows - h) / 2 + 1, w, h, 252, 235);
        p.border = true;
        p.scroll = false;
        p.set_text(HELP);
        p.full_refresh();
        let _ = Input::getchr(None);
        Crust::clear_screen();
        self.shown = Default::default();
        self.rows.clear();
    }

    /// The voltage the probe reads: a wire's node, across a two-pin part,
    /// or a logic part's first pin.
    fn probe(&self) -> Option<f64> {
        let sim = self.sim.as_ref()?;
        if let Some(&n) = self.net.node_of.get(&self.cur) { return Some(sim.v[n]); }
        let p = self.board.part(self.board.part_at(self.cur)?)?;
        let pins = p.pins();
        let at = |c: &(i32, i32)| self.net.node_of.get(c).map(|&n| sim.v[n]);
        match p.kind {
            Kind::Npn => Some(at(&pins[0])? - at(&pins[2])?),
            k if pins.len() == 2 && k.gate().is_none() => Some(at(&pins[0])? - at(&pins[1])?),
            _ => at(&pins[0]),
        }
    }

    /// A logic part's state, while it has power.
    fn logic_of(&self, i: usize) -> Option<Logic> {
        let sim = self.sim.as_ref().filter(|_| self.power)?;
        self.net.part_dev.get(i).copied().flatten().map(|d| sim.logic[d]).filter(|l| l.on)
    }

    fn vmax(&self) -> f64 {
        self.board.parts.iter().flatten().filter(|p| p.kind == Kind::Battery).map(|p| p.value).fold(0.0, f64::max).max(1.5)
    }

    // ── Drawing ────────────────────────────────────────────────────────

    fn render(&mut self) {
        let (cols, rows) = Crust::terminal_size();
        let (cols, rows) = (cols as usize, rows as usize);
        let bw = cols.saturating_sub(INFO_W).max(20);
        let bh = rows.saturating_sub(1).max(4);
        // Keep the cursor in view.
        let (bwi, bhi) = (bw as i32, bh as i32);
        if self.cur.0 < self.view.0 + 1 { self.view.0 = self.cur.0 - 1; }
        if self.cur.0 > self.view.0 + bwi - 2 { self.view.0 = self.cur.0 - bwi + 2; }
        if self.cur.1 < self.view.1 + 1 { self.view.1 = self.cur.1 - 1; }
        if self.cur.1 > self.view.1 + bhi - 2 { self.view.1 = self.cur.1 - bhi + 2; }

        let head = self.header(cols);
        if head != self.shown[0] {
            let mut p = Pane::new(1, 1, cols as u16, 1, 255, 236);
            p.wrap = false;
            p.scroll = false;
            p.set_text(&head);
            p.refresh();
            self.shown[0] = head;
        }
        let look = self.look();
        if self.dirty || look != self.look {
            let lines = self.board_lines(bw, bh);
            if self.rows.len() != lines.len() { self.rows = vec![String::new(); lines.len()]; }
            let mut frame = String::new();
            for (i, line) in lines.into_iter().enumerate() {
                if line != self.rows[i] {
                    frame.push_str(&Cursor::at(1, i as u16 + 2));
                    frame.push_str(&line);
                    self.rows[i] = line;
                }
            }
            if !frame.is_empty() {
                use std::io::Write;
                let mut out = std::io::stdout();
                let _ = out.write_all(frame.as_bytes());
                let _ = out.flush();
            }
            self.look = look;
            self.dirty = false;
        }
        let info = self.info(cols - bw);
        if info != self.shown[2] {
            let mut p = Pane::new(bw as u16 + 1, 2, (cols - bw) as u16, bh as u16, 252, 234);
            p.wrap = false;
            p.scroll = false;
            p.set_text(&info);
            p.refresh();
            self.shown[2] = info;
        }
    }

    /// Everything on the board that the solver can change: each node's
    /// wire colour and each LED's brightness step.
    fn look(&self) -> Vec<u8> {
        let Some(sim) = self.sim.as_ref().filter(|_| self.power) else { return Vec::new() };
        let vmax = self.vmax();
        let mut out: Vec<u8> = sim.v.iter().map(|&v| volt_step(v, vmax)).collect();
        for (d, &p) in self.net.dev_part.iter().enumerate() {
            if let Some(part) = self.board.part(p).filter(|x| x.kind == Kind::Led) {
                out.push(if part.burnt { 255 } else { led_color(part.color, (sim.current[d] / LED_FULL).clamp(0.0, 1.0)) });
            }
            let l = sim.logic[d];
            out.push(l.out.iter().enumerate().fold(l.on as u8, |acc, (k, &b)| acc | (b as u8) << (k + 1)));
            out.push(l.value);
        }
        out
    }

    fn header(&self, cols: usize) -> String {
        let title = if self.mode == 0 {
            "Free build".to_string()
        } else {
            let tick = if self.done[self.mode] { " ✓" } else { "" };
            format!("Challenge {}: {}{tick}", self.mode, LESSONS[self.mode - 1].title)
        };
        let power = match &self.sim {
            Some(_) if self.power && self.steady => style::fg("● on, settled", 46),
            Some(s) if self.power => style::fg(&format!("● on {:.1} s", s.t), 46),
            _ => style::fg("○ off", 245),
        };
        let mut facts = vec![style::bold("circuit"), title, power];
        if self.pen { facts.push(style::fg("drawing wire", 214)); }
        if self.moving.is_some() { facts.push(style::fg("moving part", 213)); }
        let keys = "1-6 add   a all parts   w wire   p power   n challenge   ? help   q quit";
        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
        let width = |s: &str| crust::strip_ansi(s).chars().count();
        let right = width(keys) + 3 + width(&version) + 1;
        let mut left = format!(" {}", facts.join("   "));
        while facts.len() > 1 && width(&left) + right + 2 > cols {
            facts.pop();
            left = format!(" {}", facts.join("   "));
        }
        let pad = cols.saturating_sub(width(&left) + right).max(1);
        format!("{left}{}{keys}   {} ", " ".repeat(pad), style::fg(&version, 245))
    }

    fn board_lines(&self, bw: usize, bh: usize) -> Vec<String> {
        let vmax = self.vmax();
        let cursor_bg = if self.pen { 130 } else if self.moving.is_some() { 90 } else { 24 };
        let used: std::collections::HashSet<i32> = self.board.cells.keys().map(|k| k.1).collect();
        (0..bh).map(|row| {
            let y = self.view.1 + row as i32;
            if y != self.cur.1 && !used.contains(&y) {
                // Nothing on this row: just the grid dots.
                let dots: String = (0..bw).map(|col| {
                    let x = self.view.0 + col as i32;
                    if x % 2 == 0 && y % 2 == 0 { '·' } else { ' ' }
                }).collect();
                return format!("{}{}{}{dots}{}", style::RESET, style::set_fg(237), style::set_bg(16), style::RESET);
            }
            let mut line = String::with_capacity(bw * 4);
            let mut last: Option<(u8, Option<u8>, bool)> = None;
            for col in 0..bw {
                let at = (self.view.0 + col as i32, y);
                let (ch, fg, bg, bold) = self.glyph(at, vmax);
                let bg = Some(if at == self.cur { cursor_bg } else { bg.unwrap_or(16) });
                if last != Some((fg, bg, bold)) {
                    line.push_str(style::RESET);
                    line.push_str(&style::set_fg(fg));
                    if let Some(b) = bg { line.push_str(&style::set_bg(b)); }
                    if bold { line.push_str(style::BOLD); }
                    last = Some((fg, bg, bold));
                }
                line.push(ch);
            }
            line.push_str(style::RESET);
            line
        }).collect()
    }

    /// One cell: character, colour, background, bold.
    fn glyph(&self, at: (i32, i32), vmax: f64) -> (char, u8, Option<u8>, bool) {
        const BOX: [char; 16] = ['•', '╵', '╶', '└', '╷', '│', '┌', '├', '╴', '┘', '─', '┴', '┐', '┤', '┬', '┼'];
        let wire_color = || self.volt_color(at, vmax);
        match self.board.cells.get(&at) {
            // Spaces share the dots' colour, so an empty row needs one colour code, not one per cell.
            None => (if at.0 % 2 == 0 && at.1 % 2 == 0 { '·' } else { ' ' }, 237, None, false),
            Some(Cell::Wire(m)) if *m & JOIN != 0 && *m & 15 == 15 => ('╋', wire_color(), None, true),
            Some(Cell::Wire(m)) => (BOX[*m as usize & 15], wire_color(), None, false),
            Some(Cell::Pin(i, k, m)) => {
                let Some(p) = self.board.part(*i) else { return ('?', 196, None, false) };
                if let Some(plus) = p.kind.power_pin() {
                    if *k == plus { return ('+', 196, None, true); }
                    if *k == plus + 1 { return ('−', 81, None, true); }
                }
                match p.kind {
                    Kind::Battery if *k == 0 => ('+', 196, None, true),
                    Kind::Battery => ('−', 81, None, true),
                    Kind::Npn => (['c', 'b', 'e'][*k], wire_color(), None, true),
                    Kind::Not => (['a', 'q'][*k], wire_color(), None, true),
                    Kind::Clock => ('q', wire_color(), None, true),
                    kind if kind.gate().is_some() => (['a', 'b', 'q'][*k], wire_color(), None, true),
                    _ => (BOX[(*m | p.inward(at)) as usize & 15], wire_color(), None, false),
                }
            }
            Some(Cell::Body(i)) => {
                let Some(p) = self.board.part(*i) else { return ('?', 196, None, false) };
                let (col, row) = ((at.0 - p.x).max(0) as usize, (at.1 - p.y).max(0) as usize);
                if p.kind.is_chip() { return self.chip_cell(*i, p, col, row); }
                let chars: Vec<char> = self.body_text(*i, p).chars().collect();
                let ch = chars.get(col).copied().unwrap_or(' ');
                let high = self.logic_of(*i).is_some_and(|l| l.out[0]);
                match p.kind {
                    Kind::Button => (ch, 229, None, true),
                    Kind::Clock => (ch, if high { 231 } else { 183 }, Some(if high { 127 } else { 53 }), true),
                    Kind::Not | Kind::And | Kind::Or | Kind::Nand | Kind::Nor | Kind::Xor => (ch, if high { 46 } else { 157 }, Some(23), true),
                    Kind::Counter | Kind::Display | Kind::Timer => (ch, 252, Some(238), false),
                    Kind::Battery => (ch, 16, Some(178), true),
                    Kind::Resistor => (ch, 230, Some(94), false),
                    Kind::Capacitor => (ch, 195, Some(24), false),
                    Kind::Npn => (ch, 16, Some(67), true),
                    Kind::Switch => (ch, 229, None, true),
                    Kind::Led => {
                        if p.burnt { return (ch, 88, None, true); }
                        let glow = self.led_glow(*i);
                        (ch, led_color(p.color, glow), None, glow > 0.5)
                    }
                }
            }
        }
    }

    /// One cell of a chip: labels down the edges, the name, and what it holds.
    fn chip_cell(&self, i: usize, p: &Part, col: usize, row: usize) -> (char, u8, Option<u8>, bool) {
        let logic = self.logic_of(i);
        let label = |c: char, lit: bool| (c, if lit { 46 } else { 250 }, Some(238), lit);
        match p.kind {
            Kind::Counter => {
                if col == 0 { return label(['>', 'r', ' ', ' '][row.min(3)], logic.is_some_and(|l| row < 2 && l.ins[row])); }
                if col == 4 { return label(['1', '2', '4', '8'][row.min(3)], logic.is_some_and(|l| l.out[row.min(3)])); }
                let text = match row {
                    0 => "CNT".to_string(),
                    2 => logic.map(|l| format!("{:>2} ", l.value)).unwrap_or_default(),
                    _ => String::new(),
                };
                let ch = text.chars().nth(col - 1).unwrap_or(' ');
                (ch, if row == 0 { 230 } else { 231 }, Some(238), true)
            }
            Kind::Timer => {
                if col == 0 { return label(['t', 'h', 'd'][row.min(2)], false); }
                if col == 4 { return label(if row == 0 { 'o' } else { ' ' }, row == 0 && logic.is_some_and(|l| l.out[0])); }
                let ch = if row == 1 { ['5', '5', '5'][col - 1] } else { ' ' };
                (ch, 230, Some(238), true)
            }
            _ => {
                // The display: input labels, then a digit three cells wide and five tall.
                // Each cell lights when any of the seven segments (bit 0 top, clockwise, bit 6 middle) touching it is on.
                if col == 0 { return label(['1', '2', '4', '8', ' '][row.min(4)], logic.is_some_and(|l| row < 4 && l.ins[row])); }
                if col == 4 { return (' ', 250, Some(232), false); }
                const SEGMENTS: [u8; 16] = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F, 0x77, 0x7C, 0x39, 0x5E, 0x79, 0x71];
                const TOUCH: [[u8; 3]; 5] = [[0x21, 0x01, 0x03], [0x20, 0, 0x02], [0x70, 0x40, 0x46], [0x10, 0, 0x04], [0x18, 0x08, 0x0C]];
                let touch = TOUCH[row.min(4)][col - 1];
                if touch == 0 { return (' ', 236, Some(232), false); }
                let lit = logic.is_some_and(|l| SEGMENTS[l.value as usize & 15] & touch != 0);
                let ch = '█';
                (ch, if lit { 196 } else { 236 }, Some(232), lit)
            }
        }
    }

    fn body_text(&self, _i: usize, p: &Part) -> String {
        match p.kind {
            Kind::Button => if p.closed { "─●─".into() } else { "─○─".into() },
            Kind::Not => "NOT ".into(),
            Kind::And => "AND ".into(),
            Kind::Or => " OR ".into(),
            Kind::Nand => "NAND".into(),
            Kind::Nor => "NOR ".into(),
            Kind::Xor => "XOR ".into(),
            Kind::Clock => board::code(p.kind, p.value),
            Kind::Counter | Kind::Display | Kind::Timer => String::new(),
            Kind::Battery | Kind::Resistor | Kind::Capacitor => format!("{:<3}", board::code(p.kind, p.value)),
            Kind::Npn => "NPN".into(),
            Kind::Led if p.burnt => " ✕ ".into(),
            Kind::Led => ["━▶┃", "┃◀━", " ▼ ", " ▲ "][p.orient as usize & 3].into(),
            Kind::Switch => match (p.orient < 2, p.closed) {
                (true, true) => "───".into(),
                (true, false) => "─╱ ".into(),
                (false, true) => " │ ".into(),
                (false, false) => " ╱ ".into(),
            },
        }
    }

    /// How brightly LED part `i` glows, 0 to 1.
    fn led_glow(&self, i: usize) -> f64 {
        match (&self.sim, self.net.part_dev.get(i).copied().flatten()) {
            (Some(s), Some(d)) if self.power => (s.current[d] / LED_FULL).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    /// Wires glow green with voltage, brighter for higher, and red below
    /// zero. Gray when the power is off.
    fn volt_color(&self, at: (i32, i32), vmax: f64) -> u8 {
        let (Some(sim), Some(&n)) = (self.sim.as_ref().filter(|_| self.power), self.net.node_of.get(&at)) else { return 250 };
        volt_step(sim.v[n], vmax)
    }

    fn info(&self, width: usize) -> String {
        let w = width.saturating_sub(3);
        let mut l: Vec<String> = vec![String::new()];
        if self.mode == 0 {
            l.push(style::bold("Free build"));
            l.extend(wrap("Build anything you like. Press n to pick a challenge.", w));
        } else {
            let lesson = &LESSONS[self.mode - 1];
            l.push(style::bold(&format!("Challenge {} of {}: {}", self.mode, LESSONS.len(), lesson.title)));
            l.push(String::new());
            l.extend(wrap(lesson.text, w));
            l.push(String::new());
            let (mark, color) = if self.done[self.mode] { ("✓ Done: ", 46) } else { ("○ Goal: ", 222) };
            let goal = wrap(&format!("{mark}{}", lesson.goal), w);
            l.extend(goal.iter().map(|g| style::fg(g, color)));
        }
        l.push(String::new());
        l.push(style::fg("Under the cursor", 245));
        for d in self.describe() { l.extend(wrap(&d, w)); }
        if self.power && self.trace.len() > 1 {
            let vmax = self.vmax();
            const TICKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            let scale = format!("0–{} V", vmax);
            let room = w.saturating_sub(scale.chars().count() + 1);
            let bars: String = self.trace.iter().skip(self.trace.len().saturating_sub(room))
                .map(|&v| TICKS[((v / vmax).clamp(0.0, 1.0) * 7.0).round() as usize]).collect();
            l.push(format!("{} {}", style::fg(&bars, 46), style::fg(&scale, 245)));
        }
        if let Some(n) = &self.note {
            l.push(String::new());
            l.extend(wrap(n, w).iter().map(|s| style::fg(s, 214)));
        }
        l.push(String::new());
        l.push(style::fg("Parts", 245));
        l.push("1 battery    2 resistor   3 capacitor".into());
        l.push("4 LED        5 switch     6 transistor".into());
        l.extend(wrap("a picks any part: gates, clock, counter, display, 555, button.", w));
        l.iter().map(|s| format!("  {s}")).collect::<Vec<_>>().join("\n")
    }

    /// Plain lines about whatever is under the cursor.
    fn describe(&self) -> Vec<String> {
        let sim = self.sim.as_ref().filter(|_| self.power);
        let Some(i) = self.board.part_at(self.cur) else {
            return match (self.board.cells.get(&self.cur), sim, self.net.node_of.get(&self.cur)) {
                (Some(Cell::Wire(m)), _, _) if board::crossing(*m) => vec!["Two wires cross here without touching. . joins them.".into()],
                (Some(Cell::Wire(m)), _, _) if *m & 15 == 15 => vec!["Four wires joined here. . separates them.".into()],
                (Some(Cell::Wire(_)), Some(s), Some(&n)) => vec![format!("Wire at {}", volts(s.v[n]))],
                (Some(Cell::Wire(_)), _, _) => vec!["Wire. x deletes it.".into()],
                _ if self.pen => vec!["Move to draw a wire. w or Esc stops.".into()],
                _ => vec!["Empty. Press 1 to 6 or a to add a part here, or w to draw a wire.".into()],
            };
        };
        let Some(p) = self.board.part(i) else { return vec![] };
        let d = self.net.part_dev[i];
        let node = |c: &(i32, i32)| self.net.node_of.get(c).copied();
        let pins = p.pins();
        let across = |s: &Sim| match (node(&pins[0]), node(&pins[1])) {
            (Some(a), Some(b)) => s.v[a] - s.v[b],
            _ => 0.0,
        };
        let mut out = Vec::new();
        match p.kind {
            Kind::Battery => {
                out.push(format!("Battery, {}. + is red, − is blue.", board::pretty(p.kind, p.value)));
                if let (Some(s), Some(d)) = (sim, d) { out.push(format!("Giving {}", amps(s.current[d]))); }
                out.push("+ and − change the voltage.".into());
            }
            Kind::Resistor | Kind::Capacitor => {
                out.push(format!("{}, {}", cap(p.kind.name()), board::pretty(p.kind, p.value)));
                if let (Some(s), Some(d)) = (sim, d) {
                    let v = across(s);
                    let i = s.current[d];
                    out.push(format!("{} across, {}", volts(v), amps(i)));
                    if p.kind == Kind::Resistor {
                        let watts = v * i;
                        let hot = if watts > 0.25 { " — too hot for a small resistor!" } else { "" };
                        out.push(format!("Turning {} into heat{hot}", watts_str(watts)));
                    }
                }
                out.push("+ and − change the value.".into());
            }
            Kind::Led => {
                let color = match p.color { LedColor::Red => "Red", LedColor::Green => "Green", LedColor::Yellow => "Yellow", LedColor::Blue => "Blue" };
                out.push(format!("{color} LED. Current flows from the arrow side to the bar side."));
                if p.burnt {
                    out.push("Burnt out. r puts in a new one.".into());
                } else if let (Some(s), Some(d)) = (sim, d) {
                    out.push(format!("{} across, {}, glowing {:.0}%", volts(across(s)), amps(s.current[d]), self.led_glow(i) * 100.0));
                }
                out.push("+ and − change the colour.".into());
            }
            Kind::Switch => {
                out.push(format!("Switch, {}. Space flips it.", if p.closed { "closed" } else { "open" }));
                if let (Some(s), Some(d)) = (sim, d) { out.push(format!("Carrying {}", amps(s.current[d]))); }
            }
            Kind::Npn => {
                out.push("NPN transistor: collector c, base b, emitter e.".into());
                if let (Some(s), Some(d)) = (sim, d) {
                    out.push(format!("Collector {}, base {}", amps(s.current[d]), amps(s.base[d])));
                }
            }
            Kind::Button => {
                out.push("Button: Space presses it for a moment.".into());
                if let (Some(s), Some(d)) = (sim, d) { out.push(format!("Carrying {}", amps(s.current[d]))); }
            }
            kind @ (Kind::Not | Kind::And | Kind::Or | Kind::Nand | Kind::Nor | Kind::Xor) => {
                let rule = match kind {
                    Kind::Not => "q is high when a is low",
                    Kind::And => "q is high only when a and b are both high",
                    Kind::Or => "q is high when a or b is high",
                    Kind::Nand => "q is low only when a and b are both high",
                    Kind::Nor => "q is low when a or b is high",
                    _ => "q is high when a and b differ",
                };
                out.push(format!("{}: {rule}.", kind.name()));
                if let Some(l) = self.logic_of(i) {
                    let hl = |b: bool| if b { "high" } else { "low" };
                    out.push(if kind == Kind::Not {
                        format!("a {}, q {}", hl(l.ins[0]), hl(l.out[0]))
                    } else {
                        format!("a {}, b {}, q {}", hl(l.ins[0]), hl(l.ins[1]), hl(l.out[0]))
                    });
                }
            }
            Kind::Clock => {
                out.push(format!("Clock, {}: q goes high and low by itself.", board::pretty(p.kind, p.value)));
                out.push("+ and − change the speed.".into());
            }
            Kind::Counter => {
                out.push("Counter: adds one each time > goes high; a high r puts it back to 0. 1, 2, 4 and 8 show the count in binary.".into());
                if let Some(l) = self.logic_of(i) { out.push(format!("Count {} = {}", l.value, binary_sum(l.value))); }
            }
            Kind::Display => {
                out.push("Display: shows the digit its inputs 1, 2, 4 and 8 add up to, 0 to F.".into());
                if let Some(l) = self.logic_of(i) { out.push(format!("Showing {:X} = {}", l.value, binary_sum(l.value))); }
            }
            Kind::Timer => {
                out.push("555 timer: o goes high when t drops below a third of its supply, and low when h rises above two thirds. While o is low, d pulls down.".into());
                if let Some(l) = self.logic_of(i) { out.push(format!("o {}", if l.out[0] { "high" } else { "low" })); }
            }
        }
        if p.kind.power_pin().is_some() && self.power && self.logic_of(i).is_none() {
            out.push("No power: wire + to the battery's + and − to its −.".into());
        }
        out.push(if p.kind.is_chip() { "m moves it, x deletes it." } else { "o turns it, m moves it, x deletes it." }.into());
        out
    }
}

const HELP: &str = "
  circuit keys

  Arrows / h j k l   move the cursor
  1 2 3 4 5 6        add a battery, resistor, capacitor,
                     LED, switch or transistor
  a                  pick any part: gates, clock, counter,
                     display, 555 timer, button
  w                  draw a wire: move to lay it, w stops
  .                  join two wires that cross (they don't touch)
  x                  delete the wire or part under the cursor
  o                  turn a part (chips don't turn)
  m                  move a part: arrows carry it, m drops it
  + -                change a value or an LED's colour
  Space              flip a switch, press a button
  r                  replace a burnt-out LED
  p                  power on or off
  n N                next or previous challenge
  R R                put the board back to its start
  q                  quit (the board is kept)

  Wires glow green with voltage, brighter for higher,
  and red below zero. LEDs glow with their current.
  Every chip needs its + and − wired to a battery.
  An input reads high above 60% of that voltage
  and low below 40%.
  A part's value is printed the way it is marked:
  4k7 is 4.7 kΩ, 10µ is 10 µF, 9V0 is 9 V.

  Any key closes this.";

/// A voltage as a wire colour: gray near zero, green steps up to the
/// battery's voltage, red steps below zero.
fn volt_step(v: f64, vmax: f64) -> u8 {
    let f = (v.abs() / vmax).clamp(0.0, 1.0);
    if f < 0.02 { return 244; }
    let step = ((f * 4.0).round() as usize).min(4);
    if v > 0.0 { [22, 28, 34, 40, 46][step] } else { [52, 88, 124, 160, 196][step] }
}

fn led_color(c: LedColor, glow: f64) -> u8 {
    if glow < 0.03 { return 240; }
    let ramp = match c {
        LedColor::Red => [52, 88, 124, 160, 196],
        LedColor::Green => [22, 28, 34, 40, 46],
        LedColor::Yellow => [58, 100, 142, 184, 226],
        LedColor::Blue => [17, 18, 20, 27, 33],
    };
    ramp[((glow * 4.0).round() as usize).min(4)]
}

/// A count as the powers of two it is made of: 13 is "8 + 4 + 1".
fn binary_sum(v: u8) -> String {
    let parts: Vec<String> = [8, 4, 2, 1].iter().filter(|&&b| v & b != 0).map(|b| b.to_string()).collect();
    if parts.is_empty() { "0".into() } else { parts.join(" + ") }
}

fn volts(v: f64) -> String {
    format!("{:.2} V", v)
}

fn amps(i: f64) -> String {
    let a = i.abs();
    if a >= 1.0 { format!("{:.2} A", i) } else if a >= 1e-3 { format!("{:.1} mA", i * 1e3) } else { format!("{:.0} µA", i * 1e6) }
}

fn watts_str(w: f64) -> String {
    if w.abs() >= 1.0 { format!("{:.2} W", w) } else { format!("{:.0} mW", w * 1e3) }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Break plain text into lines of at most `width` characters.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() { line.push(' '); }
        line.push_str(word);
    }
    if !line.is_empty() { out.push(line); }
    out
}

fn dir() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".circuit")
}

fn board_path(mode: usize) -> PathBuf {
    dir().join(if mode == 0 { "free.txt".to_string() } else { format!("challenge{mode}.txt") })
}

/// A saved board, or the challenge's starting parts.
fn load(mode: usize) -> Board {
    match std::fs::read_to_string(board_path(mode)) {
        Ok(text) => Board::from_text(&text),
        Err(_) => lessons::starter(mode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_wraps_on_words() {
        assert_eq!(wrap("one two three four", 9), vec!["one two", "three", "four"]);
        assert_eq!(amps(0.0151), "15.1 mA");
        assert_eq!(amps(0.00082), "820 µA");
        assert_eq!(binary_sum(13), "8 + 4 + 1");
        assert_eq!(binary_sum(0), "0");
    }
}
