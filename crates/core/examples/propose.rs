//! Proposes a layout template for a folder, as adding a root would.
//!
//!   cargo run -p cabinet-core --example propose -- <folder> [tv|movies]

use cabinet_core::layout::{LibraryKind, propose};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = args.next().ok_or("usage: propose <folder> [tv|movies]")?;
    let kind = LibraryKind::parse(&args.next().unwrap_or_else(|| "tv".into()))
        .ok_or("kind is tv or movies")?;
    let walk = cabinet_core::scan::walk(std::path::Path::new(&root))?;
    let paths: Vec<String> = walk.files.into_iter().map(|f| f.relative_path).collect();
    let p = propose(&paths, kind);
    println!(
        "{}\n  {}\n  {} of {} videos grouped; e.g. {}",
        p.template,
        p.reason,
        p.grouped,
        p.files,
        p.examples.join(", ")
    );
    Ok(())
}
