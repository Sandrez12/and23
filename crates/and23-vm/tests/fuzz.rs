//! Fuzzing del verificador, el compilador y la VM: se mutan programas reales y cada resultado se ejecuta de punta a
//! punta (analizar, verificar tipos, compilar y correr en la VM con un tope de instrucciones).
//!
//! Lo que se exige es que **nada entre en pánico ni se cuelgue**: un programa mal formado, mal tipado o que falla al
//! correr da un `Diagnostic` (con una posición dentro del texto), nunca un pánico del verificador, del compilador o de la
//! VM. A una de cada ocho ejecuciones se le activa `--gc-stress` (recolección en cada punto seguro).
//!
//! Seguridad de la prueba: el corpus excluye los programas que usan archivos, carpetas, entorno, entrada estándar o
//! `duerme`, y el generador no inventa esos nombres, así que ningún mutante toca el disco ni espera.
//!
//! `AND23_FUZZ_ITERS` (por defecto 3 000) y `AND23_FUZZ_SEED` cambian la sesión; para una larga:
//! `AND23_FUZZ_ITERS=200000 cargo test -p and23-vm --test fuzz --release -- --nocapture`.

use and23_vm::Session;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Cuántas ejecuciones terminaron sin error: si fueran muy pocas, los mutantes no estarían llegando a la VM.
static TERMINARON: AtomicUsize = AtomicUsize::new(0);
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// Instrucciones que puede ejecutar un programa antes de cortarlo.
const STEPS: u64 = 200_000;

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

/// Descarta la salida del programa, pero la cuenta para no llenar la memoria.
struct Sink(usize);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
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

/// Nombres que tocan el disco, el entorno, la entrada o el reloj: ningún programa del corpus los usa.
const PROHIBIDOS: &[&str] = &[
    "Archivo",
    "creaDir",
    "listaDir",
    "borra",
    "escribe",
    "agrega",
    "leeLinea",
    "lee(",
    "entorno",
    "duerme",
    "argumentos",
    "use ",
    "ahora",
    "azar",
];

fn corpus() -> Vec<Vec<char>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    jb_files(&root.join("examples"), &mut files);
    jb_files(&root.join("mn/casos_checker"), &mut files);
    jb_files(&root.join("mn/casos"), &mut files);
    let texts: Vec<Vec<char>> = files
        .iter()
        .filter(|f| {
            !f.to_string_lossy()
                .replace('\\', "/")
                .contains("/proyecto/")
        })
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .filter(|s| !PROHIBIDOS.iter().any(|p| s.contains(p)))
        .filter(|s| s.len() < 20_000)
        .map(|s| s.chars().collect())
        .collect();
    assert!(texts.len() > 150, "corpus de {} programas", texts.len());
    texts
}

/// Trozos que suelen romper un verificador o una VM.
const PIEZAS: &[&str] = &[
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "\"",
    "${",
    "\n",
    "fn ",
    "thing ",
    "kind ",
    "enum ",
    "match ",
    "attempt ",
    "rescue ",
    "fail ",
    "return ",
    "break",
    "continue",
    "?.",
    "?:",
    "?",
    "->",
    "<",
    ">",
    "==",
    "!=",
    "&&",
    "||",
    "!",
    "-",
    "+=",
    "*",
    "/",
    "%",
    "0",
    "1",
    "-1",
    "9223372036854775807",
    "9223372036854775808",
    "1e308",
    "0.0",
    "nil",
    "true",
    "false",
    "this",
    "super",
    "mut ",
    "fix ",
    "as ",
    "is ",
    ",",
    ":",
    ".",
    "x",
    "y",
    "f(",
    "show(",
    "len(",
    "push(",
    "pop(",
    "hash(",
    "[]",
    "[0]",
    "[1]",
    "[-1]",
    "Lista<Int>()",
    "Mapa<Str, Int>()",
    "Fallo(\"x\")",
    "lanza { ",
    "cede()",
    "Canal<Int>(1)",
    "espera()",
    "recibe()",
    "envia(1)",
    "cierra()",
    "{ a -> a }",
    "\"a\"",
    "\"${1}\"",
    "é",
    "😀",
];

/// Cambios que casi siempre conservan los tipos y por eso llegan a la VM: otro número, otro operador de la misma
/// familia, `true` por `false` o una línea menos.
fn mutate_typed(rng: &mut Rng, text: &mut Vec<char>) {
    const FAMILIAS: &[&[&str]] = &[
        &["+", "-", "*", "/", "%"],
        &["<", ">", "<=", ">=", "==", "!="],
        &["&&", "||"],
        &["true", "false"],
        &["fix", "mut"],
    ];
    const NUMEROS: &[&str] = &[
        "0",
        "1",
        "2",
        "-1",
        "100",
        "9223372036854775807",
        "-9223372036854775807",
        "0.0",
        "1.5",
    ];
    let src: String = text.iter().collect();
    match rng.below(3) {
        0 => {
            // Un literal numérico entero por otro.
            let digits: Vec<usize> = (0..text.len())
                .filter(|&i| {
                    text[i].is_ascii_digit()
                        && (i == 0 || !text[i - 1].is_alphanumeric() && text[i - 1] != '.')
                })
                .collect();
            if let Some(&at) = digits.get(rng.below(digits.len().max(1))) {
                let len = text[at..].iter().take_while(|c| c.is_ascii_digit()).count();
                if text.get(at + len) != Some(&'.') {
                    // Casi siempre un número pequeño; de vez en cuando un extremo.
                    let extremo = rng.below(4) == 0;
                    let k = rng.below(if extremo { NUMEROS.len() } else { 4 });
                    let n: Vec<char> = NUMEROS[k].chars().collect();
                    text.splice(at..at + len, n);
                }
            }
        }
        1 => {
            let familia = FAMILIAS[rng.below(FAMILIAS.len())];
            let from = familia[rng.below(familia.len())];
            let places: Vec<usize> = src.match_indices(from).map(|(b, _)| b).collect();
            if let Some(&byte) = places.get(rng.below(places.len().max(1))) {
                let at = src[..byte].chars().count();
                let to: Vec<char> = familia[rng.below(familia.len())].chars().collect();
                text.splice(at..at + from.chars().count(), to);
            }
        }
        _ => {
            // Borra una línea entera.
            let starts: Vec<usize> = std::iter::once(0)
                .chain((0..text.len()).filter(|&i| text[i] == '\n').map(|i| i + 1))
                .collect();
            let k = rng.below(starts.len());
            let (a, b) = (starts[k], starts.get(k + 1).copied().unwrap_or(text.len()));
            text.drain(a..b);
        }
    }
}

fn mutate(rng: &mut Rng, corpus: &[Vec<char>]) -> String {
    let mut text: Vec<char> = corpus[rng.below(corpus.len())].clone();
    if rng.below(10) < 6 {
        for _ in 0..1 + rng.below(2) {
            mutate_typed(rng, &mut text);
        }
        return text.into_iter().collect();
    }
    for _ in 0..1 + rng.below(3) {
        if text.is_empty() {
            text.extend("show(1)".chars());
        }
        let at = rng.below(text.len());
        match rng.below(7) {
            0 => {
                let n = 1 + rng.below(12);
                text.drain(at..(at + n).min(text.len()));
            }
            1 => {
                let end = (at + 1 + rng.below(30)).min(text.len());
                let piece: Vec<char> = text[at..end].to_vec();
                for (i, c) in piece.into_iter().enumerate() {
                    text.insert(at + i, c);
                }
            }
            2..=4 => {
                let piece: Vec<char> = PIEZAS[rng.below(PIEZAS.len())].chars().collect();
                for (i, c) in piece.into_iter().enumerate() {
                    text.insert(at + i, c);
                }
            }
            5 => {
                let other = &corpus[rng.below(corpus.len())];
                let from = rng.below(other.len().max(1));
                let to = (from + rng.below(200)).min(other.len());
                for (i, c) in other[from..to].iter().enumerate() {
                    text.insert((at + i).min(text.len()), *c);
                }
            }
            _ => {
                // Cambia una palabra entera por otra del propio programa (mismo vocabulario, tipos distintos).
                let words: Vec<usize> = text
                    .iter()
                    .enumerate()
                    .filter(|(i, c)| {
                        c.is_alphabetic() && (*i == 0 || !text[*i - 1].is_alphanumeric())
                    })
                    .map(|(i, _)| i)
                    .collect();
                if words.len() > 2 {
                    let (a, b) = (words[rng.below(words.len())], words[rng.below(words.len())]);
                    let len = |from: usize| {
                        text[from..]
                            .iter()
                            .take_while(|c| c.is_alphanumeric())
                            .count()
                    };
                    let replacement: Vec<char> = text[b..b + len(b)].to_vec();
                    let end = a + len(a);
                    text.splice(a..end, replacement);
                }
            }
        }
    }
    text.into_iter().collect()
}

/// Ejecuta `src` de punta a punta. `Err` describe una propiedad rota; un programa que simplemente falla es `Ok`.
fn check(src: &str, stress: bool) -> Result<bool, String> {
    let caught = catch_unwind(AssertUnwindSafe(|| {
        let Ok(program) = and23_syntax::parse(src) else {
            return Ok(false);
        };
        let mut session = Session::new(Sink(0))
            .with_step_limit(STEPS)
            .with_gc_stress(stress);
        let result = session
            .run(&program, false)
            .and_then(|()| session.run_main());
        let terminó = result.is_ok();
        if terminó {
            TERMINARON.fetch_add(1, Ordering::Relaxed);
        }
        if let Err(d) = result {
            // Una posición de error tiene que caer dentro del programa para poder mostrarse.
            if d.span.end > src.len() || d.span.start > d.span.end {
                return Err(format!(
                    "posición fuera del programa: {:?} ({})",
                    d.span, d.message
                ));
            }
            let _ = d.render("fuzz.jb", src);
        }
        Ok(terminó)
    }));
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

fn save(name: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fuzz-fallos");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    let _ = std::fs::write(&path, src);
    path
}

#[test]
fn programas_mutados_no_rompen_el_verificador_el_compilador_ni_la_vm() {
    let iters: usize = std::env::var("AND23_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_000);
    let seed: u64 = std::env::var("AND23_FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let progress = Arc::new(AtomicUsize::new(0));
    let current = Arc::new(Mutex::new(String::new()));
    let (tx, rx) = mpsc::channel();
    let (p, c) = (progress.clone(), current.clone());
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let corpus = corpus();
            // Los programas sin mutar también tienen que cumplirlo.
            // Se mutan los que corren bien: los casos que fallan a propósito solo se prueban sin mutar.
            let mut base: Vec<Vec<char>> = Vec::new();
            for (i, text) in corpus.iter().enumerate() {
                let src: String = text.iter().collect();
                match check(&src, i % 8 == 0) {
                    Ok(true) => base.push(text.clone()),
                    Ok(false) => {}
                    Err(e) => {
                        let _ = tx.send(Err(format!("programa sin mutar: {e}")));
                        return;
                    }
                }
            }
            assert!(base.len() > 40, "solo {} programas corren bien", base.len());
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            for i in 0..iters {
                let src = mutate(&mut rng, &base);
                if let Ok(mut cur) = c.lock() {
                    cur.clone_from(&src);
                }
                p.store(i, Ordering::Relaxed);
                if let Err(e) = check(&src, i % 8 == 0).map(|_| ()) {
                    let path = save(&format!("vm-falla-{seed}-{i}.jb"), &src);
                    let _ = tx.send(Err(format!(
                        "{e} (entrada {i}, semilla {seed}; guardada en {})",
                        path.display()
                    )));
                    return;
                }
            }
            let porcentaje = TERMINARON.load(Ordering::Relaxed) * 100 / (iters + corpus.len());
            println!("{porcentaje} % de los programas terminaron sin error");
            if iters >= 1000 && porcentaje < 10 {
                let _ = tx.send(Err(format!(
                    "solo {porcentaje} % de los programas llegan a terminar: el generador no ejercita la VM"
                )));
                return;
            }
            let _ = tx.send(Ok(()));
        })
        .unwrap();
    let mut last = usize::MAX;
    loop {
        match rx.recv_timeout(Duration::from_secs(30)) {
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
                    let path = save("vm-cuelgue.jb", &src);
                    panic!("la entrada no termina (guardada en {})", path.display());
                }
                last = now;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("el hilo del fuzzing murió"),
        }
    }
}
