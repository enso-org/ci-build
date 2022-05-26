#![feature(default_free_fn)]

use enso_build::prelude::*;

pub fn main() -> Result {
    enso_build::cli::main::main(default())
}
