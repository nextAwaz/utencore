// uc — Uten Core VM launcher. Runs .uclib files.
//
// Usage: uc [options] <file.uclib>
//        uc script.py          ← compile-to-cache + run

use clap::Parser;

#[derive(Parser)]
#[command(name = "uc", version, about = "Uten Core VM launcher")]
struct Args {
    file: std::path::PathBuf,
    #[arg(long)]
    debug: bool,
    #[arg(long, short)]
    quiet: bool,
}

fn main() {
    #[cfg(windows)]
    {
        extern "system" { fn SetConsoleOutputCP(code: u32) -> i32; }
        unsafe { SetConsoleOutputCP(65001); }
    }
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
