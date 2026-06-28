// ucw — Uten Core Window launcher. Same as uc but no console window on Windows.
// On Unix, identical to uc.

#![cfg_attr(windows, windows_subsystem = "windows")]

use clap::Parser;

#[derive(Parser)]
#[command(name = "ucw", version, about = "Uten Core (windowed)")]
struct Args {
    file: std::path::PathBuf,
    #[arg(long)]
    debug: bool,
    #[arg(long, short)]
    quiet: bool,
}

fn main() {
    let args = Args::parse();
    uc_binaries::setup_logging(args.debug, args.quiet);
    let pm = uc_binaries::register_plugins();

    let path = &args.file;
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "uclib" | "ucch" => uc_binaries::run_module(uc_binaries::load_module(path)),
        ext => {
            if pm.can_compile(ext) {
                uc_binaries::compile_and_run(path, &pm);
            } else {
                eprintln!("error: no compiler for '.{ext}'");
                std::process::exit(1);
            }
        }
    }
}
