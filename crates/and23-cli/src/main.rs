use and23_bytecode::{disassemble, Compiler};
use and23_eval::Interpreter;
use and23_syntax::Diagnostic;
use and23_types::Checker;
use and23_vm::Session;
use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;

const USAGE: &str = "\
And23 (M11)

Uso:
  and23 <archivo.jb>   ejecuta un programa (y llama a main() si existe)
  and23                abre el REPL (escribe :q o Ctrl+Z para salir)
  and23 --disasm <archivo.jb>   muestra el bytecode en vez de ejecutar
  and23 --tree <archivo.jb>     usa el intérprete de árbol (referencia de M1a; sin cadenas)
  and23 --gc-stress ...         recolecta basura en cada punto seguro (prueba del GC)
  and23 --max-depth N ...       llamadas anidadas permitidas (por defecto 100000)
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
    // `--max-depth N` cambia cuántas llamadas pueden estar anidadas antes del error de pila.
    let (depth, args) = match take_max_depth(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("error: {message}");
            return 2;
        }
    };
    let stress = Options { stress, depth };
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

/// Opciones de ejecución que valen para cualquier modo.
#[derive(Clone, Copy)]
struct Options {
    stress: bool,
    depth: Option<usize>,
}

impl Options {
    fn session<W: Write>(self, out: W) -> Session<W> {
        let session = Session::new(out).with_gc_stress(self.stress);
        match self.depth {
            Some(depth) => session.with_max_depth(depth),
            None => session,
        }
    }
}

/// Quita `--max-depth N` de los argumentos y devuelve N.
fn take_max_depth(args: Vec<String>) -> Result<(Option<usize>, Vec<String>), String> {
    let mut depth = None;
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        if arg == "--max-depth" {
            let value = it
                .next()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|n| *n > 0)
                .ok_or("`--max-depth` necesita un número entero positivo")?;
            depth = Some(value);
        } else {
            rest.push(arg);
        }
    }
    Ok((depth, rest))
}

/// Carga el archivo y todo lo que importa con `use`; el error ya viene listo para mostrarse.
fn load_program(path: &str) -> Result<and23_syntax::modules::Loaded, String> {
    and23_syntax::modules::load(path, |p| std::fs::read_to_string(p)).map_err(|f| f.render())
}

/// Analiza, compila y ejecuta `src` en `session`. El error ya viene formateado con su posición.
fn execute<W: Write>(
    session: &mut Session<W>,
    src: &str,
    file: &str,
    echo: bool,
) -> Result<(), String> {
    let program = and23_syntax::parse(src).map_err(|d| d.render(file, src))?;
    if let Some(u) = program.uses.first() {
        return Err(
            Diagnostic::new(u.span, "`use` solo funciona en archivos, no en el REPL")
                .render(file, src),
        );
    }
    session.run(&program, echo).map_err(|d| d.render(file, src))
}

fn run_file(path: &str, options: Options) -> u8 {
    let loaded = match load_program(path) {
        Ok(loaded) => loaded,
        Err(text) => return finish(Err(text)),
    };
    let sources = loaded.sources;
    let mut session = options.session(io::stdout());
    let result = session
        .run(&loaded.program, false)
        .and_then(|()| session.run_main())
        .map_err(|d| sources.render(&d));
    // La salida normal debe quedar escrita antes del mensaje de error.
    let _ = session.out().flush();
    finish(result)
}

/// Ejecuta con el intérprete de árbol: sirve para comparar velocidad y comportamiento.
fn run_file_tree(path: &str) -> u8 {
    let loaded = match load_program(path) {
        Ok(loaded) => loaded,
        Err(text) => return finish(Err(text)),
    };
    let sources = loaded.sources;
    let mut interp = Interpreter::new(io::stdout());
    let result = interp
        .run(&loaded.program, false)
        .and_then(|()| interp.run_main())
        .map_err(|d| sources.render(&d));
    let _ = interp.out.flush();
    finish(result)
}

fn disasm_file(path: &str) -> u8 {
    let result = load_program(path).and_then(|loaded| {
        let sources = loaded.sources;
        let program = loaded.program;
        {
            // El preludio (la clase `Fallo`) se compila primero y no se muestra.
            let prelude = and23_syntax::parse(and23_syntax::PRELUDE).expect("preludio válido");
            let mut checker = Checker::new();
            let mut compiler = Compiler::new();
            let res = checker.check(&prelude).expect("preludio válido");
            compiler
                .compile(&prelude, &res, false)
                .expect("preludio válido");
            checker.seal_prelude();
            compiler.seal_prelude();
            let skip = compiler.module().functions.len();
            let res = checker.check(&program).map_err(|d| sources.render(&d))?;
            compiler
                .compile(&program, &res, false)
                .map_err(|d| sources.render(&d))?;
            let module = compiler.module();
            for function in &module.functions[skip..] {
                println!("{}", disassemble(module, function));
            }
            Ok(())
        }
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

fn repl(options: Options) -> u8 {
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut session = options.session(io::stdout());
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
