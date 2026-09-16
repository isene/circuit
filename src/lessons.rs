//! The challenges: what each one teaches, the parts it starts with, and
//! how it knows the circuit works.

use crate::board::{Board, Kind, Part};
use crate::sim::{Circuit, Dev, LedColor, Sim};

pub struct Lesson {
    pub title: &'static str,
    pub text: &'static str,
    pub goal: &'static str,
}

pub const LESSONS: [Lesson; 3] = [
    Lesson {
        title: "Light an LED",
        text: "An LED lights when current flows from its arrow side (the anode) to its bar side (the cathode). \
It wants about 2 volts and 5 to 20 milliamps. A 9 volt battery pushes far more, so a resistor in the same loop \
holds the current down: (9 − 2) V ÷ 0.015 A is about 470 Ω. \
Wire + to the resistor, the resistor to the arrow side, and the bar side back to −. Press p to power on. \
Then try it without the resistor.",
        goal: "the LED glows with 5 to 25 mA.",
    },
    Lesson {
        title: "A transistor as a switch",
        text: "A transistor lets a small current steer a big one. A little current into the base (b) opens the path \
from the collector (c) to the emitter (e). \
Put the LED and the 470 Ω resistor between + and the collector, and the emitter on −. \
Then feed the base from + through the switch and the 10 kΩ resistor. Power on and press Space on the switch.",
        goal: "the switch turns the LED on through the transistor, carrying under a tenth of the LED's current, and off again.",
    },
    Lesson {
        title: "Make it blink",
        text: "Two transistors can take turns. Each collector reaches the other transistor's base through a capacitor. \
When one turns on, its capacitor holds the other one off until the capacitor charges back up through a 47 kΩ \
resistor. Then they swap. That takes about 0.7 × 47 kΩ × 10 µF, a third of a second. \
For each side: LED and 470 Ω from + to a collector, 47 kΩ from + to a base, emitter to −. \
Then a capacitor from each collector to the other transistor's base.",
        goal: "an LED blinks on and off three times, each half a tenth of a second to 3 seconds.",
    },
];

/// The parts a challenge starts with, laid out and waiting for wires.
pub fn starter(n: usize) -> Board {
    let mut b = Board::default();
    let mut put = |kind: Kind, x: i32, y: i32, f: &dyn Fn(&mut Part)| {
        let mut p = Part::new(kind, x, y);
        f(&mut p);
        b.place(p);
    };
    let stand = |p: &mut Part| p.orient = 2;
    let none = |_: &mut Part| {};
    match n {
        1 => {
            put(Kind::Battery, 4, 8, &stand);
            put(Kind::Resistor, 14, 4, &none);
            put(Kind::Led, 26, 8, &none);
        }
        2 => {
            put(Kind::Battery, 4, 10, &stand);
            put(Kind::Switch, 10, 4, &none);
            put(Kind::Resistor, 20, 4, &|p| p.value = 10_000.0);
            put(Kind::Resistor, 30, 4, &none);
            put(Kind::Led, 40, 4, &none);
            put(Kind::Npn, 24, 10, &none);
        }
        3 => {
            put(Kind::Led, 8, 3, &none);
            put(Kind::Led, 20, 3, &|p| p.color = LedColor::Green);
            put(Kind::Resistor, 32, 3, &none);
            put(Kind::Resistor, 44, 3, &none);
            put(Kind::Resistor, 8, 8, &|p| p.value = 47_000.0);
            put(Kind::Resistor, 20, 8, &|p| p.value = 47_000.0);
            put(Kind::Capacitor, 32, 8, &none);
            put(Kind::Capacitor, 44, 8, &none);
            put(Kind::Battery, 4, 14, &stand);
            put(Kind::Npn, 18, 14, &none);
            put(Kind::Npn, 34, 14, &none);
        }
        _ => {}
    }
    b
}

/// What a challenge has seen so far while the power was on.
#[derive(Default)]
pub struct Progress {
    /// Challenge 2: the LED that lit through a transistor.
    lit: Option<usize>,
    /// Challenge 3, per LED: on now, good flips in a row, time of the last flip.
    flips: Vec<(bool, u32, f64)>,
}

const ON: f64 = 0.005;
const OFF: f64 = 0.001;

impl Progress {
    pub fn reset(&mut self, devices: usize) {
        self.lit = None;
        self.flips = vec![(false, 0, 0.0); devices];
    }

    /// True once challenge `n` is met.
    pub fn check(&mut self, n: usize, c: &Circuit, s: &Sim) -> bool {
        let leds = || c.devs.iter().enumerate().filter(|(i, d)| matches!(d, Dev::Led { .. }) && !s.burnt[*i]).map(|(i, _)| i);
        match n {
            1 => leds().any(|i| (ON..=0.025).contains(&s.current[i])),
            2 => {
                let switches: Vec<(bool, f64)> = c.devs.iter().enumerate()
                    .filter_map(|(i, d)| match d { Dev::Switch { closed, .. } => Some((*closed, s.current[i].abs())), _ => None })
                    .collect();
                let transistor_on = c.devs.iter().enumerate().any(|(i, d)| matches!(d, Dev::Npn { .. }) && s.current[i] > ON * 0.8);
                if let Some(led) = self.lit {
                    return s.current[led] < OFF / 2.0 && switches.iter().any(|(closed, _)| !closed);
                }
                if let Some(led) = leds().find(|&i| s.current[i] > ON) {
                    let closed: Vec<f64> = switches.iter().filter(|(c, _)| *c).map(|(_, i)| *i).collect();
                    if transistor_on && !closed.is_empty() && closed.iter().all(|&i| i < s.current[led] / 10.0) {
                        self.lit = Some(led);
                    }
                }
                false
            }
            3 => {
                if self.flips.len() != c.devs.len() { self.reset(c.devs.len()); }
                let mut done = false;
                for i in leds() {
                    let (on, good, last) = self.flips[i];
                    let now = if on { s.current[i] > OFF } else { s.current[i] > ON };
                    if now == on { continue; }
                    let span = s.t - last;
                    let good = if last > 0.0 && (0.1..=3.0).contains(&span) { good + 1 } else if last > 0.0 { 0 } else { good };
                    self.flips[i] = (now, good, s.t);
                    done |= good >= 6;
                }
                done
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{offset, E, N, S, W};

    /// Lay wire from `at` along `moves` (U, D, L, R), each step joining.
    fn path(b: &mut Board, mut at: (i32, i32), moves: &str) {
        for c in moves.chars() {
            let dir = match c { 'U' => N, 'D' => S, 'L' => W, _ => E };
            assert!(b.wire(at, dir), "wire from {at:?} toward {c}");
            let (dx, dy) = offset(dir);
            at = (at.0 + dx, at.1 + dy);
        }
    }

    fn part(b: &mut Board, kind: Kind, x: i32, y: i32, orient: u8, f: impl Fn(&mut Part)) {
        let mut p = Part::new(kind, x, y);
        p.orient = orient;
        f(&mut p);
        b.place(p).expect("room for the part");
    }

    /// A 9 V battery with + along row 0 and − along row 30.
    fn rails() -> Board {
        let mut b = Board::default();
        part(&mut b, Kind::Battery, 1, 15, 2, |_| {});
        path(&mut b, (2, 14), &format!("{}{}", "U".repeat(14), "R".repeat(42)));
        path(&mut b, (2, 16), &format!("{}{}", "D".repeat(14), "R".repeat(42)));
        b
    }

    #[test]
    fn the_switch_challenge_is_met_on_a_wired_board() {
        let mut b = rails();
        part(&mut b, Kind::Switch, 19, 3, 2, |p| p.closed = true);
        path(&mut b, (20, 0), "DD");
        part(&mut b, Kind::Resistor, 19, 6, 2, |p| p.value = 10_000.0);
        path(&mut b, (20, 4), "D");
        part(&mut b, Kind::Npn, 23, 10, 0, |_| {});
        path(&mut b, (20, 7), "DDDRR");
        part(&mut b, Kind::Led, 23, 2, 2, |_| {});
        path(&mut b, (24, 0), "D");
        part(&mut b, Kind::Resistor, 23, 5, 2, |_| {});
        path(&mut b, (24, 3), "D");
        path(&mut b, (24, 6), "DDD");
        path(&mut b, (24, 11), &"D".repeat(19));
        let mut net = b.net();
        let mut sim = Sim::new(&net.circuit);
        let mut progress = Progress::default();
        progress.reset(net.circuit.devs.len());
        let mut done = false;
        for _ in 0..100 { sim.step(&net.circuit, 1e-3); done |= progress.check(2, &net.circuit, &sim); }
        assert!(!done, "not yet: the LED has not gone dark");
        for d in net.circuit.devs.iter_mut() {
            if let Dev::Switch { closed, .. } = d { *closed = false; }
        }
        for _ in 0..100 { sim.step(&net.circuit, 1e-3); done |= progress.check(2, &net.circuit, &sim); }
        assert!(done);
    }

    #[test]
    fn the_blinker_challenge_is_met_on_a_wired_board_with_crossing_wires() {
        let mut b = rails();
        for (x, base) in [(9, 1u8), (39, 0u8)] {
            let col = x + 1;
            part(&mut b, Kind::Led, x, 2, 2, |_| {});
            path(&mut b, (col, 0), "D");
            part(&mut b, Kind::Resistor, x, 5, 2, |_| {});
            path(&mut b, (col, 3), "D");
            part(&mut b, Kind::Npn, x, 12, base, |_| {});
            path(&mut b, (col, 6), "DDDDD");
            path(&mut b, (col, 13), &"D".repeat(17));
        }
        // 47 kΩ from + to each base.
        part(&mut b, Kind::Resistor, 14, 5, 2, |p| p.value = 47_000.0);
        path(&mut b, (15, 0), "DDDD");
        path(&mut b, (15, 6), "DDDDDDLLL");
        part(&mut b, Kind::Resistor, 34, 5, 2, |p| p.value = 47_000.0);
        path(&mut b, (35, 0), "DDDD");
        path(&mut b, (35, 6), "DDDDDDRRR");
        // Collector 1 to base 2, crossing the first 47 kΩ wire.
        part(&mut b, Kind::Capacitor, 22, 8, 0, |_| {});
        path(&mut b, (10, 8), &"R".repeat(11));
        path(&mut b, (25, 8), &"R".repeat(10));
        // Collector 2 to base 1, crossing the second 47 kΩ wire.
        part(&mut b, Kind::Capacitor, 26, 10, 0, |_| {});
        path(&mut b, (40, 10), &"L".repeat(11));
        path(&mut b, (25, 10), &"L".repeat(10));
        let net = b.net();
        assert_ne!(net.node_of[&(15, 7)], net.node_of[&(12, 8)], "the crossing does not join");
        let mut sim = Sim::new(&net.circuit);
        let mut progress = Progress::default();
        progress.reset(net.circuit.devs.len());
        let mut done = false;
        for _ in 0..8000 {
            sim.step(&net.circuit, 1e-3);
            if progress.check(3, &net.circuit, &sim) { done = true; break; }
        }
        assert!(done, "no blink after {:.1} s", sim.t);
    }
}
