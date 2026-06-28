use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

use sci_audio_extractor::encode;
use sci_audio_extractor::resource::map::ResourceMap;
use sci_audio_extractor::resource::volume;
use sci_audio_extractor::sound::parser;
use sci_audio_extractor::synth;
use sci_audio_extractor::synth::mt32_patch::{self, Mt32PatchData};

#[derive(Parser, Debug)]
#[command(name = "sci-audio-extractor")]
#[command(about = "Extract and render MIDI audio from Sierra SCI games using CM-32L emulation")]
struct Args {
    /// Path to Sierra SCI game directory containing resource.map
    game_dir: PathBuf,

    /// Output directory for audio files
    #[arg(default_value = "output")]
    output_dir: PathBuf,

    /// Directory containing CM32L_CONTROL.ROM and CM32L_PCM.ROM
    #[arg(long, default_value = "roms")]
    rom_dir: PathBuf,

    /// Extract only specific sound resource number(s), comma-separated or repeated
    #[arg(long = "resource", short = 'r', value_delimiter = ',', num_args = 1)]
    resources: Vec<u16>,

    /// OGG Vorbis quality 0.0-1.0
    #[arg(long, default_value = "0.6")]
    quality: f32,

    /// Output format: ogg or wav
    #[arg(long, default_value = "ogg")]
    format: String,

    /// List all sound resources without extracting
    #[arg(long)]
    list: bool,

    /// Verbose output
    #[arg(long, short)]
    verbose: bool,
}

fn find_resource_map(game_dir: &Path) -> Result<PathBuf> {
    // Case-insensitive search for resource.map
    if let Ok(entries) = fs::read_dir(game_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.eq_ignore_ascii_case("resource.map") {
                return Ok(entry.path());
            }
        }
    }

    // Check subdirectories (e.g., LB2 has files in GAME/)
    if let Ok(entries) = fs::read_dir(game_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let subdir = entry.path();
                if let Ok(sub_entries) = fs::read_dir(&subdir) {
                    for sub_entry in sub_entries.flatten() {
                        let name = sub_entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.eq_ignore_ascii_case("resource.map") {
                            return Ok(sub_entry.path());
                        }
                    }
                }
            }
        }
    }

    anyhow::bail!("resource.map not found in {}", game_dir.display());
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Find and parse resource map
    let map_path = find_resource_map(&args.game_dir)?;
    let game_dir = map_path.parent().unwrap();

    if args.verbose {
        eprintln!("Found resource map: {}", map_path.display());
    }

    let resource_map =
        ResourceMap::parse(&map_path).context("Failed to parse resource map")?;

    if args.verbose {
        eprintln!(
            "SCI version: {:?}, total entries: {}",
            resource_map.version,
            resource_map.entries.len()
        );
    }

    let sound_entries = resource_map.sound_entries();

    if args.list {
        println!("Found {} sound resources:", sound_entries.len());
        for entry in &sound_entries {
            println!(
                "  Sound {:>4}  vol={} offset={:#010x}",
                entry.number, entry.volume, entry.offset
            );
        }
        return Ok(());
    }

    if sound_entries.is_empty() {
        eprintln!("No sound resources found.");
        return Ok(());
    }

    // Load CM-32L ROM data
    let (control_rom, pcm_rom) =
        synth::load_rom_data(&args.rom_dir).context("Failed to load CM-32L ROMs")?;

    if args.verbose {
        eprintln!("CM-32L ROMs loaded from {}", args.rom_dir.display());
    }

    // Load MT-32 patch resource (#1) for custom instrument definitions
    let mt32_patch_data: Option<Mt32PatchData> = match resource_map.patch_entry(1) {
        Some(patch_entry) => {
            match volume::read_resource(game_dir, patch_entry, resource_map.version) {
                Ok(data) => match mt32_patch::parse_mt32_patch(&data) {
                    Ok(patch_data) => {
                        if args.verbose {
                            eprintln!(
                                "Loaded MT-32 patch: {} timbres, volume={}, extended={}",
                                patch_data.timbres.len(),
                                patch_data.volume,
                                patch_data.patches_49_96.is_some(),
                            );
                        }
                        Some(patch_data)
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to parse MT-32 patch resource: {e}");
                        None
                    }
                },
                Err(e) => {
                    if args.verbose {
                        eprintln!("No MT-32 patch resource found: {e}");
                    }
                    None
                }
            }
        }
        None => {
            if args.verbose {
                eprintln!("No MT-32 patch resource (#1) in resource map");
            }
            None
        }
    };

    // Create output directory
    fs::create_dir_all(&args.output_dir)?;

    // Filter entries if specific resources were requested
    let entries_to_process: Vec<_> = if args.resources.is_empty() {
        sound_entries
    } else {
        sound_entries
            .into_iter()
            .filter(|e| args.resources.contains(&e.number))
            .collect()
    };

    let total = entries_to_process.len();
    let mut success_count = 0;
    let mut skip_count = 0;

    for (i, entry) in entries_to_process.iter().enumerate() {
        eprint!(
            "\r[{}/{}] Processing sound {}...",
            i + 1,
            total,
            entry.number
        );

        match process_sound(
            game_dir,
            entry,
            &resource_map,
            &control_rom,
            &pcm_rom,
            mt32_patch_data.as_ref(),
            &args.output_dir,
            &args.format,
            args.quality,
            args.verbose,
        ) {
            Ok(true) => success_count += 1,
            Ok(false) => skip_count += 1,
            Err(e) => {
                eprintln!("\nWarning: Sound {}: {e:#}", entry.number);
                skip_count += 1;
            }
        }
    }

    eprintln!("\nDone: {success_count} extracted, {skip_count} skipped");

    Ok(())
}

fn process_sound(
    game_dir: &Path,
    entry: &sci_audio_extractor::resource::ResourceEntry,
    resource_map: &ResourceMap,
    control_rom: &[u8],
    pcm_rom: &[u8],
    patch_data: Option<&Mt32PatchData>,
    output_dir: &Path,
    format: &str,
    quality: f32,
    verbose: bool,
) -> Result<bool> {
    // Read and decompress the resource
    let data =
        volume::read_resource(game_dir, entry, resource_map.version)
            .with_context(|| format!("Failed to read sound {}", entry.number))?;

    if data.is_empty() {
        if verbose {
            eprintln!("\n  Sound {}: empty resource, skipping", entry.number);
        }
        return Ok(false);
    }

    // Parse the sound resource
    let sound = parser::parse_sound_resource(&data)
        .with_context(|| format!("Failed to parse sound {}", entry.number))?;

    if verbose {
        let track_types: Vec<String> = sound.tracks.iter().map(|t| format!("{:#04x}", t.device_type)).collect();
        eprintln!("\n  Sound {}: track types: [{}]", entry.number, track_types.join(", "));
    }

    // Find the MT-32 track
    let mt32_track = match sound.mt32_track() {
        Some(track) => track,
        None => {
            if verbose {
                eprintln!("  Sound {}: no MT-32 track, skipping", entry.number);
            }
            return Ok(false);
        }
    };

    if mt32_track.channels.is_empty() {
        if verbose {
            eprintln!("\n  Sound {}: MT-32 track has no channels, skipping", entry.number);
        }
        return Ok(false);
    }

    // Extract MIDI events
    let events = parser::extract_midi_events(mt32_track)
        .with_context(|| format!("Failed to extract MIDI from sound {}", entry.number))?;

    if events.is_empty() {
        if verbose {
            eprintln!("  Sound {}: no MIDI events, skipping", entry.number);
        }
        return Ok(false);
    }

    let note_on_count = events.iter().filter(|e| e.message.first().map(|b| b & 0xF0 == 0x90).unwrap_or(false)).count();

    if note_on_count == 0 {
        if verbose {
            eprintln!("  Sound {}: no note-on events (control-only), skipping", entry.number);
        }
        return Ok(false);
    }

    if verbose {
        let duration_secs = events.last().map(|e| e.tick as f64 / 60.0).unwrap_or(0.0);
        let sysex_count = events.iter().filter(|e| e.message.first() == Some(&0xF0)).count();
        eprintln!(
            "  Sound {}: {} events ({} note-ons, {} sysex), {:.1}s, {} channels",
            entry.number,
            events.len(),
            note_on_count,
            sysex_count,
            duration_secs,
            mt32_track.channels.len()
        );
    }

    // Render through CM-32L
    let pcm = synth::render_to_pcm_with_rom_data(control_rom, pcm_rom, &events, mt32_track, patch_data)
        .with_context(|| format!("Failed to render sound {}", entry.number))?;

    if pcm.is_empty() {
        return Ok(false);
    }

    // Encode and write output
    let extension = match format {
        "wav" => "wav",
        _ => "ogg",
    };
    let output_path = output_dir.join(format!("sound_{:03}.{}", entry.number, extension));

    match format {
        "wav" => encode::write_wav(&pcm, &output_path)?,
        _ => encode::encode_ogg(&pcm, &output_path, quality)?,
    }

    Ok(true)
}
