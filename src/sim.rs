//! The circuit solver.
//!
//! Every node's voltage is unknown. The currents flowing out of a node
//! must add up to zero, which gives one equation per node, and all of
//! them are solved together as one matrix (modified nodal analysis).
//! LEDs and transistors bend the equations, so each time step repeats the
//! solve until the voltages stop moving (Newton's method). A capacitor
//! remembers its voltage from the step before.
//!
//! Logic chips sit in the same circuit. Each has a + and a − pin and does
//! nothing until they are at least 3 volts apart. A high output joins its
//! wire to the chip's + through a small resistance, a low one to its −, so
//! an output's current comes from the battery, and an LED on an output
//! still needs its resistor. An input reads its wire against the chip's
//! own − and +. After each step the chips look at their inputs and set
//! their outputs for the next step, so every gate takes a millisecond.

/// The thermal voltage at room temperature, in volts.
const VT: f64 = 0.025852;
/// A tiny conductance from every node to ground, so a part left
/// unconnected still has a voltage.
const GMIN: f64 = 1e-9;
/// A battery's own resistance, in ohms.
pub const BATTERY_R: f64 = 1.0;
/// A closed switch, in ohms.
const SWITCH_ON_R: f64 = 1e-3;
/// Above this an LED starts to burn; held for `BURN_TIME` it is gone.
pub const LED_MAX: f64 = 0.05;
const BURN_TIME: f64 = 0.02;
/// The current at which an LED is at full brightness.
pub const LED_FULL: f64 = 0.02;
const LED_N: f64 = 2.0;
const NPN_IS: f64 = 1e-14;
const NPN_BF: f64 = 150.0;
const NPN_BR: f64 = 1.0;
/// A logic output's own resistance, in ohms.
const OUT_R: f64 = 100.0;
/// A 555's discharge pin when it pulls down, in ohms.
const DIS_R: f64 = 20.0;
/// A chip wakes up once its + is this many volts above its −.
const MIN_SUPPLY: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gate { Not, And, Or, Nand, Nor, Xor }

impl Gate {
    pub fn eval(self, a: bool, b: bool) -> bool {
        match self {
            Gate::Not => !a,
            Gate::And => a && b,
            Gate::Or => a || b,
            Gate::Nand => !(a && b),
            Gate::Nor => !(a || b),
            Gate::Xor => a != b,
        }
    }
}

/// A logic part's state between steps.
#[derive(Clone, Copy, Default, Debug)]
pub struct Logic {
    /// Output levels; a counter uses all four.
    pub out: [bool; 4],
    /// Input levels as last read, for the dead band and for edges.
    pub ins: [bool; 4],
    /// A counter's count, or the digit a display shows.
    pub value: u8,
    /// The chip has power.
    pub on: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LedColor { Red, Green, Yellow, Blue }

impl LedColor {
    /// The forward voltage at 10 mA.
    pub fn forward(self) -> f64 {
        match self { LedColor::Red => 1.9, LedColor::Green => 2.1, LedColor::Yellow => 2.0, LedColor::Blue => 3.0 }
    }
    fn is(self) -> f64 {
        0.01 / (self.forward() / (LED_N * VT)).exp()
    }
}

#[derive(Clone, Debug)]
pub enum Dev {
    /// Plus and minus nodes.
    Battery { p: usize, n: usize, volts: f64 },
    Resistor { a: usize, b: usize, ohms: f64 },
    Capacitor { a: usize, b: usize, farads: f64 },
    /// Anode and cathode.
    Led { a: usize, k: usize, color: LedColor, burnt: bool },
    Switch { a: usize, b: usize, closed: bool },
    /// Collector, base, emitter.
    Npn { c: usize, b: usize, e: usize },
    /// Inputs a and b (a NOT only reads a), output q. Every logic part
    /// below also has its + and − nodes.
    Gate { gate: Gate, a: usize, b: usize, q: usize, plus: usize, minus: usize },
    /// A square wave on q, `hz` times a second.
    Clock { q: usize, hz: f64, plus: usize, minus: usize },
    /// Counts each rising edge on clk; r high holds it at 0. Outputs 1, 2, 4, 8.
    Counter { clk: usize, rst: usize, q: [usize; 4], plus: usize, minus: usize },
    /// Shows the digit its inputs 1, 2, 4, 8 add up to.
    Display { d: [usize; 4], plus: usize, minus: usize },
    /// A 555 timer: trigger, threshold, discharge, output.
    Timer { trig: usize, thres: usize, dis: usize, out: usize, plus: usize, minus: usize },
}

impl Dev {
    /// A logic part's + and − nodes.
    fn supply(&self) -> Option<(usize, usize)> {
        match *self {
            Dev::Gate { plus, minus, .. } | Dev::Clock { plus, minus, .. } | Dev::Counter { plus, minus, .. }
            | Dev::Display { plus, minus, .. } | Dev::Timer { plus, minus, .. } => Some((plus, minus)),
            _ => None,
        }
    }
}

pub struct Circuit {
    pub nodes: usize,
    pub ground: usize,
    pub devs: Vec<Dev>,
}

impl Circuit {
    /// A clock keeps a circuit changing for as long as the power is on.
    pub fn has_clock(&self) -> bool {
        self.devs.iter().any(|d| matches!(d, Dev::Clock { .. }))
    }
}

pub struct Sim {
    /// Node voltages.
    pub v: Vec<f64>,
    /// Seconds simulated.
    pub t: f64,
    /// Per device: the current through it, in amps. A battery's is the
    /// current out of its plus side, an LED's from anode to cathode, a
    /// transistor's into its collector.
    pub current: Vec<f64>,
    /// Per device: a transistor's base current.
    pub base: Vec<f64>,
    /// Per device: set when an LED has burnt out.
    pub burnt: Vec<bool>,
    /// Per device: a logic part's levels and count.
    pub logic: Vec<Logic>,
    cap_v: Vec<f64>,
    junc: Vec<[f64; 2]>,
    hot: Vec<f64>,
}

impl Sim {
    /// A circuit just switched on: every capacitor empty.
    pub fn new(c: &Circuit) -> Self {
        let n = c.devs.len();
        Sim {
            v: vec![0.0; c.nodes],
            t: 0.0,
            current: vec![0.0; n],
            base: vec![0.0; n],
            burnt: c.devs.iter().map(|d| matches!(d, Dev::Led { burnt: true, .. })).collect(),
            logic: vec![Logic::default(); n],
            cap_v: vec![0.0; n],
            junc: vec![[0.0; 2]; n],
            hot: vec![0.0; n],
        }
    }

    /// Advance `dt` seconds. Returns how far the furthest node voltage moved.
    pub fn step(&mut self, c: &Circuit, dt: f64) -> f64 {
        let n = c.nodes;
        let before = self.v.clone();
        let mut v = self.v.clone();
        for iter in 0..80 {
            let mut g = vec![0.0; n * n];
            let mut rhs = vec![0.0; n];
            for i in 0..n { g[i * n + i] += GMIN; }
            let mut limited = false;
            for (di, d) in c.devs.iter().enumerate() {
                match *d {
                    Dev::Battery { p, n: m, volts } => {
                        conductance(&mut g, n, p, m, 1.0 / BATTERY_R);
                        rhs[p] += volts / BATTERY_R;
                        rhs[m] -= volts / BATTERY_R;
                    }
                    Dev::Resistor { a, b, ohms } => conductance(&mut g, n, a, b, 1.0 / ohms.max(1e-3)),
                    Dev::Switch { a, b, closed } => {
                        if closed { conductance(&mut g, n, a, b, 1.0 / SWITCH_ON_R); }
                    }
                    Dev::Capacitor { a, b, farads } => {
                        let gc = farads / dt;
                        conductance(&mut g, n, a, b, gc);
                        rhs[a] += gc * self.cap_v[di];
                        rhs[b] -= gc * self.cap_v[di];
                    }
                    Dev::Led { a, k, color, .. } => {
                        if self.burnt[di] { continue; }
                        let nvt = LED_N * VT;
                        let is = color.is();
                        let raw = v[a] - v[k];
                        let vd = limit(raw, self.junc[di][0], nvt, is);
                        limited |= (vd - raw).abs() > 1e-9;
                        self.junc[di][0] = vd;
                        let (i, gd) = diode(vd, is, nvt);
                        conductance(&mut g, n, a, k, gd);
                        let ieq = i - gd * vd;
                        rhs[a] -= ieq;
                        rhs[k] += ieq;
                    }
                    Dev::Npn { c: col, b, e } => {
                        let rbe = v[b] - v[e];
                        let rbc = v[b] - v[col];
                        let vbe = limit(rbe, self.junc[di][0], VT, NPN_IS);
                        let vbc = limit(rbc, self.junc[di][1], VT, NPN_IS);
                        limited |= (vbe - rbe).abs() > 1e-9 || (vbc - rbc).abs() > 1e-9;
                        self.junc[di] = [vbe, vbc];
                        let t = npn(vbe, vbc);
                        // Collector, base and emitter currents out of their nodes,
                        // linearised around (vbe, vbc).
                        for (node, i0, dbe, dbc) in [
                            (col, t.ic, t.dic_dbe, t.dic_dbc),
                            (b, t.ib, t.dib_dbe, t.dib_dbc),
                            (e, -(t.ic + t.ib), -(t.dic_dbe + t.dib_dbe), -(t.dic_dbc + t.dib_dbc)),
                        ] {
                            g[node * n + b] += dbe + dbc;
                            g[node * n + e] -= dbe;
                            g[node * n + col] -= dbc;
                            rhs[node] -= i0 - dbe * vbe - dbc * vbc;
                        }
                    }
                    Dev::Gate { q, plus, minus, .. } | Dev::Clock { q, plus, minus, .. } => {
                        drive(&mut g, n, q, plus, minus, self.logic[di], 0);
                    }
                    Dev::Counter { q, plus, minus, .. } => {
                        for (k, &node) in q.iter().enumerate() { drive(&mut g, n, node, plus, minus, self.logic[di], k); }
                    }
                    Dev::Timer { dis, out, plus, minus, .. } => {
                        let l = self.logic[di];
                        drive(&mut g, n, out, plus, minus, l, 0);
                        if l.on && !l.out[0] { conductance(&mut g, n, dis, minus, 1.0 / DIS_R); }
                    }
                    Dev::Display { .. } => {}
                }
            }
            // The ground node is pinned at 0 V.
            let gr = c.ground;
            for j in 0..n { g[gr * n + j] = 0.0; }
            g[gr * n + gr] = 1.0;
            rhs[gr] = 0.0;
            let Some(next) = solve(g, rhs, n) else { break };
            let moved = next.iter().zip(&v).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
            v = next;
            if iter > 0 && moved < 1e-7 && !limited { break; }
        }
        self.v = v;
        self.t += dt;
        self.measure(c, dt);
        let moved = before.iter().zip(&self.v).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        if self.logic_step(c) { moved.max(1.0) } else { moved }
    }

    /// Chips read their inputs and set their outputs for the next step.
    /// True when an output changed.
    fn logic_step(&mut self, c: &Circuit) -> bool {
        let v = &self.v;
        let t = self.t;
        let mut changed = false;
        for (di, d) in c.devs.iter().enumerate() {
            let Some((plus, minus)) = d.supply() else { continue };
            let l = &mut self.logic[di];
            let before = (l.out, l.on);
            let vs = v[plus] - v[minus];
            if vs < MIN_SUPPLY {
                // No power: the chip forgets its count and lets go of its outputs.
                *l = Logic::default();
                changed |= (l.out, l.on) != before;
                continue;
            }
            l.on = true;
            // High above 60% of the chip's supply, low below 40%; in
            // between an input keeps what it read last.
            let read = |node: usize, was: bool| {
                let x = v[node] - v[minus];
                if was { x > 0.4 * vs } else { x > 0.6 * vs }
            };
            match *d {
                Dev::Gate { gate, a, b, .. } => {
                    l.ins[0] = read(a, l.ins[0]);
                    l.ins[1] = read(b, l.ins[1]);
                    l.out[0] = gate.eval(l.ins[0], l.ins[1]);
                }
                Dev::Clock { hz, .. } => l.out[0] = (t * hz.max(1e-3)).fract() < 0.5,
                Dev::Counter { clk, rst, .. } => {
                    let now = read(clk, l.ins[0]);
                    let reset = read(rst, l.ins[1]);
                    if reset { l.value = 0; } else if now && !l.ins[0] { l.value = (l.value + 1) & 15; }
                    l.ins[0] = now;
                    l.ins[1] = reset;
                    for k in 0..4 { l.out[k] = (l.value >> k) & 1 == 1; }
                }
                Dev::Display { d, .. } => {
                    let mut value = 0;
                    for k in 0..4 {
                        l.ins[k] = read(d[k], l.ins[k]);
                        if l.ins[k] { value |= 1 << k; }
                    }
                    l.value = value;
                }
                Dev::Timer { trig, thres, .. } => {
                    // The trigger wins: below a third of the supply the
                    // output goes high; above two thirds on the threshold, low.
                    if v[trig] - v[minus] < vs / 3.0 { l.out[0] = true; } else if v[thres] - v[minus] > 2.0 * vs / 3.0 { l.out[0] = false; }
                }
                _ => {}
            }
            changed |= (l.out, l.on) != before;
        }
        changed
    }

    /// Currents from the settled voltages, capacitor memory, burnt LEDs.
    fn measure(&mut self, c: &Circuit, dt: f64) {
        let v = &self.v;
        for (di, d) in c.devs.iter().enumerate() {
            self.current[di] = match *d {
                Dev::Battery { p, n, volts } => (volts - (v[p] - v[n])) / BATTERY_R,
                Dev::Resistor { a, b, ohms } => (v[a] - v[b]) / ohms.max(1e-3),
                Dev::Switch { a, b, closed } => if closed { (v[a] - v[b]) / SWITCH_ON_R } else { 0.0 },
                Dev::Capacitor { a, b, farads } => {
                    let now = v[a] - v[b];
                    let i = farads * (now - self.cap_v[di]) / dt;
                    self.cap_v[di] = now;
                    i
                }
                Dev::Led { a, k, color, .. } => {
                    if self.burnt[di] { 0.0 } else {
                        let i = diode(v[a] - v[k], color.is(), LED_N * VT).0;
                        self.hot[di] = if i > LED_MAX { self.hot[di] + dt } else { 0.0 };
                        if self.hot[di] >= BURN_TIME { self.burnt[di] = true; }
                        i
                    }
                }
                Dev::Npn { c: col, b, e } => {
                    let t = npn(v[b] - v[e], v[b] - v[col]);
                    self.base[di] = t.ib;
                    t.ic
                }
                Dev::Gate { .. } | Dev::Clock { .. } | Dev::Counter { .. } | Dev::Display { .. } | Dev::Timer { .. } => 0.0,
            };
        }
    }
}

/// Logic output `k`: 100 Ω to the chip's + when high, to its − when low,
/// and nothing while the chip has no power.
fn drive(g: &mut [f64], n: usize, node: usize, plus: usize, minus: usize, l: Logic, k: usize) {
    if l.on { conductance(g, n, node, if l.out[k] { plus } else { minus }, 1.0 / OUT_R); }
}

fn conductance(g: &mut [f64], n: usize, a: usize, b: usize, s: f64) {
    g[a * n + a] += s;
    g[b * n + b] += s;
    g[a * n + b] -= s;
    g[b * n + a] -= s;
}

/// exp that cannot overflow: past 80 it carries on as a straight line.
fn safe_exp(x: f64) -> (f64, f64) {
    if x > 80.0 { let e = 80f64.exp(); (e * (1.0 + x - 80.0), e) } else { let e = x.exp(); (e, e) }
}

/// Diode current and its slope at voltage `vd`.
fn diode(vd: f64, is: f64, nvt: f64) -> (f64, f64) {
    let (e, de) = safe_exp(vd / nvt);
    (is * (e - 1.0) + GMIN * vd, is / nvt * de + GMIN)
}

/// Keep a junction's voltage from jumping so far in one iteration that the
/// exponential blows up (the junction limiting SPICE uses).
fn limit(new: f64, old: f64, vt: f64, is: f64) -> f64 {
    let crit = vt * (vt / (std::f64::consts::SQRT_2 * is)).ln();
    if new > crit && (new - old).abs() > 2.0 * vt {
        if old > 0.0 {
            let arg = 1.0 + (new - old) / vt;
            if arg > 0.0 { old + vt * arg.ln() } else { crit }
        } else {
            vt * (new / vt).ln()
        }
    } else {
        new
    }
}

struct Npn { ic: f64, ib: f64, dic_dbe: f64, dic_dbc: f64, dib_dbe: f64, dib_dbc: f64 }

/// The Ebers-Moll transistor: collector and base current, and how they
/// change with the base-emitter and base-collector voltages.
fn npn(vbe: f64, vbc: f64) -> Npn {
    let (ef, def) = safe_exp(vbe / VT);
    let (er, der) = safe_exp(vbc / VT);
    let (f, r) = (NPN_IS * (ef - 1.0), NPN_IS * (er - 1.0));
    let (gf, gr) = (NPN_IS / VT * def, NPN_IS / VT * der);
    Npn {
        ic: f - r - r / NPN_BR,
        ib: f / NPN_BF + r / NPN_BR,
        dic_dbe: gf,
        dic_dbc: -gr * (1.0 + 1.0 / NPN_BR),
        dib_dbe: gf / NPN_BF,
        dib_dbc: gr / NPN_BR,
    }
}

/// Gaussian elimination with partial pivoting.
fn solve(mut g: Vec<f64>, mut rhs: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    for col in 0..n {
        let piv = (col..n).max_by(|&a, &b| g[a * n + col].abs().total_cmp(&g[b * n + col].abs()))?;
        if g[piv * n + col].abs() < 1e-18 { return None; }
        if piv != col {
            for j in 0..n { g.swap(col * n + j, piv * n + j); }
            rhs.swap(col, piv);
        }
        let d = g[col * n + col];
        for row in col + 1..n {
            let f = g[row * n + col] / d;
            if f == 0.0 { continue; }
            for j in col..n { g[row * n + j] -= f * g[col * n + j]; }
            rhs[row] -= f * rhs[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut s = rhs[row];
        for j in row + 1..n { s -= g[row * n + j] * x[j]; }
        x[row] = s / g[row * n + row];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(c: &Circuit, seconds: f64) -> Sim {
        let mut s = Sim::new(c);
        let steps = (seconds / 1e-3).round() as usize;
        for _ in 0..steps { s.step(c, 1e-3); }
        s
    }

    #[test]
    fn an_and_gate_follows_its_inputs() {
        // 1 plus, 2 input a (held high), 3 input b (switch with pull-down), 4 output.
        let mut c = Circuit { nodes: 5, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Resistor { a: 1, b: 2, ohms: 1000.0 },
            Dev::Switch { a: 1, b: 3, closed: false },
            Dev::Resistor { a: 3, b: 0, ohms: 10_000.0 },
            Dev::Gate { gate: Gate::And, a: 2, b: 3, q: 4, plus: 1, minus: 0 },
            Dev::Resistor { a: 4, b: 0, ohms: 10_000.0 },
        ]};
        let s = run(&c, 0.02);
        assert!(s.v[4] < 0.5, "low with b low: {}", s.v[4]);
        c.devs[2] = Dev::Switch { a: 1, b: 3, closed: true };
        let s = run(&c, 0.02);
        assert!(s.v[4] > 8.0, "high with both high: {}", s.v[4]);
    }

    #[test]
    fn a_chip_runs_on_its_own_plus_and_minus_and_its_output_current_comes_from_the_battery() {
        // 1 plus, 2 output into 1 kΩ, 3 a + wire that goes nowhere.
        let mut c = Circuit { nodes: 4, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Gate { gate: Gate::And, a: 1, b: 1, q: 2, plus: 1, minus: 0 },
            Dev::Resistor { a: 2, b: 0, ohms: 1000.0 },
        ]};
        let s = run(&c, 0.01);
        assert!(s.logic[1].on && s.v[2] > 8.0, "powered output {}", s.v[2]);
        assert!((s.current[0] - s.current[2]).abs() < 1e-6, "battery {} load {}", s.current[0], s.current[2]);
        c.devs[1] = Dev::Gate { gate: Gate::And, a: 1, b: 1, q: 2, plus: 3, minus: 0 };
        let s = run(&c, 0.01);
        assert!(!s.logic[1].on && s.v[2].abs() < 1e-3, "unpowered output {}", s.v[2]);
        assert!(s.current[0].abs() < 1e-6, "the battery gives nothing: {}", s.current[0]);
    }

    #[test]
    fn a_clock_drives_a_counter_and_the_display_shows_its_count() {
        let c = Circuit { nodes: 7, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Clock { q: 2, hz: 10.0, plus: 1, minus: 0 },
            Dev::Counter { clk: 2, rst: 0, q: [3, 4, 5, 6], plus: 1, minus: 0 },
            Dev::Display { d: [3, 4, 5, 6], plus: 1, minus: 0 },
        ]};
        let s = run(&c, 1.05);
        let count = s.logic[2].value;
        assert!((10..=11).contains(&count), "count {count}");
        assert_eq!(s.logic[3].value, count);
    }

    #[test]
    fn a_555_with_10k_68k_and_10uf_blinks_about_once_a_second() {
        // 1 plus, 2 discharge, 3 capacitor (trigger and threshold), 4 output.
        let c = Circuit { nodes: 5, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Resistor { a: 1, b: 2, ohms: 10_000.0 },
            Dev::Resistor { a: 2, b: 3, ohms: 68_000.0 },
            Dev::Capacitor { a: 3, b: 0, farads: 10e-6 },
            Dev::Timer { trig: 3, thres: 3, dis: 2, out: 4, plus: 1, minus: 0 },
            Dev::Resistor { a: 4, b: 0, ohms: 10_000.0 },
        ]};
        let mut s = Sim::new(&c);
        let (mut high, mut rises) = (false, Vec::new());
        for _ in 0..6000 {
            s.step(&c, 1e-3);
            let now = s.v[4] > 4.5;
            if now && !high { rises.push(s.t); }
            high = now;
        }
        let periods: Vec<f64> = rises.windows(2).skip(1).map(|w| w[1] - w[0]).collect();
        let mean = periods.iter().sum::<f64>() / periods.len() as f64;
        assert!((0.8..1.25).contains(&mean), "period {mean}, rises {rises:?}");
    }

    #[test]
    fn an_and_gate_on_outputs_2_and_8_makes_a_counter_stop_at_nine() {
        // Counter 1 counts a 100 Hz clock; AND of its 2 and 8 resets it and
        // clocks counter 2, which then counts tens.
        let c = Circuit { nodes: 12, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Clock { q: 2, hz: 100.0, plus: 1, minus: 0 },
            Dev::Counter { clk: 2, rst: 7, q: [3, 4, 5, 6], plus: 1, minus: 0 },
            Dev::Gate { gate: Gate::And, a: 4, b: 6, q: 7, plus: 1, minus: 0 },
            Dev::Counter { clk: 7, rst: 0, q: [8, 9, 10, 11], plus: 1, minus: 0 },
        ]};
        let mut s = Sim::new(&c);
        let mut over = 0;
        for _ in 0..550 {
            s.step(&c, 1e-3);
            if s.logic[2].value >= 10 { over += 1; }
        }
        assert!(over <= 30, "counter 1 sat at 10 or more for {over} ms");
        let tens = s.logic[4].value;
        assert!((5..=6).contains(&tens), "tens {tens}");
    }

    #[test]
    fn a_divider_halves_the_battery() {
        // 0 ground, 1 battery plus, 2 middle.
        let c = Circuit { nodes: 3, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Resistor { a: 1, b: 2, ohms: 1000.0 },
            Dev::Resistor { a: 2, b: 0, ohms: 1000.0 },
        ]};
        let s = run(&c, 0.01);
        assert!((s.v[2] - 4.5).abs() < 0.01, "{}", s.v[2]);
        assert!((s.current[0] - 0.0045).abs() < 1e-4);
    }

    #[test]
    fn an_led_through_470_ohms_draws_about_15_ma() {
        let c = Circuit { nodes: 3, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Resistor { a: 1, b: 2, ohms: 470.0 },
            Dev::Led { a: 2, k: 0, color: LedColor::Red, burnt: false },
        ]};
        let s = run(&c, 0.01);
        let i = s.current[2];
        assert!((0.013..0.016).contains(&i), "{i}");
        assert!((1.8..2.1).contains(&s.v[2]), "{}", s.v[2]);
        assert!(!s.burnt[2]);
    }

    #[test]
    fn an_led_straight_on_a_battery_burns_out() {
        let c = Circuit { nodes: 2, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Led { a: 1, k: 0, color: LedColor::Red, burnt: false },
        ]};
        let s = run(&c, 0.1);
        assert!(s.burnt[1]);
        assert_eq!(s.current[1], 0.0);
    }

    #[test]
    fn a_capacitor_reaches_63_percent_after_one_time_constant() {
        // 10 k and 100 µF: one second.
        let c = Circuit { nodes: 3, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Resistor { a: 1, b: 2, ohms: 10_000.0 },
            Dev::Capacitor { a: 2, b: 0, farads: 100e-6 },
        ]};
        let s = run(&c, 1.0);
        let want = 9.0 * (1.0 - (-1.0f64).exp());
        assert!((s.v[2] - want).abs() < 0.1, "{} vs {want}", s.v[2]);
    }

    #[test]
    fn a_small_base_current_switches_a_large_collector_current() {
        // 1 plus, 2 base, 3 LED cathode, 4 collector, 5 switch to base.
        let mut c = Circuit { nodes: 6, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Led { a: 1, k: 3, color: LedColor::Red, burnt: false },
            Dev::Resistor { a: 3, b: 4, ohms: 470.0 },
            Dev::Npn { c: 4, b: 2, e: 0 },
            Dev::Switch { a: 1, b: 5, closed: true },
            Dev::Resistor { a: 5, b: 2, ohms: 10_000.0 },
        ]};
        let s = run(&c, 0.01);
        assert!((0.012..0.016).contains(&s.current[1]), "LED {}", s.current[1]);
        assert!(s.current[4] < s.current[1] / 10.0, "switch {} LED {}", s.current[4], s.current[1]);
        c.devs[4] = Dev::Switch { a: 1, b: 5, closed: false };
        let s = run(&c, 0.01);
        assert!(s.current[1] < 1e-4, "LED off {}", s.current[1]);
    }

    #[test]
    fn two_transistors_take_turns_and_blink() {
        // 1 plus; Q1: collector 2, base 3; Q2: collector 4, base 5;
        // LED cathodes 6 and 7.
        let c = Circuit { nodes: 8, ground: 0, devs: vec![
            Dev::Battery { p: 1, n: 0, volts: 9.0 },
            Dev::Led { a: 1, k: 6, color: LedColor::Red, burnt: false },
            Dev::Resistor { a: 6, b: 2, ohms: 470.0 },
            Dev::Led { a: 1, k: 7, color: LedColor::Green, burnt: false },
            Dev::Resistor { a: 7, b: 4, ohms: 474.0 },
            Dev::Resistor { a: 1, b: 3, ohms: 47_000.0 },
            Dev::Resistor { a: 1, b: 5, ohms: 47_400.0 },
            Dev::Capacitor { a: 2, b: 5, farads: 10e-6 },
            Dev::Capacitor { a: 4, b: 3, farads: 10.1e-6 },
            Dev::Npn { c: 2, b: 3, e: 0 },
            Dev::Npn { c: 4, b: 5, e: 0 },
        ]};
        let mut s = Sim::new(&c);
        let (mut on, mut flips, mut last) = (false, 0, 0.0);
        let mut spans = Vec::new();
        for _ in 0..4000 {
            s.step(&c, 1e-3);
            let now = if on { s.current[1] > 0.001 } else { s.current[1] > 0.005 };
            if now != on {
                if flips > 0 { spans.push(s.t - last); }
                on = now;
                flips += 1;
                last = s.t;
            }
        }
        assert!(flips >= 6, "flips {flips}");
        let mean = spans.iter().sum::<f64>() / spans.len() as f64;
        assert!((0.2..0.5).contains(&mean), "mean half period {mean}, spans {spans:?}");
    }
}
