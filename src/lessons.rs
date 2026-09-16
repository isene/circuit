//! The challenges: what each one teaches, the parts it starts with, and
//! how it knows the circuit works.

use crate::board::{Board, Kind, Part};
use crate::sim::{Circuit, Dev, Gate, LedColor, Sim};

pub struct Lesson {
    pub title: &'static str,
    pub text: &'static str,
    pub goal: &'static str,
}

pub const LESSONS: [Lesson; 8] = [
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
    Lesson {
        title: "Logic gates",
        text: "A logic chip reads each input as high, near +, or low, near −. An AND gate's output q is high only \
when a and b are both high. Chips take their power from the battery. \
Wire each switch from + to one input, with a 10 kΩ resistor from that input to − so it reads low while the switch \
is open. Then q through 470 Ω and the LED to −. Power on and try all four switch settings.",
        goal: "the LED follows the AND gate for all four switch settings.",
    },
    Lesson {
        title: "Count in binary",
        text: "A counter adds one each time its > input goes from low to high, and a high r puts it back to 0. \
Its outputs 1, 2, 4 and 8 show the count in binary: 13 is 8 + 4 + 1. \
Wire the clock's q to >, r to −, and each output through 470 Ω to an LED and on to −.",
        goal: "the LEDs count all the way to 15, all four lit, and start over.",
    },
    Lesson {
        title: "Numbers on a display",
        text: "Four wires carry any number from 0 to 15. A display chip turns them back into one digit, carrying \
on past 9 with A, b, C, d, E and F. \
Wire the clock to the counter's >, r to −, and the counter's 1, 2, 4 and 8 to the display's 1, 2, 4 and 8.",
        goal: "the display shows all sixteen digits, 0 to F.",
    },
    Lesson {
        title: "The 555 timer",
        text: "The 555 may be the most-made chip of all. Here a capacitor charges through 10 kΩ and 68 kΩ until h \
sees two thirds of the battery. Then d empties it through the 68 kΩ until t sees one third, and it starts over. \
A round takes about 0.7 × (10 kΩ + 2 × 68 kΩ) × 10 µF, one second. \
Wire + to 10 kΩ to d, d to 68 kΩ to h, h to t, and t to the capacitor and on to −. \
Then o through 470 Ω and the LED to −.",
        goal: "the 555 blinks the LED, each half a tenth of a second to 3 seconds.",
    },
    Lesson {
        title: "A stopwatch",
        text: "A counter runs from 0 to 15, but a stopwatch digit has to go from 9 back to 0. At 10, outputs 2 and \
8 are high together for the first time. An AND gate on those two outputs can reset the counter the moment it \
gets there, so it only ever shows 0 to 9. That same pulse is the carry into the next digit. \
The clock at 10 Hz drives the first counter, and its display shows tenths. The AND gate feeds the first \
counter's r and the second counter's >, and the second display shows seconds.",
        goal: "one display counts tenths from 0 to 9 and the other reaches 3 seconds.",
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
        4 => {
            put(Kind::Battery, 4, 12, &stand);
            put(Kind::Switch, 12, 4, &none);
            put(Kind::Switch, 12, 8, &none);
            put(Kind::Resistor, 19, 11, &|p| { p.orient = 2; p.value = 10_000.0; });
            put(Kind::Resistor, 25, 11, &|p| { p.orient = 2; p.value = 10_000.0; });
            put(Kind::And, 32, 6, &none);
            put(Kind::Resistor, 40, 6, &none);
            put(Kind::Led, 48, 6, &none);
        }
        5 => {
            put(Kind::Battery, 4, 14, &stand);
            put(Kind::Clock, 10, 4, &none);
            put(Kind::Counter, 22, 4, &none);
            for y in [4, 6, 8, 10] {
                put(Kind::Resistor, 32, y, &none);
                put(Kind::Led, 40, y, &|p| p.color = LedColor::Green);
            }
        }
        6 => {
            put(Kind::Battery, 4, 14, &stand);
            put(Kind::Clock, 10, 4, &|p| p.value = 2.0);
            put(Kind::Counter, 22, 4, &none);
            put(Kind::Display, 36, 4, &none);
        }
        7 => {
            put(Kind::Battery, 4, 14, &stand);
            put(Kind::Resistor, 12, 3, &|p| p.value = 10_000.0);
            put(Kind::Resistor, 12, 7, &|p| p.value = 68_000.0);
            put(Kind::Capacitor, 12, 11, &none);
            put(Kind::Timer, 24, 6, &none);
            put(Kind::Resistor, 34, 6, &none);
            put(Kind::Led, 42, 6, &none);
        }
        8 => {
            put(Kind::Battery, 4, 20, &stand);
            put(Kind::Clock, 8, 4, &|p| p.value = 10.0);
            put(Kind::Counter, 20, 4, &none);
            put(Kind::Display, 34, 4, &none);
            put(Kind::And, 22, 12, &none);
            put(Kind::Counter, 34, 12, &none);
            put(Kind::Display, 46, 12, &none);
        }
        _ => {}
    }
    b
}

/// What a challenge has seen so far while the power was on. Checked
/// after every solver step.
#[derive(Default)]
pub struct Progress {
    /// Challenge 2: the LED that lit through a transistor.
    lit: Option<usize>,
    /// Blinking, per LED: on now, good flips in a row, time of the last flip.
    flips: Vec<(bool, u32, f64)>,
    /// Per device: which gate input settings or digits have been seen.
    seen: Vec<u32>,
    /// Per device: a gate's input setting and since when it has held.
    held: Vec<(u8, f64)>,
    /// Per device: how many steps in a row a display has shown 10 or more.
    run: Vec<u32>,
    /// Per device: a counter reached 15 with the LEDs lit; a display sat past 9.
    flag: Vec<bool>,
}

const ON: f64 = 0.005;
const OFF: f64 = 0.001;

impl Progress {
    pub fn reset(&mut self, devices: usize) {
        *self = Progress {
            lit: None,
            flips: vec![(false, 0, 0.0); devices],
            seen: vec![0; devices],
            held: vec![(255, 0.0); devices],
            run: vec![0; devices],
            flag: vec![false; devices],
        };
    }

    /// Any LED blinking steadily: six flips in a row, each half 0.1 to 3 s.
    fn blinks(&mut self, c: &Circuit, s: &Sim) -> bool {
        let mut done = false;
        for i in 0..c.devs.len() {
            if !matches!(c.devs[i], Dev::Led { .. }) || s.burnt[i] { continue; }
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

    /// True once challenge `n` is met.
    pub fn check(&mut self, n: usize, c: &Circuit, s: &Sim) -> bool {
        if self.flips.len() != c.devs.len() { self.reset(c.devs.len()); }
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
            3 => self.blinks(c, s),
            4 => {
                let lit = leds().any(|i| s.current[i] > ON);
                let dark = leds().all(|i| s.current[i] < OFF);
                let mut done = false;
                for (i, d) in c.devs.iter().enumerate() {
                    if !matches!(d, Dev::Gate { gate: Gate::And, .. }) { continue; }
                    let l = s.logic[i];
                    let setting = l.ins[0] as u8 | (l.ins[1] as u8) << 1;
                    if self.held[i].0 != setting { self.held[i] = (setting, s.t); continue; }
                    let follows = if l.out[0] { lit } else { dark };
                    if follows && s.t - self.held[i].1 >= 0.05 { self.seen[i] |= 1 << setting; }
                    done |= self.seen[i] == 0b1111;
                }
                done
            }
            5 => {
                let lit = leds().filter(|&i| s.current[i] > ON).count();
                for (i, d) in c.devs.iter().enumerate() {
                    if !matches!(d, Dev::Counter { .. }) { continue; }
                    let v = s.logic[i].value;
                    if v == 15 && lit >= 4 { self.flag[i] = true; }
                    if self.flag[i] && v == 0 { return true; }
                }
                false
            }
            6 => c.devs.iter().enumerate().any(|(i, d)| {
                if !matches!(d, Dev::Display { .. }) || c.vhigh.is_none() { return false; }
                self.seen[i] |= 1 << s.logic[i].value;
                self.seen[i] == 0xFFFF
            }),
            7 => c.devs.iter().any(|d| matches!(d, Dev::Timer { .. })) && self.blinks(c, s),
            8 => {
                let displays: Vec<usize> = (0..c.devs.len()).filter(|&i| matches!(c.devs[i], Dev::Display { .. })).collect();
                for &i in &displays {
                    let v = s.logic[i].value;
                    self.seen[i] |= 1 << v;
                    // A counter that resets at 10 shows it for a step or two, never longer.
                    if v >= 10 { self.run[i] += 1; if self.run[i] > 20 { self.flag[i] = true; } } else { self.run[i] = 0; }
                }
                displays.iter().any(|&a| {
                    self.seen[a] & 0x3FF == 0x3FF && !self.flag[a]
                        && displays.iter().any(|&b| b != a && s.logic[b].value >= 3)
                })
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

    /// A two-digit stopwatch: tenths on the first display, seconds on the second.
    fn stopwatch() -> Board {
        let mut b = rails();
        part(&mut b, Kind::Clock, 6, 4, 0, |p| p.value = 10.0); // q (10,4)
        part(&mut b, Kind::Counter, 14, 4, 0, |_| {}); // > (13,4), r (13,5), 1 2 4 8 at (19,4..7)
        part(&mut b, Kind::Display, 26, 4, 0, |_| {}); // inputs (25,4..7)
        part(&mut b, Kind::And, 16, 12, 0, |_| {}); // a (17,11), b (17,13), q (20,12)
        part(&mut b, Kind::Counter, 34, 14, 0, |_| {}); // > (33,14), r (33,15), outputs (39,14..17)
        part(&mut b, Kind::Display, 46, 14, 0, |_| {}); // inputs (45,14..17)
        path(&mut b, (10, 4), "RRR");
        for y in 4..8 { path(&mut b, (19, y), "RRRRRR"); }
        // Output 2 to a, crossing the 4 and 8 wires.
        path(&mut b, (21, 5), &format!("{}{}", "D".repeat(6), "L".repeat(4)));
        // Output 8 to b, from below.
        path(&mut b, (23, 7), &format!("{}{}U", "D".repeat(7), "L".repeat(6)));
        // q down and across to the second counter's clock.
        path(&mut b, (20, 12), &format!("RR{}{}UUUR", "D".repeat(5), "R".repeat(10)));
        // The same pulse back to the first counter's reset.
        path(&mut b, (22, 16), &format!("{}{}R", "L".repeat(10), "U".repeat(11)));
        // Second counter's reset to −.
        path(&mut b, (33, 15), &"D".repeat(15));
        for y in 14..18 { path(&mut b, (39, y), "RRRRRR"); }
        b
    }

    #[test]
    fn the_stopwatch_challenge_is_met_on_a_wired_board() {
        let b = stopwatch();
        let net = b.net();
        let mut sim = Sim::new(&net.circuit);
        let mut progress = Progress::default();
        progress.reset(net.circuit.devs.len());
        let mut done = false;
        for _ in 0..5000 {
            sim.step(&net.circuit, 1e-3);
            if progress.check(8, &net.circuit, &sim) { done = true; break; }
        }
        assert!(done, "no stopwatch after {:.1} s", sim.t);
    }

    #[test]
    fn the_display_challenge_is_met_on_a_wired_board() {
        let mut b = rails();
        part(&mut b, Kind::Clock, 10, 4, 0, |p| p.value = 20.0); // q at (14,4)
        part(&mut b, Kind::Counter, 20, 4, 0, |_| {}); // > (19,4), r (19,5), 1 2 4 8 at (25,4..7)
        part(&mut b, Kind::Display, 32, 4, 0, |_| {}); // 1 2 4 8 at (31,4..7)
        path(&mut b, (14, 4), "RRRRR");
        path(&mut b, (19, 5), &format!("L{}", "D".repeat(25)));
        for y in 4..8 { path(&mut b, (25, y), "RRRRRR"); }
        let net = b.net();
        let mut sim = Sim::new(&net.circuit);
        let mut progress = Progress::default();
        progress.reset(net.circuit.devs.len());
        let mut done = false;
        for _ in 0..3000 {
            sim.step(&net.circuit, 1e-3);
            if progress.check(6, &net.circuit, &sim) { done = true; break; }
        }
        assert!(done, "the display did not show all sixteen digits in {:.1} s", sim.t);
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
