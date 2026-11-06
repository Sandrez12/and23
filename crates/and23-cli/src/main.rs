use and23_bytecode::{disassemble, Compiler};
use and23_eval::Interpreter;
use and23_types::Checker;
use and23_vm::Session;
use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;

const USAGE: &str = "\
And23 (M5)

Uso:
  and23 <archivo.jb>   ejecuta un programa (y llama a main() si existe)
  and23                abre el REPL (escribe :q o Ctrl+Z para salir)
  and23 --disasm <archivo.jb>   muestra el bytecode en vez de ejecutar
  and23 --tree <archivo.jb>     usa el intérprete de árbol (referencia de M1a; sin cadenas)
  and23 --gc-stress ...         recolecta basura en cada punto seguro (prueba del GC)
  and23 --version
";

/// La VM no recurre en la pila nativa, pero el parser sí (expresiones muy anidadas) y el
/// intérprete de árbol (`--tree`) también: con 256 MB caben sus 10 000 llamadas anidadas.
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
    // `--gc-stress` recolecta basura en cada punto seguro (para probar el GC); va con cualquier modo.
    let stress = args.iter().any(|a| a == "--gc-stress");
    let args: Vec<String> = args
        .iter()
        .filter(|a| *a != "--gc-stress")
        .cloned()
        .collect();
    match args.as_slice() {
        [] => repl(stress),
        [flag] if flag == "--version" || flag == "-V" => {
            println!("and23 {}", env!("CARGO_PKG_VERSION"));
            0
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print!("{USAGE}");
            0
        }
        [flag, path] if flag == "--disasm" => disasm_file(path),
        [flag, path] if flag == "--tree" => run_file_tree(path),
        [path] => run_file(path, stress),
        _ => {
            eprint!("{USAGE}");
            2
        }
    }
}

/// Lee el archivo; el error ya viene listo para mostrarse.
fn read_source(path: &str) -> Result<String, u8> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: no se pudo leer `{path}`: {e}");
        1
    })
}

/// Analiza, compila y ejecuta `src` en `session`. El error ya viene formateado con su posición.
fn execute<W: Write>(
    session: &mut Session<W>,
    src: &str,
    file: &str,
    echo: bool,
) -> Result<(), String> {
    let program = and23_syntax::parse(src).map_err(|d| d.render(file, src))?;
    session.run(&program, echo).map_err(|d| d.render(file, src))
}

fn run_file(path: &str, stress: bool) -> u8 {
    let src = match read_source(path) {
        Ok(src) => src,
        Err(code) => return code,
    };
    let mut session = Session::new(io::stdout()).with_gc_stress(stress);
    let result = execute(&mut session, &src, path, false)
        .and_then(|()| session.run_main().map_err(|d| d.render(path, &src)));
    // La salida normal debe quedar escrita antes del mensaje de error.
    let _ = session.out().flush();
    finish(result)
}

/// Ejecuta con el intérprete de árbol: sirve para comparar velocidad y comportamiento.
fn run_file_tree(path: &str) -> u8 {
    let src = match read_source(path) {
        Ok(src) => src,
        Err(code) => return code,
    };
    let mut interp = Interpreter::new(io::stdout());
    let result = and23_syntax::parse(&src)
        .map_err(|d| d.render(path, &src))
        .and_then(|program| {
            interp
                .run(&program, false)
                .map_err(|d| d.render(path, &src))
        })
        .and_then(|()| interp.run_main().map_err(|d| d.render(path, &src)));
    let _ = interp.out.flush();
    finish(result)
}

fn disasm_file(path: &str) -> u8 {
    let src = match read_source(path) {
        Ok(src) => src,
        Err(code) => return code,
    };
    let result = and23_syntax::parse(&src)
        .map_err(|d| d.render(path, &src))
        .and_then(|program| {
            let res = Checker::new()
                .check(&program)
                .map_err(|d| d.render(path, &src))?;
            let mut compiler = Compiler::new();
            compiler
                .compile(&program, &res, false)
                .map_err(|d| d.render(path, &src))?;
            let module = compiler.module();
            for function in &module.functions {
                println!("{}", disassemble(module, function));
            }
            Ok(())
        });
    finish(result)
}

fn finish(result: Result<(), String>) -> u8 {
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

fn repl(stress: bool) -> u8 {
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut session = Session::new(io::stdout()).with_gc_stress(stress);
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
        if let Err(rendered) = execute(&mut session, &src, "<repl>", true) {
            eprint!("{rendered}");
        }
        let _ = session.out().flush();
    }
    // Si la entrada terminó con un bloque sin cerrar, se muestra el error en vez de ignorarlo.
    if !pending.trim().is_empty() {
        if let Err(rendered) = execute(&mut session, &pending, "<repl>", true) {
            eprint!("{rendered}");
        }
    }
    let _ = session.out().flush();
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
