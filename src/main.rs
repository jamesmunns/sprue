#![recursion_limit = "1024"]
#![allow(intra_doc_link_resolution_failure)]
extern crate getopts;
extern crate quote;
extern crate syn;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate failure;

use log::LevelFilter;
use quote::ToTokens;
use syn::fold::{noop_fold_crate, noop_fold_item, Folder};
use syn::{Crate, Ident, Item, ItemKind};

use std::fs::{DirBuilder, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::cell::RefCell;

mod opts;
use failure::*;
use opts::FormOpts;

const WORKSPACE_NAME: &str = "demo";

fn main() {
    match run() {
        Ok(()) => println!("Completed successfully"),
        Err(error) => println!("Failed with:\n {}", error),
    }
}

fn run() -> Result<(), Error> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Trace)
        .try_init()
        .context("could not initialise env_logger")?;

    trace!("logging initialised");
    let try_parsed_args =
        FormOpts::from_args().context("could not parse the command line arguments")?;
    // if None, we've already printed a help text and have nothing more to do
    if let Some(opts) = try_parsed_args {
        create_directory_structure(opts.output_dir, opts.input)?;
    }
    return Ok(());
}

fn create_directory_structure<P: AsRef<Path>>(
    base_dir: P,
    string_contents: String,
) -> Result<(), Error> {
    info!("Started parsing the input as Rust. This can take a minute or two.");
    let parsed_crate = syn::parse_crate(&string_contents)
        .map_err(err_msg)
        .context("failed to parse crate")?;

    info!("Finished parsing");

    let base_dir = base_dir.as_ref();
    make_dir(base_dir)?;
    info!("Prepared target directory {}", base_dir.display());

    let crates = RefCell::new(Vec::new());

    let mut folder = FileIntoMods {
        current_dir: &base_dir,
        top_level: true,
        all_crates: crates,
    };

    // Why doesn't syn::Fold::fold handle errors again?
    // TODO: catch panics?
    let new_contents = folder.fold_crate(parsed_crate);
    trace!("transformed module contents");

    // AJM - create base crate
    // TODO - we should probably take "base" here as an option, and use
    // it to generate all crates name in the form "base-{crate}"
    {
        let dir_name = base_dir.join("base").join("src");

        make_dir(&dir_name)?;

        let lib_file_path = dir_name.join("lib.rs");
        debug!("Writing to file {}", lib_file_path.display());
        write_all_tokens(&new_contents, &lib_file_path)?;

        // wut
        folder.all_crates.get_mut().push(format!("{}", "base"));
    }

    // TODO: Create cargo workspace
    let mut workspace_contents = String::new();
    workspace_contents += "[workspace]\n";
    workspace_contents += "members = [\n";

    let lol_crates = folder.all_crates.borrow().clone();

    for craat in folder.all_crates.borrow().iter() {
        workspace_contents += &format!("    \"{}\",\n", craat);

        let mut craat_toml = String::new();
        craat_toml += "[package]\n";
        craat_toml += &format!("name = \"{}\"\n", craat);
        craat_toml += "version = \"0.1.0\"\n";
        craat_toml += "authors = [\"James Munns <james@onevariable.com>\"]\n";

        // TODO AJM - this should go in the "super" crate, not the "base" crate
        // if craat == "base" {
        //     craat_toml += "\n";
        //     craat_toml += "[dependencies]\n";

        //     for craat_2 in lol_crates.iter().filter(|&f| f != craat ) {
        //         craat_toml += &format!("{crt} = {{ path = \"../{crt}\" }}\n", crt = craat_2);
        //     }
        // }

        just_write_string(
            &craat_toml,
            &base_dir.join(craat).join("Cargo.toml")
        )?;
    }

    workspace_contents += "]\n";

    just_write_string(
        &workspace_contents,
        &base_dir.join("Cargo.toml"),
    )?;


    Ok(())
}

#[derive(Debug)]
struct FileIntoMods<P: AsRef<Path> + Send + Sync> {
    current_dir: P,
    top_level: bool,
    all_crates: RefCell<Vec<String>>,
}

impl<P: AsRef<Path> + Send + Sync> FileIntoMods<P> {
    fn sub_mod<Q: AsRef<Path>>(&self, path: Q) -> FileIntoMods<PathBuf> {
        let mut cd = self.current_dir.as_ref().join(path);

        if self.top_level {
            cd = cd.join("src")
        }

        FileIntoMods {
            current_dir: cd,
            top_level: false,
            all_crates: self.all_crates.clone()
        }
    }
}

fn make_dir<P: AsRef<Path> + Send + Sync>(dir_name: P) -> Result<(), Error> {
    let mut dir_builder = DirBuilder::new();
    info!("Creating directory {}", dir_name.as_ref().display());
    dir_builder
        .recursive(true)
        .create(dir_name.as_ref())
        .context(format_err!(
            "building {} failed",
            dir_name.as_ref().display()
        ))?;

    Ok(())
}

impl<P: AsRef<Path> + Send + Sync> FileIntoMods<P> {
    fn fold_sub_crate(&mut self, crate_name: &Ident, rust_crate: Crate) -> Result<(), Error> {
        trace!(
            "Folding over module {} - submod? - {}",
            crate_name,
            !self.top_level
        );

        // AJM - todo dedupe code here
        let dir_name = &if self.top_level {
            self.current_dir
                .as_ref()
                .join(crate_name.as_ref())
                .join("src")
        } else {
            self.current_dir.as_ref().join(crate_name.as_ref())
        };

        make_dir(dir_name)?;

        // AJM - make a Cargo.toml for the crate somewhere around here

        let mut sub_self = self.sub_mod(crate_name.as_ref());
        let folded_crate = noop_fold_crate(&mut sub_self, rust_crate);
        trace!(
            "Writing contents of module {} to file {}",
            crate_name,
            dir_name.display()
        );

        if self.top_level {
            self.all_crates.get_mut().push(format!("{}", crate_name));
            write_crate_crate(folded_crate, &dir_name).context(format_err!(
                "writing to {}/lib.rs failed",
                dir_name.display()
            ))?;
        } else {
            write_crate_mod(folded_crate, &dir_name).context(format_err!(
                "writing to {}/mod.rs failed",
                dir_name.display()
            ))?;
        }

        Ok(())
    }
}

impl<P: AsRef<Path> + Send + Sync> Folder for FileIntoMods<P> {
    fn fold_item(&mut self, mut item: Item) -> Item {
        for rust_crate in extract_crate_from_mod(&mut item.node) {
            self.fold_sub_crate(&item.ident, rust_crate).unwrap();
        }
        noop_fold_item(self, item)
    }
}

fn write_crate_mod<P: AsRef<Path>>(rust_crate: Crate, dir_name: &P) -> Result<(), Error> {
    let file_name = dir_name.as_ref().join("mod.rs");
    write_all_tokens(&rust_crate, &file_name)
}

fn write_crate_crate<P: AsRef<Path>>(rust_crate: Crate, dir_name: &P) -> Result<(), Error> {
    let file_name = dir_name.as_ref().join("lib.rs");
    write_all_tokens(&rust_crate, &file_name)
}

fn extract_crate_from_mod<'a>(node: &'a mut ItemKind) -> Option<Crate> {
    if let ItemKind::Mod(ref mut maybe_items) = *node {
        maybe_items.take().map(make_crate)
    } else {
        None
    }
}

fn make_crate(items: Vec<Item>) -> Crate {
    Crate {
        shebang: None,
        attrs: vec![],
        items,
    }
}

fn write_all_tokens<T: ToTokens>(piece: &T, path: &Path) -> Result<(), Error> {
    let mut new_tokens = quote::Tokens::new();
    piece.to_tokens(&mut new_tokens);
    let string = new_tokens.into_string();
    trace!("Written string for tokens, now writing");

    just_write_string(&string, path)?;

    Ok(())
}

fn just_write_string(contents: &str, path: &Path) -> Result<(), Error> {
    info!("creating file {}", path.display());
    let mut file = File::create(path)
        .context(format_err!("Failed to create file: {:?}", path))?;

    trace!("writing file {}", path.display());
    let _ = file.write_all(contents.as_bytes())
        .context("Failed to write to file")?;

    Ok(())
}