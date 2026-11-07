//! El formateador no puede cambiar el significado de ningún programa real: los tokens son los mismos
//! (solo cambian las posiciones) y formatear dos veces da lo mismo que una.

use and23_syntax::format::format;
use and23_syntax::lexer::lex;
use std::path::{Path, PathBuf};

fn jb_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            jb_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "jb") {
            out.push(path);
        }
    }
}

/// Quita de un `Debug` los `Span { start: N, end: M }` (las cadenas con `${}` llevan tokens anidados).
fn without_spans(debug: &str) -> String {
    let mut out = String::new();
    let mut rest = debug;
    while let Some(i) = rest.find("Span {") {
        out.push_str(&rest[..i]);
        let end = rest[i..].find('}').map_or(rest.len(), |j| i + j + 1);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Los tokens sin posiciones; varias líneas en blanco seguidas valen como una (el formateador las junta).
fn kinds(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in lex(src).unwrap_or_else(|e| panic!("no lexea: {}", e.message)) {
        let k = without_spans(&format!("{:?}", t.kind));
        if k == "Newline" && out.last().is_none_or(|p| p == "Newline") {
            continue;
        }
        out.push(k);
    }
    out
}

#[test]
fn formatear_no_cambia_los_tokens_de_ningun_ejemplo_y_es_idempotente() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![root.join("crates/and23-syntax/prelude.jb")];
    jb_files(&root.join("examples"), &mut files);
    assert!(files.len() > 10, "{} archivos", files.len());
    for file in files {
        let src = std::fs::read_to_string(&file).unwrap();
        let once = format(&src);
        assert_eq!(kinds(&src), kinds(&once), "cambian los tokens de {file:?}");
        assert_eq!(format(&once), once, "no es idempotente en {file:?}");
    }
}

#[test]
fn los_ejemplos_del_repositorio_ya_estan_formateados() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    jb_files(&root.join("examples"), &mut files);
    for file in files {
        let src = std::fs::read_to_string(&file)
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(format(&src), src, "{file:?} no está formateado (and23 fmt)");
    }
}
