use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn and23() -> Command {
    Command::new(env!("CARGO_BIN_EXE_and23"))
}

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

#[test]
fn ejecuta_el_ejemplo_de_aritmetica() {
    let out = and23().arg(example("aritmetica.jb")).output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "7\n9\n2\n2.5\n-1\n-6\n1000000\n"
    );
}

#[test]
fn error_de_ejecucion_con_linea_columna_y_codigo_de_salida() {
    let path = std::env::temp_dir().join(format!("and23_cli_test_{}.jb", std::process::id()));
    std::fs::write(&path, "show(1)\nshow(4 / 0)\n").unwrap();
    let out = and23().arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "1\n");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("error: división entre cero"), "{err}");
    assert!(err.contains(":2:6"), "{err}");
    assert!(err.contains("show(4 / 0)"), "{err}");
}

#[test]
fn archivo_inexistente() {
    let out = and23().arg("no_existe.jb").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8(out.stderr)
        .unwrap()
        .contains("no se pudo leer"));
}

#[test]
fn repl_evalua_expresiones_sueltas_y_sigue_tras_un_error() {
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
        .write_all("1 + 2 * 3\n1 / 0\n(2 + 3) * 4\n:q\n".as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "7\n20\n");
    assert!(String::from_utf8(out.stderr)
        .unwrap()
        .contains("división entre cero"));
}
