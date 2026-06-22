# NOTE
  The flatpak is currently not working so use the Release Binary method for now.

# TUIplay

A fast, keyboard-driven terminal music player and library manager.  Browse your
collection, queue up tracks, edit metadata tags, save playlists, and burn audio
CDs — all without leaving the command line.

![Platform](https://img.shields.io/badge/platform-Linux-brightgreen)
![Language](https://img.shields.io/badge/language-Rust-orange)

---

## Features

- **Multi-format playback** — Opus, MP3, FLAC, WAV, AAC/M4A, Vorbis/OGG
- **Queue & playlists** — build a play queue, save/load named playlists (JSON)
- **Inline metadata editor** — edit title, artist, album, genre tags; writes back
  to the audio files and persists in the database
- **Resume playback** — stop a track and pick it up later from the same position
- **CD image export** — decode any queue to 44.1 kHz / 16-bit stereo WAVs +
  a CUE sheet ready for burning
- **Direct CD burning** — burn the exported image straight to a CD-R with
  `cdrdao` or `wodim`
- **Copy to folder** — copy queue tracks to a USB key or any directory
- **Vim-style navigation** — `j`/`k` and arrow keys
- **Split-panel UI** — library on the left, queue on the right; `Tab` to switch

---

## Compatibility Matrix

| Feature                    | Opus (.opus) | MP3 (.mp3) | FLAC (.flac) | WAV (.wav) | AAC/M4A (.m4a) | Vorbis (.ogg) |
|----------------------------|:-----------:|:----------:|:------------:|:----------:|:--------------:|:-------------:|
| Playback                   | ✅          | ✅         | ✅           | ✅         | ✅             | ✅            |
| Metadata read              | ✅          | ✅         | ✅           | ✅         | ✅             | ✅            |
| Metadata write             | ✅          | ✅         | ✅           | ⚠️¹        | ✅             | ✅            |
| Duration detection         | ✅          | ✅         | ✅           | ✅         | ✅             | ✅            |
| Seek / resume              | ✅          | ✅         | ✅           | ✅         | ✅             | ✅            |
| CD export (→ WAV)          | ✅          | ✅         | ✅           | ✅         | ✅             | ✅            |

¹ WAV metadata writing depends on the presence of an existing INFO chunk; most
WAV files exported by other tools have one.

### Operating Systems

| Platform      | Status                              |
|---------------|-------------------------------------|
| **Linux**     | Full support (Flatpak & source)     |
| **macOS**     | Unknown  |
| **Windows**   | Not planned             |

---

## Installation

### Download a release binary (simplest)

Grab the latest `tuiplay` binary from the
[Releases](https://github.com/jacobjuneau6/TUIplay/releases) page, make it
executable, and run it.

```bash
curl -LO https://github.com/malagatech/tuiplay/releases/latest/download/tuiplay
chmod +x tuiplay
./tuiplay
```

**System dependencies** (pre-installed on most desktops):

| Package          | Arch                    | Debian/Ubuntu              | Fedora                  |
|------------------|-------------------------|----------------------------|-------------------------|
| ALSA / PipeWire  | `pipewire` (default)    | `libasound2`               | `pipewire-alsa`         |
| Opus (runtime)   | `opus`                  | `libopus0`                 | `opus`                  |
| CD burning       | `cdrdao` (optional)     | `cdrdao` (optional)        | `cdrdao` (optional)     |

### Flatpak (sandboxed)

Download the `.flatpakref` file from the
[Releases](https://github.com/jacobjuneau6/TUIplay/releases) page and install it:

```bash
flatpak install --from https://github.com/jacobjuneau6/TUIplay/releases/latest/download/com.malagatech.tuiplay.flatpakref
flatpak run com.malagatech.tuiplay
```

Or grab the `.flatpak` bundle and side-load it:

```bash
curl -LO https://github.com/jacobjuneau6/TUIplay/releases/latest/download/tuiplay.flatpak
flatpak install --bundle tuiplay.flatpak
```

The Flatpak sandbox has PulseAudio access for playback and `--filesystem=home`
so it can see your Music folder.  CD burning requires `cdrdao` on the host
(flatpak calls through to it).

### Build from source

```bash
git clone https://github.com/malagatech/tuiplay.git
cd tuiplay
cargo build --release
./target/release/tuiplay
```

**Build dependencies:** Rust 1.70+, `libasound2-dev` (or `alsa-lib-devel` on
Fedora), `libopus-dev`.

---

## Usage

Launch the program from a terminal:

```bash
tuiplay
```

TUIplay scans `/home/$USER/Music` on startup and shows every supported audio
file it finds.  The database (`music.db`) is created automatically in that
folder.

### Keyboard shortcuts

#### Navigation

| Key            | Action                         |
|----------------|--------------------------------|
| `↑` / `↓`      | Move selection up / down       |
| `j` / `k`      | Move selection (vim bindings)  |
| `Tab`          | Switch Library ↔ Queue focus   |

#### Playback

| Key            | Action                                   |
|----------------|------------------------------------------|
| `Enter`        | Play selected track                      |
| `Space`        | Pause / resume                           |
| `s`            | Stop (remembers position for resume)     |
| `n`            | Play next track from the queue (FIFO)    |

#### Queue

| Key            | Action                                   |
|----------------|------------------------------------------|
| `a`            | Add selected library track to queue      |
| `A`            | Add all library tracks to queue          |
| `d`            | Remove selected item from queue          |
| `C`            | Clear the entire queue                   |

#### Playlists

| Key            | Action                                   |
|----------------|------------------------------------------|
| `S`            | Save queue as a named playlist (prompt)  |
| `L`            | Load a named playlist into the queue     |

Playlists are stored as `<name>.playlist.json` in your Music folder.

#### Metadata editing

| Key (in editor)       | Action                         |
|------------------------|--------------------------------|
| `e`                    | Open metadata editor           |
| `Tab` / `↓`            | Next field                     |
| `Shift+Tab` / `↑`      | Previous field                 |
| Type, `Backspace`, etc.| Edit the focused field         |
| `Enter`                | Save changes to file + DB      |
| `Esc`                  | Cancel (discard changes)       |

#### CD burning & file copy

| Key            | Action                                          |
|----------------|--------------------------------------------------|
| `x`            | Export queue as CD image (prompt for name)       |
| `b`            | Burn the last CD export to a CD-R                |
| `c`            | Copy queue tracks to a folder (prompt for path)  |

CD exports are written to `~/Music/cd_<name>/` and contain numbered `.wav`
files plus a `disc.cue` sheet.

#### Quit

| Key            | Action     |
|----------------|------------|
| `q`            | Quit       |

### Workflow example

```
1.  Start tuiplay — your library appears on the left.
2.  Press a (add) on a few tracks — they appear in the Queue on the right.
3.  Press Enter to start playing.  Space pauses, s stops.
4.  Press S, type "road-trip", press Enter — queue is saved as a playlist.
5.  Next session: press L, type "road-trip" — queue is restored.
6.  Press x, type "car-mix" — WAV + CUE files are generated.
7.  Insert a blank CD-R and press b — the disc is burned.
```

---

## Configuration

TUIplay looks for music in `/home/$USER/Music`.  To change this, edit the path
in `src/main.rs` and rebuild, or set up a symlink:

```bash
ln -s /path/to/your/music ~/Music
```

---

## Building the Flatpak

These steps produce the `.flatpakref` and `.flatpak` files that are uploaded to
GitHub Releases.  End users don't need to run these — they download the
pre-built artifacts.

### 1. Install the GNOME SDK

```bash
flatpak install org.gnome.Sdk//46 org.gnome.Platform//46
```

### 2. Generate the Cargo dependency manifest

```bash
# Install the helper (one-time)
pip install flatpak-cargo-generator

# Generate from the Cargo.lock
python3 -m flatpak_cargo_generator Cargo.lock -o flatpak/generated-sources.json
```

### 3. Build the Flatpak

```bash
flatpak-builder --user --install --force-clean \
    build-dir flatpak/com.malagatech.tuiplay.yml
```

### 4. Create the release artifacts

```bash
# .flatpak bundle (users can side-load this)
flatpak build-bundle \
    ~/.local/share/flatpak/repo \
    tuiplay.flatpak \
    com.malagatech.tuiplay \
    stable

# Copy the .flatpakref to the release directory
cp flatpak/com.malagatech.tuiplay.flatpakref .
```



## License

GPLv3 — see [LICENSE](LICENSE) for details.

---

## Contributing

Issues and pull requests are welcome.  Please open an issue first to discuss
what you'd like to change.

For local development:

```bash
git clone https://github.com/malagatech/tuiplay.git
cd tuiplay
cargo run
```

The project uses the Rust 2015 edition.  `cargo fmt` and `cargo clippy` are
appreciated on PRs.
