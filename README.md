
 broken
 ..

wip
 
A terminal-based RF signal analysis tool for capturing, decoding, and retransmitting signals. Built in Rust with a real-time TUI powered by `ratatui`.

**Note:** Protocol decoders and encoders in this project are based on the excellent reference implementations from [ProtoPirate](https://protopirate.net/ProtoPirate/ProtoPirate) and [Flipper-ARF](https://github.com/limbo111111/Flipper-ARF).


## ⚠️ Disclaimer
**Use KAT only on systems and vehicles you own or have explicit, written permission to test.** Capturing, decoding, or transmitting keyfob and vehicle-access signals without authorization may be illegal in your jurisdiction. KAT is intended for security research, authorized penetration testing, and education. The authors assume no liability for misuse.

## ✨ Features
- **Real-time capture & Demodulation:** Receive AM/OOK signals via HackRF One (full RX/TX) and RTL-SDR (RX only).
- **89 Supported Protocols:** Adaptive decoding for a massive range
- **Signal Retransmission:** Replay captures or transmit Lock/Unlock/Trunk/Panic commands.
- **Export & Import:** `.fob` (versioned JSON with metadata) and `.sub` (Flipper Zero) format support.
- **Vulnerability DB:** Built-in CVE matching against capture metadata (e.g., Year/Make/Model).
- **Interactive TUI:** Real-time capture lists, signal detail panels, and VIM-style command line.

## 📻 Hardware Support
- **HackRF One** (Receive & Transmit)
  

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
