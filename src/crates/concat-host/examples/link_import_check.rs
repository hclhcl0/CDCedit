//! Manual smoke harness for link import; never reads the user's Drive account.
use concat_host::{
    dirs::AppDirs,
    link_import::{self, Tools},
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let root = PathBuf::from(
        args.get(2)
            .ok_or("usage: link_import_check install|inspect|download DIRECTORY [URL]")?,
    );
    let dirs = AppDirs {
        config: root.join("config"),
        data: root.join("data"),
    };
    let tools = Tools::new(&dirs);
    let cancel = AtomicBool::new(false);
    match args.get(1).map(String::as_str) {
        Some("install") => link_import::install(&tools, &cancel, &mut |p, s| {
            println!("{:.0}% {s}", p * 100.0)
        })?,
        Some("inspect") | Some("download") => {
            let items = link_import::inspect(
                &tools,
                args.get(3).ok_or("Missing URL")?,
                false,
                None,
                &cancel,
            )?;
            println!("Found {} item(s)", items.len());
            for i in &items {
                println!("{} | {:.1}s | {:?}", i.title, i.duration, i.heights);
            }
            if args[1] == "download" {
                let (files, errors, _failed) = link_import::download(
                    &tools,
                    &items,
                    &root.join("project"),
                    480,
                    false,
                    None,
                    &cancel,
                    &mut |p, s| println!("{:.0}% {s}", p * 100.0),
                )?;
                for f in files {
                    let summary = concat_host::media::probe(&f.to_string_lossy())?;
                    println!("Imported media probe: {summary:?}");
                }
                if !errors.is_empty() {
                    return Err(errors.join("\n"));
                }
            }
        }
        _ => return Err("Unknown operation".into()),
    }
    Ok(())
}
