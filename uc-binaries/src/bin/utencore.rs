// utencore — All-in-one Uten Core CLI.
//
// Usage:
//   utencore script.py              compile + run
//   utencore script.uclib           run
//   utencore --compile script.py    compile to .uclib
//   utencore --dump script.uclib    disassemble
//   utencore                        REPL

use std::path::PathBuf;
use clap::Parser;

#[derive(Parser)]
#[command(name = "utencore", version, about = "Uten Core — universal stack-based language VM")]
struct Cli {
    file: Option<PathBuf>,
    #[arg(long)]
    compile: Option<PathBuf>,
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(long)]
    dump: bool,
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
    let cli = Cli::parse();
    uc_binaries::setup_logging(cli.debug, cli.quiet);
    let pm = uc_binaries::register_plugins();

    if let Some(ref src) = cli.compile {
        let source = uc_binaries::read_source(src);
        let bytes = uc_binaries::compile_source(&pm, &source, &src.to_string_lossy());
        let out = cli.output.clone().unwrap_or_else(|| src.with_extension("uclib"));
        std::fs::write(&out, &bytes).unwrap_or_else(|e| { eprintln!("error: {e}"); std::process::exit(1); });
        println!("{}", out.display());
        return;
    }

    if let Some(ref file) = cli.file {
        match file.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "uclib" | "ucch" => {
                if cli.dump { uc_binaries::dump_module(&uc_binaries::load_module(file)); }
                else { uc_binaries::run_module(uc_binaries::load_module(file)); }
            }
            ext => {
                if pm.can_compile(ext) { uc_binaries::compile_and_run(file, &pm); }
                else { eprintln!("error: no compiler for '.{ext}'"); std::process::exit(1); }
            }
        }
    } else {
        eprintln!("Usage: utencore <file.py> | utencore <file.uclib> | utencore --compile <file.py>");
        std::process::exit(1);
    }
}
