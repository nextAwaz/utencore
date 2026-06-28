// ucc — Uten Core Compiler. Compiles source → .uclib.
//
// Usage: ucc [options] <source.py>
//        ucc -o output.uclib source.py

use std::path::PathBuf;
use clap::Parser;

#[derive(Parser)]
#[command(name = "ucc", version, about = "Uten Core Compiler")]
struct Args {
    source: PathBuf,
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(long)]
    debug: bool,
    #[arg(long, short)]
    quiet: bool,
}

fn main() {
    let args = Args::parse();
    uc_binaries::setup_logging(args.debug, args.quiet);
    let pm = uc_binaries::register_plugins();

    let source = uc_binaries::read_source(&args.source);
    let ext = args.source.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !pm.can_compile(ext) {
        eprintln!("error: no compiler for '.{ext}'");
        std::process::exit(1);
    }

    let out = args.output.unwrap_or_else(|| args.source.with_extension("uclib"));
    let out_dir = out.parent().unwrap_or(std::path::Path::new("."));
    let module = uc_binaries::compile_with_deps(&pm, &source, &args.source.to_string_lossy(), out_dir);
    let bytes = module.to_bytes().unwrap_or_else(|e| { eprintln!("error: serialize: {e}"); std::process::exit(1); });
    std::fs::write(&out, &bytes)
        .unwrap_or_else(|e| { eprintln!("error: {e}"); std::process::exit(1); });
}
