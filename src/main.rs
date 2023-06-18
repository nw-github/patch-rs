use clap::Parser;
use patch_rs::ups::Patch;
use std::{fs, path::PathBuf};

#[derive(Parser)]
struct Arguments {
    /// The desired ROM file to patch.
    rom: PathBuf,
    /// The UPS patch file.
    patch: PathBuf,
    /// The output file.
    dest: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Arguments::parse();
    let patch = Patch::load(&fs::read(&args.patch)?)?;
    let rom = fs::read(&args.rom)?;

    fs::write(
        args.dest.unwrap_or(args.rom.with_file_name(format!(
            "{}.{}",
            args.patch
                .file_stem()
                .unwrap()
                .to_string_lossy(),
            args.rom.extension()
                .unwrap_or(&std::ffi::OsStr::new("rom"))
                .to_string_lossy()
        ))),
        patch.apply(&rom)?,
    )?;

    Ok(())
}
