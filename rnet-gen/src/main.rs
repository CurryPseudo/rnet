use std::io::{stdout, BufWriter};

use rnet_gen::*;
use structopt::StructOpt;
fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let stdout = stdout();
    let mut writer = BufWriter::new(stdout.lock());
    gen(opt, &mut writer)?;
    eprintln!("Done.");
    Ok(())
}
