# Web3 Pi LCD (`web3-pi-lcd`)

***The dedicated LCD dashboard for [Web3 Pi vOS](https://github.com/Web3-Pi/Web3-Pi-vOS).***

This repository builds and releases **`w3p-hwm`** — the firmware that drives the
1.69" ST7789V2 SPI display on a Raspberry Pi 5 running Web3 Pi vOS. On the panel
it shows the node at a glance: Ethereum client state (EXEC / CONS / VALI), sync,
peer counts, CPU / RAM / disk / temperature, IP address and UPS status.

**Relationship to Web3 Pi vOS.** On [Web3 Pi vOS](https://github.com/Web3-Pi/Web3-Pi-vOS)
the binary is **preinstalled and enabled by the image build**, with SPI on by
default, and is managed from the **Control Panel → System → LCD Display**
(install/update, start/stop, logs). The vOS image pulls a versioned release from
here, so you normally don't build or install from this repo directly — it exists
so the display firmware is versioned and can be built/updated independently of
the OS. The sections below cover standalone/manual use on a plain Web3 Pi image.

> `w3p-hwm` ships as a **single self-contained aarch64 binary** — the font, logos
> and opening animation are embedded, so there is no asset directory to deploy. A
> missing or unwired panel is non-fatal: the service stays active and renders as
> soon as a panel is present.

The project allows for the installation of a colorful LCD in the Argon Neo 5 enclosure.
We have designed our own 3D model of the enclosure cover with a space for the display. The assembly is simple, using snap-fits, with no tools required. The models are open-source, so anyone can print them on a 3D printer. The source code is also open-source, allowing users to add new functionalities, customize it to their needs, or add support for new displays.

<p align="center">
  <img src="docs/img/ArgonMainImage1.jpg">
</p>


## Requirements

- Raspberry Pi 5 (aarch64)
- [SPI interface enabled](docs/EnableSPI.md) — on by default on Web3 Pi vOS
- 1.69" LCD display with ST7789V2 Driver
  - Waveshare 24382 - [product page](https://www.waveshare.com/1.69inch-lcd-module.htm)
  - Seeed Studio 104990802 - [product page](https://www.seeedstudio.com/1-69inch-240-280-Resolution-IPS-LCD-Display-Module-p-5755.html)
- (Optional) 3D printed model of Argon Neo 5 cover
- (Optional) Argon Neo 5 enclosure

## Assembly

### 1. Connect wires
Connect the display to the Raspberry Pi according to the diagram below.  
The colors of the cables may vary depending on the supplier and batch. Focus on the function and pin number, not the color.

![Rpi_LCD_diagram.png](docs/img/Rpi_LCD_diagram.png)
Diagram is valid for Raspberry Pi 4 and Pi 5

If on Raspberry Pi 5 your LCD backlight is flickering connect `BL` to `3.3V PIN 17`

### 2. Mount display module

Mount the display in the printed enclosure cover. The display is held in place by four clips. Make sure all 3D printing support residues are removed and the surface to which the display adheres is flat. Install the display by sliding one side under the clips first, then pressing the other side down. Do not use excessive force to avoid damaging the display. The display should fit in easily.

Since each 3D printer may be calibrated differently, it may be necessary to adjust the scale of the 3D model in the slicer software before printing. Our prints are done on [Original Prusa i3 MK3S+](https://www.prusa3d.com/pl/produkt/drukarka-3d-original-prusa-i3-mk3s-3/).

### 3. Mount enclosure cover

Mount the enclosure cover and secure it with two screws. Make sure to arrange the cables inside the enclosure so they do not obstruct the fan and minimize interference with cooling.


## Installation

### Recommended: one-line installer

Downloads the latest release binary + systemd unit, enables SPI, and starts the
service. Re-run any time to update.

```shell
curl -sSL https://raw.githubusercontent.com/Web3-Pi/web3-pi-lcd/main/install.sh | sudo bash
```

Pin a specific version: `... | sudo bash -s -- v0.2.0`.

This installs the binary to `/usr/local/bin/w3p-hwm` and enables the
`w3p-hwm.service` systemd unit. On Web3 Pi vOS you can also manage it from the
**Control Panel → System → LCD Display** menu (install/update/start/stop).

To **stop**: `sudo systemctl stop w3p-hwm.service`. To **uninstall**:
`sudo ./remove_service.sh` (or disable + remove the unit and `/usr/local/bin/w3p-hwm`).

### Dev: install from a local build

Build off-device with `mise run build-aarch64` (see
[Build with mise](#build-with-mise)), then from the repo:

```shell
sudo ./create_service.sh   # install the local binary + unit
./run.sh                   # or run once in the foreground (Ctrl+C to stop)
```

## Web3 Pi vOS

Notes for running the dashboard on Web3 Pi vOS.

### Wiring

Identical to the diagram above: display on SPI0 CE0 (`/dev/spidev0.0`),
GPIO lines (BCM numbering): `DC=25`, `RST=27`, `BL=18`. The GPIO chip is
resolved by its label `pinctrl-rp1` (override with `W3P_GPIOCHIP`).

### SPI & install on vOS

On current Web3 Pi vOS images SPI is **enabled by default** (`dtparam=spi=on`
baked into the image) and `w3p-hwm` is **preinstalled and enabled** — the panel
lights up on first boot, nothing to do.

To (re)install or update on a running device, use the one-line installer above
or the **Control Panel → System → LCD Display** menu. If you are on an older
image without SPI on, `install.sh` enables it for you (reboot required), or run
`sudo ./scripts/enable-spi.sh`.

A missing or unwired panel is **not** fatal: the service stays `active` and
starts rendering as soon as a panel is present (retry with backoff), so it never
crash-loops on a Pi without the LCD.

### Environment variables

The service works out of the box; behaviour can be tuned with the `W3P_*`
environment variables documented in [docs/Configuration.md](docs/Configuration.md)
(GPIO chip, geth RPC / beacon REST endpoints, systemd unit names, poll
interval, install-status path, mock display).

### What the tiles mean on vOS

The top row shows one tile per Ethereum client, each with up to three lines:

| Tile | Source | Lines |
|---|---|---|
| EXEC | `geth.service` + geth JSON-RPC | service state, sync state, peer count |
| CONS | `nimbus-beacon-node.service` + beacon REST | service state, sync state, peer count |
| VALI | `nimbus-validator` | service state only |

**Line 1 — systemd service state** (`systemctl is-active`):
`running` (green), `starting` (yellow, activating/reloading), `stopped`
(gray, inactive — deliberately not red: a stopped validator is normal unless
you are staking), `failed` (red), `unknown` (gray, shown until the first
successful `systemctl` poll or for unrecognized states; if `systemctl` fails
mid-run the last known states are kept. Note: a missing or misspelled unit
name reports `inactive` and therefore shows as `stopped`).

**Line 2 — sync state** (only while the service is running):
`synced` (green), `syncing` (orange — sync in progress, or head older than
90 s: geth reports "synced" even when offline), `no api` (yellow — the
client's RPC/REST endpoint is unreachable). CONS counts as `synced` when the
sync distance is ≤ 2 slots and the beacon is not syncing. VALI never has this
line (the validator client has no sync concept here).

**Line 3 — peer count** (EXEC from `net_peerCount`, CONS from the beacon
`peer_count` endpoint), shown only when the API responded in the last poll
cycle. VALI never has this line.

## Build with mise

The project uses [mise](https://mise.jdx.dev/) for build tooling.

### 1. Install tools from `mise.toml`

```shell
mise install
```

### 2. Build AArch64 release binary (Raspberry Pi target)

```shell
mise run build-aarch64
```

### 3. Output binary path

```shell
target/aarch64-unknown-linux-gnu/release/w3p-hwm
```

## Customisation

In the Rust configuration file `src/app/config.rs`, there is a flag `SHOW_PER_CORE` that determines whether the CPU usage percentage should be in the range of `0-100%` or `0-400%`.

0-400% represents the summed load of each core in the Raspberry Pi.

```rust
# Choose how to display CPU usage percentages
pub const SHOW_PER_CORE: bool = false;
# False = [0 - 100%]
# True  = [0 - 400%]
```
note: Restart the service after making changes.   
```shell
sudo systemctl restart w3p-hwm.service
```

For full runtime/configuration values, see [docs/Configuration.md](docs/Configuration.md).


## 3D Model

The models are free, so anyone can print them on a 3D printer.

![3D_Model.png](docs/img/3D_Model.png)

Download 3D model: [3D_Model](docs/3D_Model)

## 3D Printing

We recommend printing with [PETG](https://botland.store/849-petg-filaments?manufacturers=devil-design,prusa&weight=1000-g&material=petg&diameter=1-75-mm) filament due to the high operating temperatures of the Raspberry Pi.  
To ensure the snap-fits print correctly, enable 'supports everywhere.'  
Use a 0.4 mm nozzle.  
0.2 mm layer height or smaller.  
Our models are printed on [Original Prusa i3 MK3S+](https://www.prusa3d.com/pl/produkt/drukarka-3d-original-prusa-i3-mk3s-3/)

If you do not have access to a 3D printer, you can order an online print from one of the providers such as [JLC3DP](https://jlc3dp.com/3d-printing-quote).   
There are various materials technology and you can choose from:
- FDM - ABS, ASA or PA12-CF
- MJF - PA16-HP Nylon
- SLS - 3201PA-F Nylon

![PrintBed.png](docs/img/PrintBed.png)
