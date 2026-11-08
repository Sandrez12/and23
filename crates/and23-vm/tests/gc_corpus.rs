//! El recolector de basura no puede cambiar lo que hace un programa: cada programa del repositorio que se puede ejecutar
//! sin tocar el disco ni esperar da la misma salida y el mismo resultado con la recolección normal que recolectando en
//! **cada** punto seguro (`--gc-stress`). Si el GC liberara algo vivo o no registrara una raíz, aquí cambiaría la salida.

use and23_vm::Session;
use std::path::{Path, PathBuf};

/// Lo que ve quien ejecuta el programa: lo que escribió y cómo terminó.
#[derive(Debug, PartialEq)]
struct Outcome {
    output: String,
    result: Result<(), String>,
}

fn run(src: &str, stress: bool) -> Option<Outcome> {
    let program = and23_syntax::parse(src).ok()?;
    let mut out = Vec::new();
    let result = {
        let mut session = Session::new(&mut out)
            .with_step_limit(5_000_000)
            .with_gc_stress(stress);
        session
            .run(&program, false)
            .and_then(|()| session.run_main())
            .map_err(|d| d.message)
    };
    Some(Outcome {
        output: String::from_utf8_lossy(&out).into_owned(),
        result,
    })
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

/// Nombres que tocan el disco, el entorno, la entrada o el reloj: esos programas no se ejecutan aquí.
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

#[test]
fn el_gc_en_cada_punto_seguro_no_cambia_ningun_programa_del_repositorio() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    jb_files(&root.join("examples"), &mut files);
    jb_files(&root.join("mn/casos_checker"), &mut files);
    jb_files(&root.join("mn/casos"), &mut files);
    let mut compared = 0;
    let mut finished = 0;
    let mut failures = Vec::new();
    // El hilo con la pila grande de la CLI: algunos casos de parser anidan mucho.
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            for file in &files {
                let Ok(src) = std::fs::read_to_string(file) else {
                    continue;
                };
                if PROHIBIDOS.iter().any(|p| src.contains(p)) {
                    continue;
                }
                let (Some(normal), Some(stress)) = (run(&src, false), run(&src, true)) else {
                    continue;
                };
                compared += 1;
                finished += usize::from(normal.result.is_ok());
                if normal != stress {
                    failures.push(file.to_string_lossy().into_owned());
                }
            }
            (compared, finished, failures)
        })
        .unwrap();
    let (compared, finished, failures) = handle.join().unwrap();
    assert!(
        failures.is_empty(),
        "con --gc-stress cambia el resultado de: {failures:#?}"
    );
    // Que el corpus sea de verdad ejecutable: si el filtro se llevara casi todo, la prueba no probaría nada.
    assert!(compared > 150, "solo {compared} programas comparados");
    assert!(finished > 60, "solo {finished} programas terminan bien");
}
