use std::io::{stdout, BufWriter};

use clap::Parser;
use rnet_gen::*;
fn main() -> anyhow::Result<()> {
    let opt = Args::parse();
    let stdout = stdout();
    let mut writer = BufWriter::new(stdout.lock());
    gen(opt, &mut writer)?;
    eprintln!("Done.");
    Ok(())
}
