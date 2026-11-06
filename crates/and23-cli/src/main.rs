use and23_eval::Interpreter;
use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;

const USAGE: &str = "\
And23 (M1a)

Uso:
  and23 <archivo.jb>   ejecuta un programa (y llama a main() si existe)
  and23                abre el REPL (escribe :q o Ctrl+Z para salir)
  and23 --version
";

/// El intérprete de árbol recurre en la pila nativa; con 256 MB caben las 10 000 llamadas
/// anidadas que permite antes de dar un error en lugar de abortar el proceso.
const STACK_BYTES: usize = 256 * 1024 * 1024;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let worker = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(move || dispatch(&args));
    match worker.map(|handle| handle.join()) {
        Ok(Ok(code)) => ExitCode::from(code),
        _ => {
            eprintln!("error: falló el hilo principal del intérprete");
            ExitCode::from(1)
        }
    }
}

fn dispatch(args: &[String]) -> u8 {
    match args {
        [] => repl(),
        [flag] if flag == "--version" || flag == "-V" => {
            println!("and23 {}", env!("CARGO_PKG_VERSION"));
            0
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print!("{USAGE}");
            0
        }
        [path] => run_file(path),
        _ => {
            eprint!("{USAGE}");
            2
        }
    }
}

/// Analiza y ejecuta `src` en `interp`. El error ya viene formateado con su posición.
fn execute<W: Write>(
    interp: &mut Interpreter<W>,
    src: &str,
    file: &str,
    echo: bool,
) -> Result<(), String> {
    let program = and23_syntax::parse(src).map_err(|d| d.render(file, src))?;
    interp.run(&program, echo).map_err(|d| d.render(file, src))
}

fn run_file(path: &str) -> u8 {
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) => {
            eprintln!("error: no se pudo leer `{path}`: {e}");
            return 1;
        }
    };
    let mut interp = Interpreter::new(io::stdout());
    let result = execute(&mut interp, &src, path, false)
        .and_then(|()| interp.run_main().map_err(|d| d.render(path, &src)));
    // La salida normal debe quedar escrita antes del mensaje de error.
    let _ = interp.out.flush();
    match result {
        Ok(()) => 0,
        Err(rendered) => {
            eprint!("{rendered}");
            1
        }
    }
}

/// Llaves abiertas menos cerradas, sin contar los comentarios; sirve para saber si una
/// entrada del REPL quedó a medias.
fn open_braces(src: &str) -> isize {
    src.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .flat_map(str::chars)
        .map(|c| match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        })
        .sum()
}

fn repl() -> u8 {
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut interp = Interpreter::new(io::stdout());
    let mut pending = String::new();
    loop {
        if interactive {
            print!(
                "{}",
                if pending.is_empty() {
                    "and23> "
                } else {
                    "...    "
                }
            );
            let _ = io::stdout().flush();
        }
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("error: no se pudo leer la entrada: {e}");
                return 1;
            }
        }
        if pending.is_empty() && line.trim() == ":q" {
            break;
        }
        pending.push_str(&line);
        if open_braces(&pending) > 0 {
            continue;
        }
        let src = std::mem::take(&mut pending);
        if let Err(rendered) = execute(&mut interp, &src, "<repl>", true) {
            eprint!("{rendered}");
        }
        let _ = interp.out.flush();
    }
    // Si la entrada terminó con un bloque sin cerrar, se muestra el error en vez de ignorarlo.
    if !pending.trim().is_empty() {
        if let Err(rendered) = execute(&mut interp, &pending, "<repl>", true) {
            eprint!("{rendered}");
        }
    }
    let _ = interp.out.flush();
    0
}

#[cfg(test)]
mod tests {
    use super::open_braces;

    #[test]
    fn llaves_abiertas() {
        assert_eq!(open_braces("fn f() {"), 1);
        assert_eq!(open_braces("fn f() {\n  if x {\n"), 2);
        assert_eq!(open_braces("fn f() { }\n"), 0);
        assert_eq!(open_braces("x = 1 // una { en un comentario"), 0);
    }
}
