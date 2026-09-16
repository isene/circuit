# circuit

<img src="img/circuit.svg" align="right" width="150">

**Build circuits in the terminal and watch them work. Written in Rust.**

![Rust](https://img.shields.io/badge/language-Rust-f74c00) ![License](https://img.shields.io/badge/license-Unlicense-green) ![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS-blue) ![Stay Amazing](https://img.shields.io/badge/Stay-Amazing-important)

Put batteries, resistors, capacitors, LEDs, switches and transistors on a
grid, wire them up and power on. Every voltage and current is worked out
as you watch: wires glow green with voltage, LEDs light up with their
current, and an LED with no resistor burns out. Challenges teach one idea
at a time, and each one ticks itself off when your circuit works.

Part of the [Fe₂O₃ suite](https://isene.github.io/fe2o3/). Built on
[crust](https://github.com/isene/crust).

![A two-transistor blinker running, the third challenge done](img/screenshot.png)

## The challenges

1. **Light an LED** without burning it out, and find out why it needs a
   resistor.
2. **A transistor as a switch**: a tiny current through a switch turns a
   big LED current on and off.
3. **Make it blink**: two transistors and two capacitors take turns, a
   third of a second each.

`n` steps through them, and a free board is always there for building
anything else. Every board is kept in `~/.circuit/` when you quit.

## Keys

| Key | Action |
|---|---|
| `←` `↑` `↓` `→` / `h` `j` `k` `l` | move the cursor |
| `1` … `6` | add a battery, resistor, capacitor, LED, switch or transistor |
| `w` | draw a wire: move to lay it, `w` again stops |
| `.` | join two wires that cross; crossing wires do not touch |
| `x` | delete the wire or part under the cursor |
| `o` | turn a part |
| `m` | move a part: the arrows carry it, `m` drops it |
| `+` `-` | change a value or an LED's colour |
| `Space` | flip a switch |
| `r` | replace a burnt-out LED |
| `p` | power on or off |
| `n` `N` | next or previous challenge |
| `R` `R` | put the board back to its start |
| `?` | every key |
| `q` | quit |

Values are printed the way they are marked on real parts, with the unit
letter standing in for the decimal point: `4k7` is 4.7 kΩ, `10µ` is
10 µF, `9V0` is 9 V.

## How it works

Every junction's voltage is unknown. The currents flowing into a junction
must add up to zero, so each junction gives one equation, and they are
solved together. A capacitor remembers its voltage from one millisecond
to the next. LEDs and transistors bend the equations, so each step
repeats the solve until the numbers settle.

Real parts are never exactly their marked value, so each one is off by up
to 1%. That is also what lets a blinker pick a side and start.

The solver runs only while the power is on and something is still
changing. A circuit that has settled costs nothing. A blinking one costs
about 16 ms of processor time a second.

## Install

```bash
git clone https://github.com/isene/circuit
cd circuit
cargo build --release
```

`crust` is expected as a sibling checkout (`../crust`). Release binaries
for Linux and macOS are on the
[releases page](https://github.com/isene/circuit/releases).

## License

Public domain (Unlicense).
