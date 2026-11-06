use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn and23() -> Command {
    Command::new(env!("CARGO_BIN_EXE_and23"))
}

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

/// Escribe `src` en un archivo temporal único, lo ejecuta y lo borra.
fn run_source(tag: &str, src: &str) -> Output {
    let path = std::env::temp_dir().join(format!("and23_cli_{tag}_{}.jb", std::process::id()));
    std::fs::write(&path, src).unwrap();
    let out = and23().arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    out
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap()
}

fn run_example(name: &str) -> String {
    let out = and23().arg(example(name)).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

#[test]
fn ejemplo_de_aritmetica() {
    assert_eq!(
        run_example("aritmetica.jb"),
        "7\n9\n2\n2.5\n-1\n-6\n1000000\n"
    );
}

#[test]
fn ejemplo_de_funciones() {
    assert_eq!(run_example("funciones.jb"), "3628800\n6765\n6\n144\n");
}

#[test]
fn ejemplo_de_control_de_flujo() {
    assert_eq!(
        run_example("control.jb"),
        "2\n3\n5\n7\n11\n13\n17\n19\n23\n29\n30\n"
    );
}

#[test]
fn error_de_ejecucion_con_linea_columna_y_codigo_de_salida() {
    let out = run_source("div0", "show(1)\nshow(4 / 0)\n");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "1\n");
    let err = stderr(&out);
    assert!(err.contains("error: división entre cero"), "{err}");
    assert!(err.contains(":2:6"), "{err}");
    assert!(err.contains("show(4 / 0)"), "{err}");
}

#[test]
fn error_de_sintaxis_no_ejecuta_nada() {
    let out = run_source("sintaxis", "show(1)\nshow(2 +)\n");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("se esperaba una expresión"));
}

#[test]
fn main_se_llama_despues_del_codigo_del_nivel_superior() {
    let out = run_source("main", "fn main() { show(2) }\nshow(1)\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "1\n2\n");
}

#[test]
fn archivo_con_bom_de_windows() {
    let out = run_source("bom", "\u{feff}show(40 + 2)\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "42\n");
}

#[test]
fn la_recursion_infinita_es_un_error_y_no_un_cierre_abrupto() {
    let out = run_source("recursion", "fn f(n: Int) -> Int = f(n + 1)\nshow(f(0))\n");
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("desbordamiento de pila"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn recursion_profunda_dentro_del_limite_funciona() {
    let src = "fn suma(n: Int) -> Int {\n if n == 0 { return 0 }\n return n + suma(n - 1)\n}\nshow(suma(5000))\n";
    let out = run_source("profunda", src);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "12502500\n");
}

#[test]
fn archivo_inexistente() {
    let out = and23().arg("no_existe.jb").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("no se pudo leer"));
}

#[test]
fn version() {
    let out = and23().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert!(stdout(&out).starts_with("and23 "));
}

fn repl(input: &str) -> Output {
    let mut child = and23()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn repl_evalua_expresiones_sueltas_y_sigue_tras_un_error() {
    let out = repl("1 + 2 * 3\n1 / 0\n(2 + 3) * 4\n:q\n");
    assert!(out.status.success());
    assert_eq!(stdout(&out), "7\n20\n");
    assert!(stderr(&out).contains("división entre cero"));
}

#[test]
fn repl_recuerda_variables_y_funciones() {
    let out = repl("mut total = 1\nfn doble(x: Int) = x * 2\ntotal = doble(total) + 1\ntotal\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "3\n");
}

#[test]
fn repl_acepta_bloques_en_varias_lineas() {
    let input = "fn fact(n: Int) -> Int {\n  if n <= 1 { return 1 }\n  return n * fact(n - 1)\n}\nfact(5)\n";
    let out = repl(input);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "120\n");
    assert_eq!(stderr(&out), "");
}

#[test]
fn repl_con_bloque_sin_cerrar_al_terminar_muestra_el_error() {
    let out = repl("fn f() {\n");
    assert!(out.status.success());
    assert!(stderr(&out).contains("`}`"), "{}", stderr(&out));
}
