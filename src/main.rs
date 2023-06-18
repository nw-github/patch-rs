use anyhow::{bail, Result};
use clap::Parser;
use patch_rs::{ups::UpsPatch, Patch, ips::IpsPatch};
use std::{ffi::OsStr, fs, path::{PathBuf, Path}};

#[derive(Parser)]
struct Arguments {
    /// The ROM file to patch.
    rom: PathBuf,
    /// The UPS patch file.
    patch: PathBuf,
    /// The output file.
    dest: Option<PathBuf>,
}

fn load_patch(path: impl AsRef<Path>) -> Result<Box<dyn Patch>> {
    macro_rules! box_inner {
        ($e: expr) => {
            $e.map(|patch| Box::new(patch) as Box<dyn Patch>)
        };
    }

    let path = path.as_ref();
    let data = fs::read(path)?;
    match path.extension().map(|s| s.to_str()).flatten() {
        Some("ips") => box_inner!(IpsPatch::load(&data)),
        Some("ups") => box_inner!(UpsPatch::load(&data)),
        _ => {
            if let Ok(patch) = IpsPatch::load(&data) {
                Ok(Box::new(patch))
            } else if let Ok(patch) = UpsPatch::load(&data) {
                Ok(Box::new(patch))
            } else {
                bail!("Patch file is unsupported.");
            }
        }
    }
}

fn main() -> Result<()> {
    let args = Arguments::parse();
    let patch = load_patch(&args.patch)?;
    let rom = fs::read(&args.rom)?;

    fs::write(
        args.dest.unwrap_or_else(|| {
            args.rom
                .with_file_name(args.patch.file_stem().unwrap())
                .with_extension(args.rom.extension().unwrap_or(OsStr::new("out")))
        }),
        patch.apply(&rom)?,
    )?;

    Ok(())
}
