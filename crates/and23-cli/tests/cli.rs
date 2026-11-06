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

#[test]
fn tree_ejecuta_con_el_interprete_de_arbol() {
    let path = example("funciones.jb");
    let out = and23().arg("--tree").arg(&path).output().unwrap();
    assert!(out.status.success());
    assert_eq!(stdout(&out), run_example("funciones.jb"));
}

#[test]
fn disasm_muestra_el_bytecode_sin_ejecutar() {
    let path = example("funciones.jb");
    let out = and23().arg("--disasm").arg(&path).output().unwrap();
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("fn factorial/1"), "{text}");
    assert!(text.contains("Call(") && text.contains("Return"), "{text}");
    assert!(!text.contains("3628800"), "no debe ejecutar: {text}");
}

#[test]
fn un_error_de_tipos_se_informa_antes_de_ejecutar_nada() {
    let out = run_source("tipos", "show(1)\nfix x: Int = true\n");
    assert!(!out.status.success());
    assert_eq!(stdout(&out), "", "no debe ejecutarse nada");
    let err = stderr(&out);
    assert!(
        err.contains("error: se esperaba Int, pero es Bool"),
        "{err}"
    );
    assert!(err.contains(":2:14"), "{err}");
}

#[test]
fn repl_rechaza_tipos_y_sigue() {
    let out = repl("mut n = 1\nn = true\nn = n + 1\nn\n");
    assert_eq!(stdout(&out), "2\n");
    assert!(stderr(&out).contains("se esperaba Int, pero es Bool"));
}

#[test]
fn ejemplo_de_cadenas_y_arreglos() {
    let out = run_example("cadenas.jb");
    assert_eq!(
        out,
        "1 2 Fizz 4 Buzz Fizz 7 8 Fizz Buzz 11 Fizz 13 14 FizzBuzz \n\
         Ana tiene 3 notas: [7, 9, 8], suma 24\n\
         [\"Rex!\", \"Luna\"]\n"
    );
}

#[test]
fn gc_stress_da_la_misma_salida() {
    let path = example("cadenas.jb");
    let out = and23().arg("--gc-stress").arg(&path).output().unwrap();
    assert!(out.status.success());
    assert_eq!(stdout(&out), run_example("cadenas.jb"));
}

#[test]
fn error_de_indice_con_posicion() {
    let out = run_source("indice", "fix a = [1, 2]\nshow(a[7])\n");
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("índice fuera de rango: 7"), "{err}");
    assert!(err.contains(":2:6"), "{err}");
}

#[test]
fn repl_con_cadenas() {
    let out = repl("fix n = \"Ana\"\n\"hola ${n}\"\nn + \"!\"\n");
    assert_eq!(stdout(&out), "hola Ana\nAna!\n");
}

#[test]
fn ejemplo_de_clases() {
    let out = run_example("clases.jb");
    assert!(out.contains("Vec2(x=1.0, y=2.0)\n"), "{out}");
    assert!(out.contains("Ana: saldo 30 (abrió con 100)\n"), "{out}");
    assert!(
        out.contains("páginas en total: 940, préstamos: 3\n"),
        "{out}"
    );
    assert!(out.ends_with("tras reiniciar: 0\n"), "{out}");
}

#[test]
fn un_error_de_clase_se_informa_con_posicion() {
    let out = run_source(
        "clase_err",
        "thing P(fix x: Int)\nfix p = P(1)\np.x = 2\nshow(1)\n",
    );
    assert!(!out.status.success());
    assert!(stdout(&out).is_empty(), "no debe ejecutar nada");
    let err = stderr(&out);
    assert!(err.contains(":3:3"), "{err}");
    assert!(err.contains("es inmutable"), "{err}");
}

#[test]
fn disasm_de_una_clase_muestra_constructor_y_metodos() {
    let path =
        std::env::temp_dir().join(format!("and23_cli_disasm_clase_{}.jb", std::process::id()));
    std::fs::write(
        &path,
        "thing C(mut n: Int) { fn inc() { n += 1 } }\nshow(C(1).n)\n",
    )
    .unwrap();
    let out = and23().arg("--disasm").arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    let text = stdout(&out);
    assert!(text.contains("fn C/1"), "{text}");
    assert!(text.contains("fn C.inc/1"), "{text}");
    assert!(text.contains("Alloc"), "{text}");
    assert!(text.contains("GetField"), "{text}");
}

#[test]
fn repl_con_clases() {
    let out = repl("thing P(fix x: Int)\nfix p = P(4)\np.x\np\n");
    assert!(stdout(&out).contains("4\nP(x=4)\n"), "{}", stdout(&out));
}

#[test]
fn ejemplo_polimorfico() {
    let out = run_example("polimorfismo.jb");
    assert!(out.starts_with("patas en total: 14\n"), "{out}");
    assert!(
        out.contains("¿toby es Cachorro? true, ¿rex es Cachorro? false\n"),
        "{out}"
    );
    assert!(out.contains("truco de toby: dar la pata\n"), "{out}");
    assert!(
        out.ends_with("la figura mayor es un círculo de área 12.56\ntrue\n"),
        "{out}"
    );
}

#[test]
fn gc_stress_con_herencia_da_la_misma_salida() {
    let normal = and23().arg(example("polimorfismo.jb")).output().unwrap();
    let stress = and23()
        .arg("--gc-stress")
        .arg(example("polimorfismo.jb"))
        .output()
        .unwrap();
    assert!(stress.status.success(), "{}", stderr(&stress));
    assert_eq!(stdout(&normal), stdout(&stress));
}

#[test]
fn un_error_de_interfaz_se_informa_con_posicion() {
    let out = run_source(
        "kind_err",
        "kind K { fn f() -> Int }\nthing A() : K\nshow(1)\n",
    );
    assert!(!out.status.success());
    assert!(stdout(&out).is_empty(), "no debe ejecutar nada");
    let err = stderr(&out);
    assert!(err.contains(":2:7"), "{err}");
    assert!(err.contains("no implementa el método `f`"), "{err}");
}

#[test]
fn un_as_invalido_es_un_error_de_ejecucion_con_posicion() {
    let out = run_source(
        "as_err",
        "thing A()\nthing B() : A()\nthing C() : A()\nfix a: A = C()\nshow(a as B)\n",
    );
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("no se puede convertir un `C` a `B`"), "{err}");
    assert!(err.contains(":5:6"), "{err}");
}

#[test]
fn disasm_muestra_llamadas_virtuales_e_is() {
    let path =
        std::env::temp_dir().join(format!("and23_cli_disasm_virt_{}.jb", std::process::id()));
    std::fs::write(
        &path,
        "kind K { fn f() -> Int }\nthing A() : K { fn f() -> Int = 1 }\nfix k: K = A()\nshow(k.f())\nshow(k is A)\n",
    )
    .unwrap();
    let out = and23().arg("--disasm").arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    let text = stdout(&out);
    assert!(text.contains("CallMethod"), "{text}");
    assert!(text.contains("IsInstance"), "{text}");
    assert!(text.contains("fn A.<init>/1"), "{text}");
}
