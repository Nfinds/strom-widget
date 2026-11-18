<p align="center"><img src="docs/img/hero.png" alt="Strom-Widget: what your PC costs, right now. The collapsed strip and the expanded Live panel."></p>

# Strom-Widget

**A power meter for your Windows taskbar, in watts and in money.**

It sits in the taskbar and shows three things: what your PC draws right now, what that costs per
hour at your tariff, and what this session has cost so far. Click it to open a panel with the full
breakdown, your local ports and your session history.

Written in Rust with [egui](https://github.com/emilk/egui). One 5 MB exe, with no Electron, no
installer, no background service and no admin rights.

<p align="center"><img src="docs/img/compact-strip.png" width="632" alt="The collapsed strip in the taskbar: energy class, watts, cost per hour, CPU, RAM and network"></p>

> [!NOTE]
> Processor and graphics card are measured. Mainboard, power supply loss and monitors are estimated
> from your settings, and every line in the widget says which of the two it is. This is not a
> wall-socket meter.

## Get it

Download `Strom-Widget.exe` from the [latest release](https://github.com/Nfinds/strom-widget/releases/latest)
and start it. It docks into the taskbar. Windows 10 or 11, nothing to install.

- **First start.** The exe is not code-signed, so Windows SmartScreen may say "Windows protected
  your PC". Click **More info**, then **Run anyway**.
- **Check the download.** The exe is built by GitHub Actions straight from this repository, and
  `SHA256SUMS.txt` next to it holds its checksum.
- **Remove it.** Delete the exe and the `%APPDATA%\Strom-Widget` folder.

## Live watts, in money

<img align="right" src="docs/img/expanded-live.png" width="300" alt="Live tab: energy class, breakdown per component, session totals">

The number in the strip is the whole PC at the wall socket. You see it as cost per hour, updating
live.

The Live tab splits that number into processor, graphics card, rest of the system, power supply
loss and monitors. Each line says whether it is **measured** or **estimated**, so you always know
how much of the total is a guess.

The colour is the EU energy label. Below 150 W you are in class A, and every further 100 W moves
you one class towards G.

Under the breakdown you get the session totals: start time, duration, kWh, average, peak and cost
so far. Above it is a chart for the last 15 minutes, the last hour or the whole session.

<br clear="right">

## How it measures

| Part | Source | |
|---|---|---|
| Processor | Windows "Energy Meter" counter (RAPL), no admin rights | measured |
| Graphics card | NVML board power, load and temperature (NVIDIA) | measured |
| Mainboard, RAM, drives, fans | the rest-of-system value in your settings | estimated |
| Power supply loss | from the load and the PSU rating you choose | estimated |
| Monitors | your list of monitors and their draw | estimated |

A second model runs alongside, built only from CPU and GPU load, and the Live tab shows how far
apart the two are.

For exact figures, plug the PC into a power meter once and adjust the rest-of-system value until
the two match.

## System & Ports

<img align="right" src="docs/img/system-ports.png" width="300" alt="System & Ports tab: detected local web interfaces, all listening ports with process and PID">

On a developer's machine, half a dozen things listen on localhost and nobody remembers which. This
tab shows them.

- **Detected web interfaces.** The widget sends a plain HTTP request to each listening port, reads
  each page's title and lists what answers: your dev server, a dashboard, a local tool. One click
  opens it in the browser.
- **All listening ports**, each with the program behind it, its PID and the address it is bound to.
  You see what is blocking port 3000, and what is reachable from your network.
- **Biggest RAM and CPU consumers**, grouped by program rather than by process, so 24 browser
  processes show up as one line.

The tab label shows the count, so you notice when something new starts listening.

<br clear="right">

## Spikes, sessions and the receipt

- **Spikes.** When the draw is at least 60 W and 35 % above the load of the last two minutes, the
  widget records a spike and the programs that were busy at that moment.
- **Sessions survive a power cut.** They are saved every 30 seconds, so even a hard power-off
  loses half a minute at most. The session is closed on the next start.
- **Good night.** One button (or Windows shutting down) closes the session and prints a receipt
  with duration, kWh, cost, average and peak, the spikes, the CPU load per program, and what a
  month like this would cost.
- **History** keeps every finished session with its total.

## Your setup, your numbers

<img align="right" src="docs/img/settings.png" width="300" alt="Settings: position, tariff, monitors, size, language, currency">

The more you tell the widget about your PC, the better its estimates get. In the settings you
enter:

- your tariff in your currency (`20`, `0,20`, `20 ct` and `$0.20` all mean 20 cents per kWh)
- your monitors and what each one draws
- what the rest of your system draws
- your power supply's rating

You also choose how it looks and where it sits:

- **Place.** In the taskbar of your main or second screen, or in a corner (top right, top centre,
  bottom right). Drag the strip sideways along the taskbar, or move it in 10 px steps.
- **English or German, euro or dollar.** On first start both follow your Windows settings.
- Two sizes, opacity when collapsed, always on top, start with Windows (current user, no admin
  rights).

**Optional: ask Claude.** If the [Claude CLI](https://docs.claude.com/en/docs/claude-code) is
installed and signed in, you can ask questions about the session or about a single spike. The
widget runs the CLI on your machine and stores no API key. Without the CLI the question field
simply stays unused.

<br clear="right">

## Your data stays on your PC

Settings and sessions are stored in `%APPDATA%\Strom-Widget`. The widget sends nothing anywhere.
The only network traffic it creates:

- checking your own local ports for the System & Ports tab
- the Claude CLI, if you ask it something

## Under the hood

<details>
<summary><b>Build it yourself</b></summary>

You need the stable Rust toolchain (MSVC).

```powershell
git clone https://github.com/Nfinds/strom-widget.git
cd strom-widget
.\build-release.ps1
```

The script builds a release exe with no local paths inside it and puts `Strom-Widget.exe` next to
the script. Plain `cargo build --release` works too.

</details>

<details>
<summary><b>Rebuild the images in this README</b></summary>

```powershell
Strom-Widget.exe --self-test-screenshots <folder> --demo-data --zoom 2
```

It opens every tab once, saves a PNG of each and exits. `--demo-data` swaps your real ports and
programs for example data. Close a running widget first: a second instance exits at once.

The picture at the top is [`docs/hero/hero.html`](docs/hero/hero.html), rendered with headless
Chrome at 1600 × 860.

</details>

<details>
<summary><b>What is in this repository</b></summary>

| Path | Content |
|---|---|
| `src/main.rs` | The window, the tabs, the settings and everything that is drawn |
| `src/power.rs` | The power model: measured and estimated watts, settings |
| `src/sensors.rs` | RAPL, NVML, network, RAM, listening ports and the probe for local web interfaces |
| `src/session.rs` | Sessions, history, spikes, and saving them continuously |
| `src/taskbar.rs` | Docking into the Windows taskbar and the corner placements |
| `src/theme.rs` | Colours, the energy label and the drawn icons |
| `src/claude.rs` | Asking the local Claude CLI, off the UI thread |
| `src/i18n.rs`, `src/autostart.rs` | German and English, start with Windows |
| `assets/` | The icon |
| `docs/` | The images of this README and the source of the hero picture |

Check a change with `cargo test` and `cargo clippy --all-targets -- -D warnings`. CI runs both on
every push.

</details>

<details>
<summary><b>How a release is made</b></summary>

Raise `version` in `Cargo.toml`, commit, then push a tag with the same number:

```powershell
git tag v2.0.1
git push origin v2.0.1
```

GitHub Actions checks that the tag matches `Cargo.toml`, builds the exe with
`build-release.ps1` and publishes it with `SHA256SUMS.txt`.

</details>

## Kurz auf Deutsch

<img align="right" src="docs/img/expanded-live-de.png" width="240" alt="Live-Ansicht auf Deutsch">

**Strom-Widget** ist ein kleines Windows-Widget für die Taskleiste. Es zeigt, wie viel Strom dein
PC gerade zieht, was das pro Stunde kostet und was die laufende Sitzung bisher gekostet hat.

- **Gemessen und geschätzt, getrennt ausgewiesen.** CPU und Grafikkarte werden gemessen.
  Mainboard, Netzteilverluste und Monitore werden geschätzt.
- **System & Ports** erkennt lokale Weboberflächen (Dev-Server, Dashboards) und öffnet sie mit
  einem Klick. Dazu kommen alle offenen Ports mit Programm und PID.
- **Lastspitzen** mit der wahrscheinlichen Ursache, **Sitzungen**, die auch einen harten
  Stromausfall überstehen, und zum Abschluss eine **Gute-Nacht-Quittung**.
- Deutsch oder Englisch, Euro oder Dollar. Alle Daten bleiben lokal in `%APPDATA%\Strom-Widget`.

**Download:** `Strom-Widget.exe` aus dem [neuesten Release](https://github.com/Nfinds/strom-widget/releases/latest)
laden und starten, ohne Installation. Beim ersten Start warnt Windows SmartScreen eventuell, weil
die exe nicht signiert ist: „Weitere Informationen“, dann „Trotzdem ausführen“.

<br clear="right">

## License

MIT, see [LICENSE](LICENSE). Made by [Nfinds](https://github.com/Nfinds) together with
[Claude](https://claude.com/claude-code) (Anthropic), which wrote most of the code. The interface
uses the Windows system fonts; no fonts are bundled.

<p align="center"><img src="assets/icon.svg" width="96" alt="The Strom-Widget icon"></p>
