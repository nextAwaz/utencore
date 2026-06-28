// ucdump — Uten Core disassembler. Dumps .uclib / .ucch contents.
//
// Usage: ucdump [options] <file.uclib>

use std::path::PathBuf;
use clap::Parser;

#[derive(Parser)]
#[command(name = "ucdump", version, about = "Uten Core disassembler")]
struct Args {
    file: PathBuf,
    #[arg(long)]
    debug: bool,
    #[arg(long, short)]
    quiet: bool,
}

fn main() {
    let args = Args::parse();
    uc_binaries::setup_logging(args.debug, args.quiet);
    let module = uc_binaries::load_module(&args.file);
    uc_binaries::dump_module(&module);
}
