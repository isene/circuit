//! The breadboard: parts and wires on a grid of cells, and the circuit
//! they make.
//!
//! A part is three cells of body in a row, with a pin cell at each end
//! (a transistor has three pins). Wires are cells too, each holding which
//! of its four sides it joins. A wire and a pin join when both point at
//! each other. Where two wires cross straight over each other they do not
//! touch, unless the cell is marked as a join.

use std::collections::HashMap;

use crate::sim::{Circuit, Dev, LedColor};

pub const N: u8 = 1;
pub const E: u8 = 2;
pub const S: u8 = 4;
pub const W: u8 = 8;
/// On a wire cell where four sides meet: the two wires are joined.
pub const JOIN: u8 = 16;

/// Two wires crossing over each other without touching.
pub fn crossing(mask: u8) -> bool {
    mask & 15 == 15 && mask & JOIN == 0
}

pub fn offset(dir: u8) -> (i32, i32) {
    match dir { N => (0, -1), E => (1, 0), S => (0, 1), _ => (-1, 0) }
}

pub fn opposite(dir: u8) -> u8 {
    match dir { N => S, E => W, S => N, _ => E }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind { Battery, Resistor, Capacitor, Led, Switch, Npn }

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Battery, Kind::Resistor, Kind::Capacitor, Kind::Led, Kind::Switch, Kind::Npn];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Battery => "battery", Kind::Resistor => "resistor", Kind::Capacitor => "capacitor",
            Kind::Led => "LED", Kind::Switch => "switch", Kind::Npn => "transistor",
        }
    }
    fn code(self) -> &'static str {
        match self {
            Kind::Battery => "battery", Kind::Resistor => "resistor", Kind::Capacitor => "capacitor",
            Kind::Led => "led", Kind::Switch => "switch", Kind::Npn => "npn",
        }
    }
    fn from_code(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.code() == s)
    }
    fn default_value(self) -> f64 {
        match self { Kind::Battery => 9.0, Kind::Resistor => 470.0, Kind::Capacitor => 10e-6, _ => 0.0 }
    }
}

#[derive(Clone, Debug)]
pub struct Part {
    pub kind: Kind,
    /// The left cell of the body.
    pub x: i32,
    pub y: i32,
    /// 0 and 1 lie flat with the first pin left or right; 2 and 3 stand
    /// up with the first pin on top or below.
    pub orient: u8,
    /// Volts, ohms or farads.
    pub value: f64,
    pub color: LedColor,
    pub closed: bool,
    pub burnt: bool,
}

impl Part {
    pub fn new(kind: Kind, x: i32, y: i32) -> Part {
        Part { kind, x, y, orient: 0, value: kind.default_value(), color: LedColor::Red, closed: false, burnt: false }
    }

    /// Pin cells, in order: a battery's + and −, an LED's anode and
    /// cathode, a transistor's collector, base and emitter.
    pub fn pins(&self) -> Vec<(i32, i32)> {
        let (x, y) = (self.x, self.y);
        let (left, right, top, bottom) = ((x - 1, y), (x + 3, y), (x + 1, y - 1), (x + 1, y + 1));
        match self.kind {
            Kind::Npn => {
                let base = if self.orient % 2 == 0 { left } else { right };
                let (c, e) = if self.orient < 2 { (top, bottom) } else { (bottom, top) };
                vec![c, base, e]
            }
            _ => match self.orient {
                0 => vec![left, right],
                1 => vec![right, left],
                2 => vec![top, bottom],
                _ => vec![bottom, top],
            },
        }
    }

    pub fn body(&self) -> [(i32, i32); 3] {
        [(self.x, self.y), (self.x + 1, self.y), (self.x + 2, self.y)]
    }

    /// The side of a pin that faces the body; wires join on the others.
    pub fn inward(&self, pin: (i32, i32)) -> u8 {
        if pin.1 < self.y { S } else if pin.1 > self.y { N } else if pin.0 < self.x { E } else { W }
    }

    fn cells(&self) -> Vec<(i32, i32)> {
        self.body().into_iter().chain(self.pins()).collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Cell {
    /// The sides it joins.
    Wire(u8),
    /// Part index.
    Body(usize),
    /// Part index, pin number, the sides where wires join it.
    Pin(usize, usize, u8),
}

#[derive(Default, Clone)]
pub struct Board {
    pub parts: Vec<Option<Part>>,
    pub cells: HashMap<(i32, i32), Cell>,
}

/// A board turned into something the solver takes.
pub struct Net {
    pub circuit: Circuit,
    /// Device number to part index, and back.
    pub dev_part: Vec<usize>,
    pub part_dev: Vec<Option<usize>>,
    /// Which node each wire and pin cell belongs to.
    pub node_of: HashMap<(i32, i32), usize>,
}

impl Board {
    pub fn part(&self, i: usize) -> Option<&Part> {
        self.parts.get(i).and_then(|p| p.as_ref())
    }

    pub fn part_at(&self, at: (i32, i32)) -> Option<usize> {
        match self.cells.get(&at) {
            Some(Cell::Body(i)) | Some(Cell::Pin(i, _, _)) => Some(*i),
            _ => None,
        }
    }

    /// Room for `p`, counting cells that part `except` holds as free.
    fn fits(&self, p: &Part, except: Option<usize>) -> bool {
        p.cells().iter().all(|c| match self.cells.get(c) {
            None => true,
            Some(Cell::Body(i)) | Some(Cell::Pin(i, _, _)) => Some(*i) == except,
            Some(Cell::Wire(_)) => false,
        })
    }

    fn put(&mut self, i: usize, p: Part) {
        for c in p.body() { self.cells.insert(c, Cell::Body(i)); }
        for (k, c) in p.pins().into_iter().enumerate() { self.cells.insert(c, Cell::Pin(i, k, 0)); }
        self.parts[i] = Some(p);
    }

    /// Place a new part. None when its cells are taken.
    pub fn place(&mut self, p: Part) -> Option<usize> {
        if !self.fits(&p, None) { return None; }
        self.parts.push(None);
        let i = self.parts.len() - 1;
        self.put(i, p);
        Some(i)
    }

    /// Change part `i` with `f` (move it, turn it) if it still fits.
    /// Wires to pins that moved come loose.
    pub fn change(&mut self, i: usize, f: impl FnOnce(&mut Part)) -> bool {
        let Some(mut new) = self.part(i).cloned() else { return false };
        f(&mut new);
        if !self.fits(&new, Some(i)) { return false; }
        self.lift(i);
        self.put(i, new);
        true
    }

    /// Take part `i` off the board, cutting the wires at its pins.
    fn lift(&mut self, i: usize) -> Option<Part> {
        let p = self.parts.get_mut(i)?.take()?;
        for c in p.body() { self.cells.remove(&c); }
        for c in p.pins() {
            if let Some(Cell::Pin(_, _, m)) = self.cells.remove(&c) { self.unhook(c, m); }
        }
        Some(p)
    }

    fn mask(&self, at: (i32, i32)) -> Option<u8> {
        match self.cells.get(&at) {
            Some(Cell::Wire(m)) | Some(Cell::Pin(_, _, m)) => Some(*m),
            _ => None,
        }
    }

    fn set_mask(&mut self, at: (i32, i32), mask: u8) {
        match self.cells.get_mut(&at) {
            Some(Cell::Wire(m)) | Some(Cell::Pin(_, _, m)) => *m = mask,
            _ => {}
        }
    }

    /// Clear the sides that neighbours join toward `at`.
    fn unhook(&mut self, at: (i32, i32), mask: u8) {
        for d in [N, E, S, W] {
            if mask & d == 0 { continue; }
            let (dx, dy) = offset(d);
            let nb = (at.0 + dx, at.1 + dy);
            if let Some(m) = self.mask(nb) { self.set_mask(nb, m & !opposite(d)); }
        }
    }

    /// Delete the wire or part at `at`.
    pub fn delete(&mut self, at: (i32, i32)) -> bool {
        match self.cells.get(&at).copied() {
            Some(Cell::Wire(m)) => {
                self.cells.remove(&at);
                self.unhook(at, m);
                true
            }
            Some(Cell::Body(i)) | Some(Cell::Pin(i, _, _)) => self.lift(i).is_some(),
            None => false,
        }
    }

    /// Can a wire leave `at` toward `dir`?
    fn open(&self, at: (i32, i32), dir: u8) -> bool {
        match self.cells.get(&at) {
            None | Some(Cell::Wire(_)) => true,
            Some(Cell::Body(_)) => false,
            Some(Cell::Pin(i, _, _)) => self.part(*i).is_some_and(|p| p.inward(at) != dir),
        }
    }

    /// Lay a wire from `at` one cell toward `dir`, joining what is there.
    pub fn wire(&mut self, at: (i32, i32), dir: u8) -> bool {
        let (dx, dy) = offset(dir);
        let to = (at.0 + dx, at.1 + dy);
        if !self.open(at, dir) || !self.open(to, opposite(dir)) { return false; }
        for (c, d) in [(at, dir), (to, opposite(dir))] {
            let m = self.mask(c).unwrap_or(0);
            self.cells.entry(c).or_insert(Cell::Wire(0));
            self.set_mask(c, m | d);
        }
        true
    }

    /// Mark or unmark the four-way wire cell at `at` as a join.
    pub fn toggle_join(&mut self, at: (i32, i32)) -> bool {
        match self.cells.get_mut(&at) {
            Some(Cell::Wire(m)) if *m & 15 == 15 => { *m ^= JOIN; true }
            _ => false,
        }
    }

    /// The circuit this board makes. Joined wires and pins share a node;
    /// the first battery's − side is ground.
    pub fn net(&self) -> Net {
        let keys: Vec<(i32, i32)> = self.cells.iter()
            .filter(|(_, c)| !matches!(c, Cell::Body(_)))
            .map(|(k, _)| *k)
            .collect();
        let index: HashMap<(i32, i32), usize> = keys.iter().enumerate().map(|(i, k)| (*k, i)).collect();
        // Every cell is point 2i; a crossing also has point 2i+1 for its
        // east-west wire, so the two wires stay apart.
        let port = |i: usize, at: (i32, i32), dir: u8| {
            if crossing(self.mask(at).unwrap_or(0)) && (dir == E || dir == W) { 2 * i + 1 } else { 2 * i }
        };
        let mut parent: Vec<usize> = (0..2 * keys.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i { parent[i] = parent[parent[i]]; i = parent[i]; }
            i
        }
        for (i, &at) in keys.iter().enumerate() {
            let m = self.mask(at).unwrap_or(0);
            for d in [E, S] {
                if m & d == 0 { continue; }
                let (dx, dy) = offset(d);
                let nb = (at.0 + dx, at.1 + dy);
                if let (Some(&j), Some(nm)) = (index.get(&nb), self.mask(nb)) {
                    if nm & opposite(d) != 0 {
                        let (a, b) = (root(&mut parent, port(i, at, d)), root(&mut parent, port(j, nb, opposite(d))));
                        parent[a] = b;
                    }
                }
            }
        }
        let mut number: HashMap<usize, usize> = HashMap::new();
        let mut node_of = HashMap::new();
        for (i, &at) in keys.iter().enumerate() {
            let r = root(&mut parent, 2 * i);
            let len = number.len();
            let n = *number.entry(r).or_insert(len);
            node_of.insert(at, n);
            if crossing(self.mask(at).unwrap_or(0)) {
                let r = root(&mut parent, 2 * i + 1);
                let len = number.len();
                number.entry(r).or_insert(len);
            }
        }
        let mut devs = Vec::new();
        let mut dev_part = Vec::new();
        let mut part_dev = vec![None; self.parts.len()];
        let mut ground = None;
        for (i, p) in self.parts.iter().enumerate() {
            let Some(p) = p else { continue };
            let pin: Vec<usize> = p.pins().iter().map(|c| node_of[c]).collect();
            // Real parts are never exactly their marked value. The spread
            // also lets a symmetric blinker pick a side and start.
            let tol = 1.0 + tolerance(i);
            let dev = match p.kind {
                Kind::Battery => {
                    ground.get_or_insert(pin[1]);
                    Dev::Battery { p: pin[0], n: pin[1], volts: p.value }
                }
                Kind::Resistor => Dev::Resistor { a: pin[0], b: pin[1], ohms: p.value * tol },
                Kind::Capacitor => Dev::Capacitor { a: pin[0], b: pin[1], farads: p.value * tol },
                Kind::Led => Dev::Led { a: pin[0], k: pin[1], color: p.color, burnt: p.burnt },
                Kind::Switch => Dev::Switch { a: pin[0], b: pin[1], closed: p.closed },
                Kind::Npn => Dev::Npn { c: pin[0], b: pin[1], e: pin[2] },
            };
            part_dev[i] = Some(devs.len());
            dev_part.push(i);
            devs.push(dev);
        }
        Net {
            circuit: Circuit { nodes: number.len().max(1), ground: ground.unwrap_or(0), devs },
            dev_part,
            part_dev,
            node_of,
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for p in self.parts.iter().flatten() {
            out.push_str(&format!("part {} {} {} {} {} {} {} {}\n", p.kind.code(), p.x, p.y, p.orient, p.value,
                color_code(p.color), p.closed as u8, p.burnt as u8));
        }
        let mut cells: Vec<_> = self.cells.iter().collect();
        cells.sort_by_key(|(k, _)| (k.1, k.0));
        for (&(x, y), c) in cells {
            match c {
                Cell::Wire(m) => out.push_str(&format!("wire {x} {y} {m}\n")),
                Cell::Pin(_, _, m) if *m != 0 => out.push_str(&format!("pin {x} {y} {m}\n")),
                _ => {}
            }
        }
        out
    }

    pub fn from_text(text: &str) -> Board {
        let mut b = Board::default();
        let rows: Vec<Vec<&str>> = text.lines().map(|l| l.split_whitespace().collect()).collect();
        for f in rows.iter().filter(|f| f.first() == Some(&"part") && f.len() >= 9) {
            let Some(kind) = Kind::from_code(f[1]) else { continue };
            let (Ok(x), Ok(y), Ok(orient), Ok(value)) = (f[2].parse(), f[3].parse(), f[4].parse(), f[5].parse()) else { continue };
            b.place(Part { kind, x, y, orient, value, color: color_from(f[6]), closed: f[7] == "1", burnt: f[8] == "1" });
        }
        for f in rows.iter().filter(|f| f.len() == 4) {
            let (Ok(x), Ok(y), Ok(m)) = (f[1].parse::<i32>(), f[2].parse::<i32>(), f[3].parse::<u8>()) else { continue };
            match f[0] {
                "wire" if !b.cells.contains_key(&(x, y)) => { b.cells.insert((x, y), Cell::Wire(m)); }
                "pin" => b.set_mask((x, y), m),
                _ => {}
            }
        }
        b
    }
}

/// A steady spread of up to ±1% per part.
fn tolerance(i: usize) -> f64 {
    let h = ((i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) % 2001;
    h as f64 / 100_000.0 - 0.01
}

fn color_code(c: LedColor) -> &'static str {
    match c { LedColor::Red => "red", LedColor::Green => "green", LedColor::Yellow => "yellow", LedColor::Blue => "blue" }
}

fn color_from(s: &str) -> LedColor {
    match s { "green" => LedColor::Green, "yellow" => LedColor::Yellow, "blue" => LedColor::Blue, _ => LedColor::Red }
}

pub fn next_color(c: LedColor, up: bool) -> LedColor {
    let all = [LedColor::Red, LedColor::Green, LedColor::Yellow, LedColor::Blue];
    let i = all.iter().position(|&x| x == c).unwrap_or(0);
    all[(i + if up { 1 } else { 3 }) % 4]
}

const E12: [f64; 12] = [1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];

/// The next stocked value up or down: batteries, the E12 resistor range
/// from 10 Ω to 1 MΩ, capacitors from 1 µF to 1000 µF.
pub fn step_value(kind: Kind, value: f64, up: bool) -> f64 {
    let list: Vec<f64> = match kind {
        Kind::Battery => vec![1.5, 3.0, 4.5, 6.0, 9.0, 12.0],
        Kind::Resistor => (1..6).flat_map(|d| E12.iter().map(move |v| v * 10f64.powi(d))).chain([1e6]).collect(),
        Kind::Capacitor => [1.0, 2.2, 4.7, 10.0, 22.0, 47.0, 100.0, 220.0, 470.0, 1000.0].iter().map(|v| v * 1e-6).collect(),
        _ => return value,
    };
    let at = (0..list.len()).min_by(|&a, &b| (list[a] / value).ln().abs().total_cmp(&(list[b] / value).ln().abs())).unwrap_or(0);
    let next = if up { (at + 1).min(list.len() - 1) } else { at.saturating_sub(1) };
    list[next]
}

/// A value in three characters, the unit letter standing in for the
/// decimal point, as printed on parts: 4k7, 10k, 1V5, 4µ7.
pub fn code(kind: Kind, v: f64) -> String {
    fn ee(v: f64, unit: char) -> String {
        if v >= 10.0 { return format!("{:.0}{unit}", v); }
        let tenths = (v * 10.0).round() as u32;
        format!("{}{unit}{}", tenths / 10, tenths % 10)
    }
    match kind {
        Kind::Battery => ee(v, 'V'),
        Kind::Resistor if v < 100.0 => ee(v, 'R'),
        Kind::Resistor if v < 1e3 => format!("{:.0}", v),
        Kind::Resistor if v < 1e5 => ee(v / 1e3, 'k'),
        Kind::Resistor if v < 1e6 => format!("M{:.0}", v / 1e4),
        Kind::Resistor => ee(v / 1e6, 'M'),
        Kind::Capacitor if v < 100e-6 => ee(v * 1e6, 'µ'),
        Kind::Capacitor if v < 1e-3 => format!("m{:.0}", v * 1e5),
        Kind::Capacitor => ee(v * 1e3, 'm'),
        _ => String::new(),
    }
}

/// A value for reading: "4.7 kΩ", "10 µF", "9 V".
pub fn pretty(kind: Kind, v: f64) -> String {
    fn num(x: f64) -> String {
        let s = format!("{:.2}", x);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
    match kind {
        Kind::Battery => format!("{} V", num(v)),
        Kind::Resistor if v < 1e3 => format!("{} Ω", num(v)),
        Kind::Resistor if v < 1e6 => format!("{} kΩ", num(v / 1e3)),
        Kind::Resistor => format!("{} MΩ", num(v / 1e6)),
        Kind::Capacitor => format!("{} µF", num(v * 1e6)),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_print_the_way_parts_are_marked() {
        assert_eq!(code(Kind::Resistor, 470.0), "470");
        assert_eq!(code(Kind::Resistor, 4700.0), "4k7");
        assert_eq!(code(Kind::Resistor, 47_000.0), "47k");
        assert_eq!(code(Kind::Resistor, 470_000.0), "M47");
        assert_eq!(code(Kind::Resistor, 1e6), "1M0");
        assert_eq!(code(Kind::Resistor, 82.0), "82R");
        assert_eq!(code(Kind::Capacitor, 4.7e-6), "4µ7");
        assert_eq!(code(Kind::Capacitor, 220e-6), "m22");
        assert_eq!(code(Kind::Battery, 9.0), "9V0");
        assert_eq!(code(Kind::Battery, 12.0), "12V");
        assert_eq!(pretty(Kind::Resistor, 4700.0), "4.7 kΩ");
        assert_eq!(step_value(Kind::Resistor, 470.0, true), 560.0);
        assert!((step_value(Kind::Capacitor, 10e-6, false) - 4.7e-6).abs() < 1e-12);
    }

    #[test]
    fn wires_join_pins_into_nodes_and_survive_a_save() {
        let mut b = Board::default();
        let mut bat = Part::new(Kind::Battery, 0, 2);
        bat.orient = 2; // + at (1,1), − at (1,3)
        b.place(bat).unwrap();
        let r = b.place(Part::new(Kind::Resistor, 6, 1)).unwrap(); // pins (5,1), (9,1)
        // + to the resistor's left pin: (1,1) → (2,1) … (5,1).
        let mut at = (1, 1);
        for _ in 0..4 { assert!(b.wire(at, E)); at.0 += 1; }
        let net = b.net();
        let res = &b.part(r).unwrap().pins();
        assert_eq!(net.node_of[&(1, 1)], net.node_of[&res[0]]);
        assert_ne!(net.node_of[&res[0]], net.node_of[&res[1]]);
        // A wire cannot enter a pin from the side that faces the body.
        assert!(!b.wire((9, 1), W));
        let back = Board::from_text(&b.to_text());
        let net2 = back.net();
        assert_eq!(net2.node_of[&(1, 1)], net2.node_of[&(5, 1)]);
        assert_eq!(back.parts.iter().flatten().count(), 2);
    }
}
