//! The circuit solver.
//!
//! Every node's voltage is unknown. The currents flowing out of a node
//! must add up to zero, which gives one equation per node, and all of
//! them are solved together as one matrix (modified nodal analysis).
//! LEDs and transistors bend the equations, so each time step repeats the
//! solve until the voltages stop moving (Newton's method). A capacitor
//! remembers its voltage from the step before.

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
}

pub struct Circuit {
    pub nodes: usize,
    pub ground: usize,
    pub devs: Vec<Dev>,
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
        before.iter().zip(&self.v).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max)
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
            };
        }
    }
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
