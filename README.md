# KAT — Keyfob Analysis Toolkit

A terminal-based RF signal analysis tool for capturing, decoding, and retransmitting automotive keyfob signals. Built in Rust with a real-time TUI powered by `ratatui`.

**Note:** Protocol decoders and encoders in this project are based on the excellent reference implementations from [ProtoPirate](https://protopirate.net/ProtoPirate/ProtoPirate) and [Flipper-ARF](https://github.com/limbo111111/Flipper-ARF).

![Keyfob Analysis Toolkit screenshot](images/kat-screenshot.png)

## ⚠️ Disclaimer
**Use KAT only on systems and vehicles you own or have explicit, written permission to test.** Capturing, decoding, or transmitting keyfob and vehicle-access signals without authorization may be illegal in your jurisdiction. KAT is intended for security research, authorized penetration testing, and education. The authors assume no liability for misuse.

## ✨ Features
- **Real-time capture & Demodulation:** Receive AM/OOK signals via HackRF One (full RX/TX) and RTL-SDR (RX only).
- **89 Supported Protocols:** Adaptive decoding for a massive range of vehicles, gate systems, and alarms.
- **KeeLoq Fallback:** Automatically tries KeeLoq decoding using embedded keystore keys for unknown signals.
- **Signal Retransmission:** Replay captures or transmit Lock/Unlock/Trunk/Panic commands.
- **Export & Import:** `.fob` (versioned JSON with metadata) and `.sub` (Flipper Zero) format support.
- **Vulnerability DB:** Built-in CVE matching against capture metadata (e.g., Year/Make/Model).
- **Interactive TUI:** Real-time capture lists, signal detail panels, and VIM-style command line.

## 📻 Hardware Support
- **HackRF One** (Receive & Transmit)
- **RTL-SDR / RTL433** (Receive Only)

## 🚀 Quick Start

### Dependencies
**macOS:** `brew install hackrf`
**Debian/Ubuntu:** `sudo apt install libhackrf-dev pkg-config libusb-1.0-0-dev`

### Build & Run
```bash
git clone <repo-url> && cd KAT
cargo build --release
./target/release/kat
```

## 📡 Supported Protocols
KAT supports **89** protocol decoders.

<details>
<summary><b>Click to view all 89 Supported Protocols</b></summary>

Alutech AT-4N, Ansonic, Beninca ARC, BETT, BinRAW, BMW CAS4, CAME, CAME Atomo, CAME TWEE, Chamberlain Code, Chrysler, Clemsa, Dickert MAHS, Doitrand, Dooya, Elplast, FAAC SLH, Feron, Fiat Marelli, Fiat Spa, Fiat V0, Fiat V1, Ford V0, Ford V1, Ford V2, Ford V3, GangQi, GateTX, Hay21, Hollarm, Holtek, Holtek_HT12X, Honda Static, Honeywell, Honeywell Sec, Hormann HSM, Hyundai/Kia RIO, IDo117/111, Intertechno_V3, Jarolift, KeeLoq, KeyFinder, Kia V0, Kia V1, Kia V2, Kia V3/V4, Kia V5, Kia V6, Kia V7, KingGates Stylo4k, Land Rover RKE, Legrand, Linear, Linear Delta3, Magellan, Marantec, Marantec24, Mastercode, Mazda V0, MazdaSiemens, MegaCode, Mitsubishi V0, Nero Radio, Nero Sketch, Nice Flo, Nice FloR-S, Porsche Cayenne, Porsche Touareg, PowerSmart, Princeton, PSA, PSA2, Revers_RB2, Roger, SantaFe 13-16, Scher-Khan, SecPlus_v1, SecPlus_v2, Sheriff CFM, SMC5326, Somfy Keytis, Somfy Telis, Star Line, Subaru, Suzuki, Treadmill37, V2 Phoenix, VAG.
</details>

## 🛡️ Vulnerability Database & Cryptography

<details>
<summary><b>Vulnerability DB (CVE matching)</b></summary>
KAT automatically matches capture metadata against a built-in CVE list, linking directly to the NVD. Includes matches for known RollBack and replay attacks on Honda, Nissan, Mazda, Renault, and more.
</details>

<details>
<summary><b>Cryptographic Modules</b></summary>

- **KeeLoq** (Normal, Secure, FAAC, Magic Serial/XOR)
- **AES-128** (Kia V6)
- **Modified TEA** (PSA)
- **AUT64** (VAG)
- **Embedded Keystore** for major manufacturers.
</details>

## 📖 Usage & Commands

Use the interactive TUI with `j`/`k` (or arrows) to navigate, `Tab` for radio settings, and `Enter` to open signal actions.
Alternatively, use the built-in VIM-style command line (`:`):
- `:freq <MHz>` - Set receive frequency.
- `:lock <ID>`, `:unlock <ID>`, `:trunk <ID>`, `:panic <ID>` - Transmit vehicle commands.
- `:replay <ID>` - Replay a raw capture.
- `:save <ID>`, `:load <file>`, `:delete <ID>` - Manage captures.
- `:q` / `:quit` - Exit application.

*(Captures are in-memory and auto-save requires explicit export to `~/.config/KAT/exports` via the action menu or commands.)*

## 🤝 Credits
KAT is developed by **Kara Zajac (.leviathan)**.

A massive thanks to **[ProtoPirate](https://protopirate.net/ProtoPirate/ProtoPirate)** and **[Flipper-ARF](https://github.com/limbo111111/Flipper-ARF)**. The protocol decoders, reference implementations, and extensive community work from these projects form the foundation of this tool. I am truly standing on the shoulders of giants.

## 📄 License
[BSD-3-Clause NO MILITARY NO GOVERNMENT](LICENSE)
