//! Offline converter only. There is deliberately no interpreter in this CLI.
use rekursiv_smalltalk::{import_distribution, source, squeak, target, IMAGE_SHA256};
use std::{env, fs, io::BufWriter, path::PathBuf};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("Usage: rekursiv-smalltalk inspect VirtualImage\n       rekursiv-smalltalk import VirtualImage output.json\n       rekursiv-smalltalk audit VirtualImage output.json\n       rekursiv-smalltalk inspect-squeak Squeak1.1.image\n       rekursiv-smalltalk audit-squeak Squeak1.1.image output.json\n       rekursiv-smalltalk import-squeak Squeak1.1.image output.json\n\nOffline Smalltalk image tools. Squeak execution uses a separate microcode profile.\nXerox import requires SHA-256 {IMAGE_SHA256}. Squeak import requires the pinned 1.1 image. Inspect validates the format without requiring that checksum.");
        return Ok(());
    }
    if args.len() < 2
        || !(((args[0] == "inspect" || args[0] == "inspect-squeak") && args.len() == 2)
            || ((args[0] == "import"
                || args[0] == "audit"
                || args[0] == "audit-squeak"
                || args[0] == "import-squeak")
                && args.len() == 3))
    {
        return Err("use --help for inspect/import/audit syntax".into());
    }
    let bytes = fs::read(&args[1])?;
    if args[0] == "inspect-squeak" || args[0] == "audit-squeak" || args[0] == "import-squeak" {
        let source = squeak::image::Image::parse(&bytes)?;
        let inventory = squeak::audit::inventory(&source)?;
        if args[0] == "inspect-squeak" {
            println!("{}", serde_json::to_string_pretty(&inventory)?);
        } else {
            let output = PathBuf::from(&args[2]);
            if output.exists() && fs::canonicalize(&args[1])? == fs::canonicalize(&output)? {
                return Err("output must not overwrite the source image".into());
            }
            let converted = if args[0] == "import-squeak" {
                if source.sha256 != squeak::IMAGE_SHA256 {
                    return Err("Squeak import requires the pinned 1.1 image".into());
                }
                Some(squeak::target::Image::convert(&source)?)
            } else {
                None
            };
            let mut writer = BufWriter::new(fs::File::create(&output)?);
            if let Some(converted) = converted {
                serde_json::to_writer_pretty(&mut writer, &converted)?;
            } else {
                serde_json::to_writer_pretty(&mut writer, &inventory)?;
            }
            std::io::Write::flush(&mut writer)?;
            println!(
                "Validated {} objects and {} methods; wrote Squeak {} to {}",
                inventory.objects,
                inventory.methods,
                if args[0] == "import-squeak" {
                    "records"
                } else {
                    "inventory"
                },
                output.display()
            );
        }
        return Ok(());
    }
    let source = source::Image::parse(&bytes)?;
    let mut counts = [0usize; 4];
    for object in source.objects.values() {
        counts[match object.body {
            source::Body::Pointers(_) => 0,
            source::Body::Words(_) => 1,
            source::Body::Bytes(_) => 2,
            source::Body::Method { .. } => 3,
        }] += 1;
    }
    println!("SHA-256: {}\nObjects: {} ({} pointer, {} word, {} byte, {} method)\nObject space: {} words; object table: {} words", source.sha256, source.objects.len(), counts[0], counts[1], counts[2], counts[3], source.object_space_words, source.object_table_words);
    if args[0] == "inspect" {
        return Ok(());
    }
    let output = PathBuf::from(&args[2]);
    if output.exists() && fs::canonicalize(&args[1])? == fs::canonicalize(&output)? {
        return Err("output must not overwrite the source image".into());
    }
    if args[0] == "audit" {
        let inventory = rekursiv_smalltalk::audit::inventory(&source)?;
        let mut writer = BufWriter::new(fs::File::create(&output)?);
        serde_json::to_writer_pretty(&mut writer, &inventory)?;
        std::io::Write::flush(&mut writer)?;
        println!(
            "Wrote primitive declarations and dictionary bindings to {}",
            output.display()
        );
        return Ok(());
    }
    let image: target::Image = import_distribution(&bytes)?;
    let mut writer = BufWriter::new(fs::File::create(&output)?);
    image.write_json(&mut writer)?;
    std::io::Write::flush(&mut writer)?;
    println!("Converted and checked every object and GC edge.\nPhysical body words: {}\nInitial context: 0x{:04x}\nNext identity: {}\nPrimitive inventory: {:?}\nWrote {}", image.records.iter().map(|r| r.body.len()).sum::<usize>(), source.initial_context()?, image.next_identity, image.primitives, output.display());
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("rekursiv-smalltalk: {error}");
        std::process::exit(1);
    }
}
