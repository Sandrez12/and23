use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;

const USAGE: &str = "\
And23 (M0)

Uso:
  and23 <archivo.jb>   ejecuta un programa
  and23                abre el REPL (escribe :q o Ctrl+Z para salir)
  and23 --version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => repl(),
        [flag] if flag == "--version" || flag == "-V" => {
            println!("and23 {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        [path] => run_file(path),
        _ => {
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Analiza y ejecuta `src`. El error ya viene formateado con su posición.
fn execute(src: &str, file: &str, echo: bool, out: &mut dyn Write) -> Result<(), String> {
    let program = and23_syntax::parse(src).map_err(|d| d.render(file, src))?;
    and23_eval::run(&program, echo, out).map_err(|d| d.render(file, src))
}

fn run_file(path: &str) -> ExitCode {
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) => {
            eprintln!("error: no se pudo leer `{path}`: {e}");
            return ExitCode::from(1);
        }
    };
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match execute(&src, path, false, &mut out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(rendered) => {
            let _ = out.flush();
            eprint!("{rendered}");
            ExitCode::from(1)
        }
    }
}

fn repl() -> ExitCode {
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut out = io::stdout();
    loop {
        if interactive {
            print!("and23> ");
            let _ = out.flush();
        }
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("error: no se pudo leer la entrada: {e}");
                return ExitCode::from(1);
            }
        }
        if line.trim() == ":q" {
            break;
        }
        if let Err(rendered) = execute(&line, "<repl>", true, &mut out) {
            eprint!("{rendered}");
        }
    }
    ExitCode::SUCCESS
}
