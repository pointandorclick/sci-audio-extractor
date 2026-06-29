# SCI Audio Extractor

A command-line tool that extracts sound resources from Sierra SCI games and renders them to audio files using an embedded Roland CM-32L synthesizer emulator.

Sierra's SCI1/SCI1.1 games stored music as MIDI data targeting the Roland MT-32/CM-32L sound module. This tool parses the game's resource files, extracts the MIDI sound data, synthesizes it through an emulated CM-32L, and encodes the output as OGG Vorbis or WAV.

This tool was created to aid in the creation of [Sierra Quest](https://pointandclick.studio/games/sierra-quest) a fan-mdae point and click Sierra adventure game.

## Requirements

### CM-32L ROM Files

The CM-32L emulator requires original ROM dumps to function. You need two files:

- `CM32L_CONTROL.ROM` (~65 KB)
- `CM32L_PCM.ROM` (~1 MB)

Place them in a `roms/` directory next to the binary (the default), or specify a path with `--rom-dir`.

The tool searches for ROM filenames case-insensitively and also matches patterns like `cm32l_ctrl_*.rom` and `cm32l_pcm.rom`.

### Building

Requires Rust 1.85+ (2024 edition).

```
cargo build --release
```

The compiled binary will be at `target/release/sci-audio-extractor`.

## Usage

```
sci-audio-extractor [OPTIONS] <GAME_DIR> [OUTPUT_DIR]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `GAME_DIR` | Path to a Sierra SCI game directory containing `resource.map` |
| `OUTPUT_DIR` | Output directory for audio files (default: `output`) |

### Options

| Option | Description |
|--------|-------------|
| `--rom-dir <DIR>` | Directory containing CM-32L ROM files (default: `roms`) |
| `-r, --resource <N>` | Extract specific sound resource numbers, comma-separated |
| `--quality <Q>` | OGG Vorbis quality, 0.0 to 1.0 (default: 0.6) |
| `--format <FMT>` | Output format: `ogg` or `wav` (default: `ogg`) |
| `--list` | List all sound resources without extracting |
| `-v, --verbose` | Verbose output |

### Examples

List all sound resources in a game:

```
sci-audio-extractor --list ~/Games/KQ5
```

Extract all sounds to OGG:

```
sci-audio-extractor ~/Games/KQ5 ./kq5-music
```

Extract specific sounds:

```
sci-audio-extractor -r 1,2,3 ~/Games/KQ5 ./kq5-music
```

Extract as WAV at high quality:

```
sci-audio-extractor --format wav ~/Games/KQ5 ./kq5-music
```

Use ROMs from a custom location:

```
sci-audio-extractor --rom-dir /path/to/roms ~/Games/KQ5
```

## Supported Games

The tool supports SCI1 and SCI1.1 games with MT-32/CM-32L sound tracks. Tested with:

| Game | SCI Version | Compression |
|------|-------------|-------------|
| King's Quest V | SCI1 | None |
| Space Quest I VGA | SCI1 | LZW1 |
| Conquests of the Longbow | SCI1 | LZW1 |
| Police Quest 3 | SCI1 | LZW1 |
| Leisure Suit Larry 5 | SCI1 | LZW1 |
| Police Quest 1 VGA | SCI1 | LZW1 |
| Quest for Glory I VGA | SCI1 | DCL |
| King's Quest VI CD | SCI1.1 | DCL |
| Space Quest IV | SCI1.1 | DCL |
| Leisure Suit Larry 6 | SCI1.1 | DCL |
| Space Quest V | SCI1.1 | DCL |
| Quest for Glory III | SCI1.1 | DCL |

## How It Works

1. **Resource map parsing** - Reads `resource.map` and auto-detects whether the game uses SCI1 (6-byte map entries) or SCI1.1 (5-byte entries with bit-shifted offsets).

2. **Volume reading** - Opens the corresponding `resource.nnn` volume file and reads the 9-byte resource header to determine compression method and sizes.

3. **Decompression** - Supports three methods:
   - Method 0: No compression
   - Method 2: Sierra's custom LZW1 variant
   - Method 18: PKWARE DCL (Data Compression Library)

4. **Sound resource parsing** - Parses the track/channel structure of the sound resource and locates the MT-32 track (device type 0x00).

5. **MIDI extraction** - Decodes Sierra's modified MIDI format from channel data, handling custom features like 0xF8 delay markers and running status.

6. **CM-32L synthesis** - Feeds MIDI events to an emulated CM-32L synthesizer ([moont](https://crates.io/crates/moont)) and renders stereo 32kHz PCM audio. Each sound gets a fresh synthesizer instance.

7. **Encoding** - Encodes the PCM output as OGG Vorbis (via [vorbis_rs](https://crates.io/crates/vorbis_rs)) or writes raw WAV.

## Output

Files are named `sound_NNN.ogg` (or `.wav`) where NNN is the zero-padded resource number. Output at default quality (0.6) typically produces files of 50 KB to 4 MB depending on track length.

## License

MIT
