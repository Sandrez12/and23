//! Fuzzing del lexer, el parser, el volcado del árbol, el formateador y los mensajes de error.
//!
//! Sin dependencias ni compilador nocturno: un generador determinista (xorshift) muta los programas reales del
//! repositorio (ejemplos, `mn/`, casos y el preludio) y cada resultado tiene que cumplir:
//!
//! - ninguna etapa entra en pánico ni se cuelga;
//! - un error apunta a una posición dentro del texto y `render` la puede mostrar;
//! - si el programa es válido, `format` lo deja válido y formatear dos veces da lo mismo que una.
//!
//! Por defecto corre `AND23_FUZZ_ITERS` = 20 000 entradas (unos segundos). Para una sesión larga:
//! `AND23_FUZZ_ITERS=1000000 AND23_FUZZ_SEED=7 cargo test -p and23-syntax --test fuzz --release -- --nocapture`.
//! Una entrada que falla se guarda en `target/fuzz-fallos/` para convertirla en un test.

use and23_syntax::astdump::dump_program;
use and23_syntax::format::format;
use and23_syntax::parse;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn jb_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            jb_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "jb") {
            out.push(path);
        }
    }
}

fn corpus() -> Vec<Vec<char>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    jb_files(&root.join("examples"), &mut files);
    jb_files(&root.join("mn"), &mut files);
    let mut texts: Vec<Vec<char>> = files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .map(|s| s.chars().collect())
        .collect();
    texts.push(and23_syntax::PRELUDE.chars().collect());
    assert!(texts.len() > 100, "corpus de {} archivos", texts.len());
    texts
}

/// Trozos que suelen romper un analizador.
const PIEZAS: &[&str] = &[
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "\"",
    "\\",
    "${",
    "'",
    "//",
    "/*",
    "*/",
    "\n",
    "\r\n",
    "\t",
    " ",
    "fn ",
    "thing ",
    "kind ",
    "enum ",
    "match ",
    "attempt ",
    "rescue ",
    "fail ",
    "use ",
    "open ",
    "prueba ",
    "return",
    "?.",
    "?:",
    "?",
    "->",
    "=>",
    "::",
    "..",
    "<",
    ">",
    "<=",
    "==",
    "!=",
    "&&",
    "||",
    "!",
    "-",
    "+=",
    "1e",
    "1e+",
    "0x",
    "1_",
    "_",
    ".",
    "9223372036854775807",
    "9223372036854775808",
    "1e999",
    "é",
    "ñ",
    "😀",
    "\u{feff}",
    "\u{0}",
    "\u{7f}",
    "\u{2028}",
    "mut ",
    "fix ",
    "nil",
    "true",
    "this",
    "super",
    ",",
    ";",
    ":",
    "as ",
    "is ",
    "{ x -> ",
    "show(",
    "x",
    "123",
    "1.5",
];

fn mutate(rng: &mut Rng, corpus: &[Vec<char>]) -> String {
    let base = &corpus[rng.below(corpus.len())];
    // Con frecuencia, solo una ventana del archivo: más rápido y los comienzos y finales raros importan.
    let mut text: Vec<char> = if rng.below(2) == 0 && base.len() > 2000 {
        let start = rng.below(base.len() - 2000);
        base[start..start + 200 + rng.below(1800)].to_vec()
    } else {
        base.clone()
    };
    for _ in 0..1 + rng.below(4) {
        if text.is_empty() {
            text.extend("fn f() {}".chars());
        }
        let at = rng.below(text.len());
        match rng.below(8) {
            0 => {
                let n = 1 + rng.below(20);
                text.drain(at..(at + n).min(text.len()));
            }
            1 => {
                let end = (at + 1 + rng.below(40)).min(text.len());
                let piece: Vec<char> = text[at..end].to_vec();
                for (i, c) in piece.into_iter().enumerate() {
                    text.insert(at + i, c);
                }
            }
            2 | 3 => {
                let piece: Vec<char> = PIEZAS[rng.below(PIEZAS.len())].chars().collect();
                for (i, c) in piece.into_iter().enumerate() {
                    text.insert(at + i, c);
                }
            }
            4 => text[at] = char::from_u32(rng.next() as u32 % 0x3000).unwrap_or('?'),
            5 => text.truncate(at),
            6 => {
                // Un trozo de otro archivo.
                let other = &corpus[rng.below(corpus.len())];
                let from = rng.below(other.len().max(1));
                let to = (from + rng.below(300)).min(other.len());
                for (i, c) in other[from..to].iter().enumerate() {
                    text.insert((at + i).min(text.len()), *c);
                }
            }
            _ => {
                let other = rng.below(text.len());
                text.swap(at, other);
            }
        }
    }
    text.into_iter().collect()
}

/// Comprueba las propiedades de una entrada. `Err` describe la que no se cumple.
fn check(src: &str) -> Result<(), String> {
    check_with(src, true)
}

/// Con `dump` en falso no se pide el volcado del árbol, que escribe una línea por nodo con tanta sangría como
/// profundidad: en un árbol de 100 000 niveles son cientos de MB por diseño, no un fallo.
fn check_with(src: &str, dump: bool) -> Result<(), String> {
    let caught = catch_unwind(AssertUnwindSafe(|| check_inner(src, dump)));
    match caught {
        Ok(result) => result,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            Err(format!("pánico: {msg}"))
        }
    }
}

fn check_inner(src: &str, dump: bool) -> Result<(), String> {
    match parse(src) {
        Err(d) => {
            if d.span.start > d.span.end || d.span.end > src.len() {
                return Err(format!("posición de error fuera del texto: {:?}", d.span));
            }
            if !src.is_char_boundary(d.span.start) || !src.is_char_boundary(d.span.end) {
                return Err(format!(
                    "la posición de error cae dentro de un carácter: {:?}",
                    d.span
                ));
            }
            let _ = d.render("fuzz.jb", src);
            let _ = d.line_col(src);
        }
        Ok(program) => {
            if dump {
                let _ = dump_program(&program, src);
            }
            let once = format(src);
            if parse(&once).is_err() {
                return Err("format dejó inválido un programa válido".into());
            }
            let twice = format(&once);
            if once != twice {
                return Err("format no es idempotente".into());
            }
        }
    }
    Ok(())
}

fn save(name: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fuzz-fallos");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    let _ = std::fs::write(&path, src);
    path
}

/// Corre `body` en un hilo con la pila grande de la CLI y vigila que avance (si no avanza en 20 s, hay un cuelgue).
fn with_watchdog(
    body: impl FnOnce(&AtomicUsize, &Mutex<String>) -> Result<(), String> + Send + 'static,
) {
    let progress = Arc::new(AtomicUsize::new(0));
    let current = Arc::new(Mutex::new(String::new()));
    let (tx, rx) = mpsc::channel();
    let (p, c) = (progress.clone(), current.clone());
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let _ = tx.send(body(&p, &c));
        })
        .unwrap();
    let mut last = usize::MAX;
    loop {
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(result) => {
                if let Err(e) = result {
                    panic!("{e}");
                }
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let now = progress.load(Ordering::Relaxed);
                if now == last {
                    let src = current.lock().map(|s| s.clone()).unwrap_or_default();
                    let path = save("cuelgue.jb", &src);
                    panic!("la entrada no termina (guardada en {})", path.display());
                }
                last = now;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("el hilo del fuzzing murió"),
        }
    }
}

#[test]
fn entradas_mutadas_no_rompen_el_lexer_ni_el_parser() {
    let iters: usize = std::env::var("AND23_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    let seed: u64 = std::env::var("AND23_FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    with_watchdog(move |progress, current| {
        let corpus = corpus();
        // Los programas sin mutar tienen que cumplir lo mismo.
        for text in &corpus {
            let src: String = text.iter().collect();
            if let Err(e) = check(&src) {
                return Err(format!("programa sin mutar: {e}"));
            }
        }
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut validas = 0usize;
        for i in 0..iters {
            let src = mutate(&mut rng, &corpus);
            if let Ok(mut c) = current.lock() {
                c.clone_from(&src);
            }
            progress.store(i, Ordering::Relaxed);
            validas += usize::from(parse(&src).is_ok());
            if let Err(e) = check(&src) {
                let path = save(&format!("falla-{seed}-{i}.jb"), &src);
                return Err(format!(
                    "{e} (entrada {i}, semilla {seed}; guardada en {})",
                    path.display()
                ));
            }
        }
        // Un fuzzer que solo produce basura (o solo programas válidos) no prueba las dos mitades.
        if iters >= 1000 {
            let porcentaje = validas * 100 / iters;
            if !(2..=98).contains(&porcentaje) {
                return Err(format!("{porcentaje} % de entradas válidas: el generador no equilibra válidas e inválidas"));
            }
        }
        Ok(())
    });
}

/// Anidamiento profundo: no debe agotar la pila del analizador (la CLI corre con 256 MB).
#[test]
fn el_anidamiento_profundo_no_agota_la_pila() {
    with_watchdog(|progress, current| {
        let n = 20_000;
        let casos: Vec<(&str, String)> = vec![
            (
                "paréntesis",
                format!("fix x = {}1{}", "(".repeat(n), ")".repeat(n)),
            ),
            (
                "paréntesis sin cerrar",
                format!("fix x = {}", "(".repeat(n)),
            ),
            ("menos unario", format!("fix x = {}1", "-".repeat(n))),
            ("negación", format!("fix x = {}true", "!".repeat(n))),
            (
                "arreglos",
                format!("fix x = {}1{}", "[".repeat(n), "]".repeat(n)),
            ),
            (
                "bloques",
                format!("fn f() {}{}", "{ ".repeat(n), "}".repeat(n)),
            ),
            (
                "if",
                format!("fn f() {{ {}1{} }}", "if true { ".repeat(n), " }".repeat(n)),
            ),
            (
                "llamadas",
                format!("fix x = {}1{}", "f(".repeat(n), ")".repeat(n)),
            ),
            (
                "lambdas",
                format!("fix x = {}1{}", "{ a -> ".repeat(n), " }".repeat(n)),
            ),
            (
                "interpolación",
                format!("fix x = {}1{}", "\"${".repeat(n), "}\"".repeat(n)),
            ),
            ("binarios", format!("fix x = {}1", "1 + ".repeat(n * 5))),
            ("accesos", format!("fix x = a{}", ".b".repeat(n * 5))),
            (
                "tipos",
                format!("fix x: {}Int{} = 1", "[".repeat(n), "]".repeat(n)),
            ),
            (
                "tipos genéricos",
                format!("fix x: {}Int{} = 1", "Lista<".repeat(n), ">".repeat(n)),
            ),
            ("opcionales", format!("fix x: Int{} = 1", "?".repeat(n))),
        ];
        for (i, (nombre, src)) in casos.iter().enumerate() {
            if let Ok(mut c) = current.lock() {
                *c = nombre.to_string();
            }
            progress.store(i, Ordering::Relaxed);
            if let Err(e) = check_with(src, false) {
                save(&format!("anidado-{i}.jb"), src);
                return Err(format!("{nombre}: {e}"));
            }
        }
        Ok(())
    });
}
