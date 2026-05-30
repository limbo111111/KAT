# KAT - Android / Termux Portierungsplan

Dieses Dokument dient als detaillierter Bauplan für zukünftige AI-Agents und Entwickler, um das Keyfob Analysis Toolkit (KAT) auf Android (via Termux) zu portieren. Da Android direkten Zugriff auf USB-Geräte unterbindet, erfordert diese Portierung spezifische Anpassungen beim USB-Handling und bei der Kompilierung.

## Zielsetzung
Das primäre Ziel ist es, die bestehende Ratatui-basierte TUI von KAT nativ innerhalb von **Termux** auf einem Android-Gerät auszuführen. Die Anwendung soll per USB OTG mit einem HackRF One kommunizieren können.

Eine native Android-App (APK) mit eigener GUI ist nicht Teil dieses Plans, sondern eine mögliche spätere Iteration.

---

## 1. Voraussetzungen & Systemumgebung

Um KAT unter Android auszuführen, werden folgende Komponenten auf dem Android-Gerät benötigt:
* **Termux** (Terminal-Emulator, vorzugsweise aus F-Droid).
* **Termux:API** (Zusatz-App für Systemzugriffe, wie z.B. USB).
* Ein OTG-fähiges Android-Gerät.
* Ein HackRF One verbunden über ein USB-OTG-Kabel.

In Termux müssen folgende Pakete installiert sein:
```bash
pkg install termux-api libusb clang rust
```

---

## 2. Herausforderung: USB OTG Berechtigungen (Das File-Descriptor Problem)

**Das Problem:**
Unter regulären Linux-Systemen greift `libhackrf` (bzw. `libusb`) direkt auf `/dev/bus/usb/` zu. Android blockiert diesen direkten Pfad aus Sicherheitsgründen (ohne Root-Rechte).

**Die Lösung:**
Android stellt stattdessen einen "File Descriptor" (FD) über die Java-API zur Verfügung. In Termux geschieht dies über das Tool `termux-usb`.

### Workflow für den USB-Zugriff:
1. **Gerät identifizieren:** `termux-usb -l` listet die USB-Geräte auf.
2. **Berechtigung anfordern:** `termux-usb -r /dev/bus/usb/xxx/yyy` öffnet einen Android-Dialog, in dem der Nutzer dem USB-Zugriff zustimmen muss.
3. **Programm mit FD starten:** Termux übergibt den File Descriptor an das Programm (z.B. über `termux-usb -e ./kat /dev/bus/usb/xxx/yyy`).

### Code-Anpassungen in KAT / `libhackrf`
Damit KAT diesen File Descriptor nutzen kann, muss die zugrunde liegende C-Bibliothek `libhackrf` (oder eine alternative Rust-USB-Bibliothek wie `nusb`) diesen FD verarbeiten können.

* **C-Ebene (`libusb`):** Normalerweise nutzt man `libusb_open()`. Unter Android/Termux muss stattdessen `libusb_wrap_sys_device(context, fd, &handle)` verwendet werden, wobei `fd` der numerische File Descriptor ist, den Android übergeben hat.
* **Rust-Ebene (`KAT`):** KAT verwendet aktuell einen eigenen Wrapper in `vendor/libhackrf`. Dieser Wrapper (und potenziell die kompilierte C-Library `libhackrf.so` in Termux) muss erweitert werden, um eine Funktion wie `hackrf_open_by_fd(int fd)` anzubieten.

**Alternative Architektur (Pure Rust):**
Es sollte geprüft werden, ob `vendor/libhackrf` durch reine Rust-Crates wie `waverave-hackrf` ersetzt werden kann. Diese basieren auf `nusb`, was möglicherweise bereits bessere Mechanismen für das Einreichen von Android File Descriptors bietet, ohne dass C-Code (`libhackrf.c`) neu kompiliert oder gepatcht werden muss.

---

## 3. Kompilierung (Cross-Compilation & Android NDK)

Es gibt zwei Wege, KAT für Android zu kompilieren:

### Weg A: Direkt auf dem Android-Gerät in Termux (Einfacher)
Da Rust in Termux via `pkg install rust` verfügbar ist, kann der Code direkt auf dem Handy via `cargo build --release` gebaut werden.
* **Vorteil:** Keine Cross-Compilation-Probleme, verhältnismäßig einfaches Setup.
* **Nachteil:** Langsam, erfordert dass der gesamte Sourcecode auf das Telefon geladen wird.

### Weg B: Cross-Compilation auf dem PC (Für CI/CD & Releases)
* **Ziel:** `aarch64-linux-android` (oder spezifische Termux-Targets).
* **Setup:** Installation des Android NDK.
* **Cargo-Konfiguration:** Die `~/.cargo/config.toml` muss den Linker auf die clang-Toolchain des Android NDKs verweisen.
* **C-Abhängigkeiten:** Da KAT (aktuell) `libhackrf` als C-Abhängigkeit hat, muss auch diese per CMake mit der Android NDK Toolchain für ARM64 cross-kompiliert werden.

---

## 4. Anforderungen an zukünftige AI-Agents (Skills & Kontext)

Damit AI-Agents diesen Plan fehlerfrei und ohne Halluzinationen umsetzen können, **müssen** sie über folgendes Domänenwissen verfügen und strikt angewiesen werden, nur faktenbasierten Code zu generieren:

### 4.1. Hardware & USB-Interaktion
* **HackRF One:** Verständnis der Architektur (RX/TX-Modes, Sample Rates, Baseband Filter, LNA/VGA Gains).
* **libusb & Android FFI:** Genaues Wissen über `libusb_wrap_sys_device`, das Übergeben von Filedescriptoren in Unix/Android-Systemen und das Handling von asynchronen USB-Transfers.
* **Termux API:** Wissen, wie `termux-usb` aufgerufen und geparst wird.

### 4.2. Funkprotokolle & Signalverarbeitung (SDR)
* **SDR Grundlagen:** Software Defined Radio, I/Q-Daten, Sampling-Theoreme.
* **Modulationsarten:** ASK (Amplitude Shift Keying), OOK (On-Off Keying), FSK (Frequency Shift Keying).
* **Lineare Codierung:** Manchester-Codierung (Differential, Standard), PWM (Pulse Width Modulation).

### 4.3. Verschlüsselung & Rolling Codes
* **KeeLoq:** Verständnis der Funktionsweise des Microchip KeeLoq Algorithmus (Non-Linear Feedback Shift Register - NLFSR), Seed-Generierung, HCS300/301 Decoder.
* **Kryptographie:** AES (Advanced Encryption Standard), Lineare und nicht-lineare Schieberegister (LFSR/NLFSR), Prüfsummen (CRC8/CRC16).

### 4.4. Softwareentwicklung & Rust
* **Rust FFI (Foreign Function Interface):** Sicheres Einbinden von C-Code (`libhackrf.h`) und Umgang mit `unsafe` Code, Pointern und Speichersicherheit in C.
* **Cross-Compilation:** Handhabung des Android NDK, `rustup target add aarch64-linux-android` und Konfiguration von Cargo Build-Scripts (`build.rs`).
* **Ratatui:** Umgang mit der TUI-Bibliothek von KAT, um sicherzustellen, dass die Benutzeroberfläche unter Termux-Bedingungen (Touch-Tastatur, ggf. fehlende Modifier-Tasten) nutzbar bleibt.

---

## 5. Konkrete Implementierungsschritte (Roadmap)

1. **Proof of Concept (Termux USB):**
   Schreibe ein winziges C- oder Rust-Programm, das in Termux läuft, einen File-Descriptor von `termux-usb -e` entgegennimmt, `libusb_wrap_sys_device` aufruft und die Seriennummer des HackRF ausliest.
2. **KAT USB-Backend anpassen:**
   Modifiziere `vendor/libhackrf` (oder wechsle zu einem pure-Rust Backend), sodass es den Android-FD akzeptieren kann, anstatt `hackrf_open` (welches USB-Discovery versucht und auf Android fehlschlägt) zu verwenden.
3. **Kompilierungs-Pipeline aufsetzen:**
   Sicherstellen, dass KAT über `cargo build` direkt in Termux fehlerfrei kompiliert. Wenn C-Abhängigkeiten Probleme machen, müssen diese lokal in Termux gebaut werden.
4. **Integrationstest:**
   Starte KAT in Termux, fange ein einfaches Manchester-moduliertes Signal auf (z.B. eine Steckdose) und validiere, ob die bestehenden Decoder (siehe 4.2 und 4.3) unter ARM64 korrekt funktionieren.
