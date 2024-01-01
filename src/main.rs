mod lib {
    pub use anyhow::{Result, bail};
    pub use clap::{Arg, ArgAction, ArgMatches, Command};
    pub use core::ops::RangeInclusive;
    pub use image;
    pub use rand::prelude::*;
    pub use std::{fmt::Display, path::Path, fs::File, io::Write};
    pub use thiserror::*;
}
pub use lib::*;
mod texture;

fn main() -> Result<()> {
    let matches = Command::new("bb")
        .about("Command line interface for bb scenario")
        .version("0.0.1")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .author("cogito123")
        .subcommand(
            Command::new("lua")
                .about("Generate lua files based on input parameters")
                .subcommand(
                    Command::new("texture")
                        .about(
                            "Generate lua texture file using reference image. The image is automatically converted to grayscale."
                        )
                        .arg(
                            Arg::new("steps")
                                .short('s')
                                .long("steps")
                                .help(
                                    "Range between [0..255] that maps onto name of a factorio tile. The value is derived from grayscale. Format: $tile-name:$x..$y. Multiple space-separated steps can be provided at once. Entire [0..255] range must be covered"
                                )
                                .required(true)
                                .num_args(1..)
                        ).arg(
                            Arg::new("image")
                                .short('i')
                                .long("image")
                                .help("Path of an input image")
                                .required(true)
                        ).arg(
                            Arg::new("output")
                                .short('o')
                                .long("output")
                                .help("Path of a generated lua script")
                                .required(true)
                        ).arg(
                            Arg::new("blending")
                                .short('b')
                                .long("blending")
                                .help(
                                    "Does a blending noise pass over grayscale. This can help with blending of feature edges. Value is between 0 - 100(%). 0 disables blending pass, 20 makes smooth transitions and 100 is pure randomness"
                                )
                                .default_value("0")
                        ).arg(
                            Arg::new("seed")
                                .short('x')
                                .long("seed")
                                .help(
                                    "64 bit value that initializes PRNG, 0 - pick random seed"
                                )
                                .default_value("0")
                        )
                ).subcommand_required(true),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("lua", params)) => {
            match params.subcommand() {
                Some(("texture", params)) => texture::handle(params)?,
                Some((&_, _)) | None => unreachable!(),
            }
        },
        _ => unreachable!(),
    }

    Ok(())
}
