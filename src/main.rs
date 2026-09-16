//! circuit — build circuits in the terminal and watch them work.
//!
//! Parts and wires sit on a grid. Power on and a solver works out every
//! voltage and current, a millisecond at a time: LEDs glow, burn out, or
//! blink. Challenges teach one idea each. When the circuit settles the
//! solver stops, and nothing runs between key presses.

mod board;
mod lessons;
mod sim;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use board::{Board, Cell, Kind, Net, Part, E, JOIN, N, S, W};
use crust::cursor::Cursor;
use crust::{style, Crust, Input, Pane};
use lessons::{Progress, LESSONS};
use sim::{Dev, LedColor, Sim, LED_FULL};

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
        println!("Place batteries, resistors, capacitors, LEDs, switches and transistors,");
        println!("wire them up and power on. Three challenges teach LEDs, transistors and a blinker.");
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
        for _ in 0..steps { moved = moved.max(sim.step(&self.net.circuit, DT)); }
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
        if self.mode > 0 && !self.done[self.mode] && self.progress.check(self.mode, &self.net.circuit, sim) {
            self.done[self.mode] = true;
            self.note = Some(if self.mode < LESSONS.len() {
                "Well done! Press n for the next challenge.".into()
            } else {
                "Well done! That was the last challenge for now.".into()
            });
            self.save();
        }
        self.steady = moved < SETTLED;
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
            "o" => self.edit(|p| p.orient = (p.orient + 1) % 4, true),
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

    /// Space on a switch: flip it without restarting the circuit.
    fn flip_switch(&mut self) {
        let Some(i) = self.board.part_at(self.cur) else { return };
        let Some(Some(p)) = self.board.parts.get_mut(i) else { return };
        if p.kind != Kind::Switch { return; }
        p.closed = !p.closed;
        let now = p.closed;
        if let Some(d) = self.net.part_dev[i] {
            if let Dev::Switch { closed, .. } = &mut self.net.circuit.devs[d] { *closed = now; }
        }
        self.steady = false;
        self.last = Instant::now();
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

    /// The voltage the probe reads: a wire's node, or across a part.
    fn probe(&self) -> Option<f64> {
        let sim = self.sim.as_ref()?;
        if let Some(&n) = self.net.node_of.get(&self.cur) { return Some(sim.v[n]); }
        let p = self.board.part(self.board.part_at(self.cur)?)?;
        let pins = p.pins();
        let (a, b) = if p.kind == Kind::Npn { (pins[0], pins[2]) } else { (pins[0], pins[1]) };
        Some(sim.v[self.net.node_of[&a]] - sim.v[self.net.node_of[&b]])
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
        let keys = "1-6 add   w wire   p power   n challenge   ? help   q quit";
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
                match p.kind {
                    Kind::Battery if *k == 0 => ('+', 196, None, true),
                    Kind::Battery => ('−', 81, None, true),
                    Kind::Npn => (['c', 'b', 'e'][*k], wire_color(), None, true),
                    _ => (BOX[(*m | p.inward(at)) as usize & 15], wire_color(), None, false),
                }
            }
            Some(Cell::Body(i)) => {
                let Some(p) = self.board.part(*i) else { return ('?', 196, None, false) };
                let idx = (at.0 - p.x).clamp(0, 2) as usize;
                let chars: Vec<char> = self.body_text(*i, p).chars().collect();
                let ch = chars.get(idx).copied().unwrap_or(' ');
                match p.kind {
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

    fn body_text(&self, _i: usize, p: &Part) -> String {
        match p.kind {
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
            let bars: String = self.trace.iter().map(|&v| TICKS[((v / vmax).clamp(0.0, 1.0) * 7.0).round() as usize]).collect();
            l.push(format!("{} {}", style::fg(&bars, 46), style::fg(&format!("0–{} V", vmax), 245)));
        }
        if let Some(n) = &self.note {
            l.push(String::new());
            l.extend(wrap(n, w).iter().map(|s| style::fg(s, 214)));
        }
        l.push(String::new());
        l.push(style::fg("Parts", 245));
        l.push("1 battery    2 resistor   3 capacitor".into());
        l.push("4 LED        5 switch     6 transistor".into());
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
                _ => vec!["Empty. Press 1 to 6 to add a part here, or w to draw a wire.".into()],
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
        }
        out.push("o turns it, m moves it, x deletes it.".into());
        out
    }
}

const HELP: &str = "
  circuit keys

  Arrows / h j k l   move the cursor
  1 2 3 4 5 6        add a battery, resistor, capacitor,
                     LED, switch or transistor
  w                  draw a wire: move to lay it, w stops
  .                  join two wires that cross (they don't touch)
  x                  delete the wire or part under the cursor
  o                  turn a part
  m                  move a part: arrows carry it, m drops it
  + -                change a value or an LED's colour
  Space              flip a switch
  r                  replace a burnt-out LED
  p                  power on or off
  n N                next or previous challenge
  R R                put the board back to its start
  q                  quit (the board is kept)

  Wires glow green with voltage, brighter for higher,
  and red below zero. LEDs glow with their current.
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
    }
}
