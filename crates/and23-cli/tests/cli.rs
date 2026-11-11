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

#[test]
fn ejemplo_de_excepciones() {
    let out = run_example("excepciones.jb");
    assert!(out.starts_with("ok: 30\nfaltan 430\n"), "{out}");
    assert!(
        out.contains("error: la cuenta de Ana está cerrada\n"),
        "{out}"
    );
    assert!(out.ends_with("6\n0\n3\nnil\n"), "{out}");
}

#[test]
fn gc_stress_con_excepciones_da_la_misma_salida() {
    let normal = and23().arg(example("excepciones.jb")).output().unwrap();
    let stress = and23()
        .arg("--gc-stress")
        .arg(example("excepciones.jb"))
        .output()
        .unwrap();
    assert!(stress.status.success(), "{}", stderr(&stress));
    assert_eq!(stdout(&normal), stdout(&stress));
}

#[test]
fn una_excepcion_sin_capturar_termina_con_error_y_posicion() {
    let out = run_source(
        "sin_capturar",
        "thing E(fix c: Int) : Fallo(\"se rompió\")\nshow(\"antes\")\nfn f() { fail E(3) }\nf()\nshow(\"despues\")\n",
    );
    assert!(!out.status.success());
    assert_eq!(stdout(&out), "antes\n");
    let err = stderr(&out);
    assert!(
        err.contains("excepción sin capturar `E`: se rompió"),
        "{err}"
    );
    assert!(err.contains(":3:10"), "{err}");
}

#[test]
fn desreferenciar_un_opcional_se_rechaza_antes_de_ejecutar() {
    let out = run_source(
        "opcional_err",
        "thing P(fix n: Int)\nfix p: P? = nil\nshow(1)\nshow(p.n)\n",
    );
    assert!(!out.status.success());
    assert!(stdout(&out).is_empty(), "no debe ejecutar nada");
    let err = stderr(&out);
    assert!(err.contains(":4:6"), "{err}");
    assert!(err.contains("puede ser nil"), "{err}");
}

#[test]
fn disasm_de_excepciones_no_muestra_el_preludio() {
    let path = std::env::temp_dir().join(format!("and23_cli_disasm_exc_{}.jb", std::process::id()));
    std::fs::write(
        &path,
        "thing E() : Fallo(\"x\")\nattempt { fail E() }\nrescue e: E { show(e.msg) }\n",
    )
    .unwrap();
    let out = and23().arg("--disasm").arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    let text = stdout(&out);
    assert!(text.contains("Try"), "{text}");
    assert!(text.contains("Fail"), "{text}");
    assert!(text.contains("EndTry"), "{text}");
    assert!(text.contains("fn E/0"), "{text}");
    assert!(!text.contains("fn Fallo/"), "{text}");
}

#[test]
fn repl_con_opcionales_y_excepciones() {
    let out = repl(
        "thing E() : Fallo(\"x\")\nfix a: Int? = nil\na ?: 5\nattempt { fail E() } rescue e: E { show(e.msg) }\nfail E()\n1 + 1\n",
    );
    let text = stdout(&out);
    assert!(text.contains("5\nx\n"), "{text}");
    assert!(text.contains("2\n"), "{text}");
    assert!(
        stderr(&out).contains("excepción sin capturar `E`"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn ejemplo_de_genericos() {
    let out = run_example("genericos.jb");
    assert!(out.starts_with("[1, 4, 9, 16, 25]\n"), "{out}");
    assert!(out.contains("a=3 b=2 c=1 d=nil\n"), "{out}");
    assert!(
        out.contains("cima tres, saco tres, dos, quedan 1\n"),
        "{out}"
    );
    assert!(out.ends_with("[\"ja\", \"ja\", \"ja\"]\n[0, 0]\n"), "{out}");
}

#[test]
fn gc_stress_con_genericos_da_la_misma_salida() {
    let normal = and23().arg(example("genericos.jb")).output().unwrap();
    let stress = and23()
        .arg("--gc-stress")
        .arg(example("genericos.jb"))
        .output()
        .unwrap();
    assert!(stress.status.success(), "{}", stderr(&stress));
    assert_eq!(stdout(&normal), stdout(&stress));
}

#[test]
fn un_error_de_genericos_se_informa_con_posicion() {
    let out = run_source(
        "gen_err",
        "fix l = Lista<Int>()\nshow(1)\nl.agrega(\"hola\")\n",
    );
    assert!(!out.status.success());
    assert!(stdout(&out).is_empty(), "no debe ejecutar nada");
    let err = stderr(&out);
    assert!(err.contains(":3:10"), "{err}");
    assert!(err.contains("se esperaba Int, pero es Str"), "{err}");
}

#[test]
fn desduccion_fallida_pide_los_argumentos_de_tipo() {
    let out = run_source("gen_deduce", "fix l = Lista()\n");
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("no se puede deducir el tipo `T`"), "{err}");
    assert!(err.contains("Lista<Int>(...)"), "{err}");
}

#[test]
fn repl_con_genericos() {
    let out =
        repl("fix m = Mapa<Str, Int>()\nm.pone(\"a\", 7)\nm.obtiene(\"a\")\nm.obtiene(\"b\")\n");
    assert_eq!(stdout(&out), "7\nnil\n");
}

#[test]
fn ejemplo_de_lambdas() {
    let out = run_example("lambdas.jb");
    assert!(
        out.starts_with(
            "21
18
50
1 2 1 3
"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "pares al cuadrado: [4, 16, 36]
"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "[\"ana(31)\", \"caro(31)\", \"beto(25)\", \"dani(19)\"]
"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "té terminó.té terminó.
"
        ),
        "{out}"
    );
    assert!(
        out.ends_with(
            "rescatado: paré en 3
"
        ),
        "{out}"
    );
}

#[test]
fn gc_stress_con_lambdas_da_la_misma_salida() {
    let normal = and23().arg(example("lambdas.jb")).output().unwrap();
    let stress = and23()
        .arg("--gc-stress")
        .arg(example("lambdas.jb"))
        .output()
        .unwrap();
    assert!(stress.status.success(), "{}", stderr(&stress));
    assert_eq!(stdout(&normal), stdout(&stress));
}

#[test]
fn disasm_de_lambdas_muestra_la_creacion_y_las_celdas() {
    let path = std::env::temp_dir().join(format!("and23_cli_disasm_lam_{}.jb", std::process::id()));
    std::fs::write(
        &path,
        "fn f() -> () -> Int {
  mut n = 0
  return { n += 1
 n }
}
",
    )
    .unwrap();
    let out = and23().arg("--disasm").arg(&path).output().unwrap();
    std::fs::remove_file(&path).ok();
    let text = stdout(&out);
    assert!(text.contains("NewCell"), "{text}");
    assert!(text.contains("Closure"), "{text}");
    assert!(text.contains("f.<lambda>"), "{text}");
    assert!(text.contains("GetCapture(0)"), "{text}");
    assert!(text.contains("CellSet"), "{text}");
}

#[test]
fn repl_con_lambdas() {
    let out = repl(
        "mut n = 0
fix inc = { n += 1
 n }
inc()
inc()
Lista<Int>().map { x -> x }.tam()
",
    );
    let text = stdout(&out);
    assert!(
        text.contains(
            "1
2
0
"
        ),
        "{text}"
    );
}

#[test]
fn un_error_de_tipos_en_una_lambda_senala_su_posicion() {
    let out = run_source(
        "lambda_err",
        "fix f: (Int) -> Int = { a -> \"s\" }
",
    );
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains(":1:30"), "{err}");
    assert!(err.contains("se esperaba Int, pero es Str"), "{err}");
}

#[test]
fn la_recursion_de_decenas_de_miles_de_llamadas_funciona() {
    let src = "fn suma(n: Int) -> Int {
 if n == 0 { return 0 }
 return n + suma(n - 1)
}
show(suma(90000))
";
    let out = run_source("recursion90k", src);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "4050045000
"
    );
}

#[test]
fn max_depth_cambia_el_limite_de_llamadas() {
    let path = std::env::temp_dir().join(format!("and23_cli_depth_{}.jb", std::process::id()));
    std::fs::write(
        &path,
        "fn suma(n: Int) -> Int {
 if n == 0 { return 0 }
 return n + suma(n - 1)
}
show(suma(500))
",
    )
    .unwrap();
    let corto = and23()
        .args(["--max-depth", "100"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(corto.status.code(), Some(1));
    assert!(
        stderr(&corto).contains("más de 100 llamadas"),
        "{}",
        stderr(&corto)
    );
    let largo = and23()
        .args(["--max-depth", "1000"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        stdout(&largo),
        "125250
"
    );
    let malo = and23()
        .args(["--max-depth", "abc"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(malo.status.code(), Some(2));
    assert!(stderr(&malo).contains("--max-depth"), "{}", stderr(&malo));
    std::fs::remove_file(&path).ok();
}

/// Los ejemplos de archivos usan rutas relativas a la raíz del repositorio.
fn run_example_from_root(name: &str, stdin: &str) -> Output {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = and23()
        .current_dir(root)
        .arg(format!("examples/{name}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn ejemplo_de_palabras_lee_y_procesa_un_archivo() {
    let out = run_example_from_root("palabras.jb", "");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "5 líneas, 39 palabras
distintas: 26
el: 4
lenguaje: 4
pequeño: 3
es: 2
programa: 2
"
    );
}

#[test]
fn el_repl_estrecha_una_global_mut_salvo_que_una_funcion_la_asigne() {
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
        .write_all(
            b"mut x: Int? = 3\nif x != nil { show(x + 1) }\nfn limpia() { x = nil }\nif x != nil { limpia()\n show(x + 1) }\n",
        )
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(stdout(&out).contains('4'), "{}", stdout(&out));
    assert!(stderr(&out).contains("no admite"), "{}", stderr(&out));
}

#[test]
fn el_ejemplo_de_palabras_funciona_con_saltos_de_linea_de_windows() {
    // Una copia del texto con CRLF (como queda tras clonar en Windows) da el mismo resultado.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let texto = std::fs::read_to_string(root.join("examples/datos/texto.txt")).unwrap();
    let crlf = texto.replace("\r\n", "\n").replace('\n', "\r\n");
    let dir = std::env::temp_dir().join(format!("and23_crlf_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("examples/datos")).unwrap();
    std::fs::write(dir.join("examples/datos/texto.txt"), crlf).unwrap();
    std::fs::copy(root.join("examples/palabras.jb"), dir.join("palabras.jb")).unwrap();
    let out = and23()
        .current_dir(&dir)
        .arg("palabras.jb")
        .output()
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).starts_with("5 líneas, 39 palabras\n"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn ejemplo_de_notas_convierte_texto_y_escribe_un_archivo() {
    let out = run_example_from_root("notas.jb", "");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "línea 4 ignorada: «sin nota» no es una nota
promedio de 4 notas: 4.05
mejor nota: dani con 5.0
validas=4 | mejor=dani
"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(
        !root.join("resumen.tmp").exists(),
        "el ejemplo debe borrar su archivo"
    );
}

#[test]
fn lee_linea_lee_la_entrada_estandar_hasta_el_final() {
    let path = std::env::temp_dir().join(format!("and23_cli_stdin_{}.jb", std::process::id()));
    std::fs::write(
        &path,
        "mut total = 0
mut lineas = 0
while true {
    fix l = leeLinea()
    if l == nil { break }
    lineas += 1
    total += Int.parse(l) ?: 0
}
show(\"${lineas} líneas, suma ${total}\")
",
    )
    .unwrap();
    let mut child = and23()
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"10
20
abc

5",
        )
        .unwrap();
    let out = child.wait_with_output().unwrap();
    std::fs::remove_file(&path).ok();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "5 líneas, suma 35
"
    );
}

#[test]
fn leer_un_archivo_inexistente_sin_atrapar_es_un_error_con_posicion() {
    let out = run_source(
        "sinarchivo",
        "show(leeArchivo(\"no_existe_nunca.txt\"))
",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("no se pudo leer `no_existe_nunca.txt`"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn ejemplo_de_tareas_suma_con_canales() {
    let out = run_example_from_root("tareas.jb", "");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "suma de cuadrados 1..10 = 385\n");
}

#[test]
fn un_interbloqueo_es_un_error_con_posicion() {
    let dir = std::env::temp_dir().join("and23_interbloqueo");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("p.jb");
    std::fs::write(&file, "fix c = Canal<Int>(1)\nshow(c.recibe())\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg(&file)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(stderr(&out).contains("interbloqueo"), "{}", stderr(&out));
}

#[test]
fn los_argumentos_tras_el_archivo_llegan_al_programa() {
    let dir = std::env::temp_dir().join("and23_args");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.jb");
    std::fs::write(&file, "show(argumentos())\nshow(argumentos().tam())\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg(&file)
        .args(["uno", "dos tres", "--max-depth", "--gc-stress"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "[\"uno\", \"dos tres\", \"--max-depth\", \"--gc-stress\"]\n4\n"
    );
    // Las opciones de la VM van antes del archivo.
    let out = Command::new(env!("CARGO_BIN_EXE_and23"))
        .args(["--gc-stress", "--max-depth", "500"])
        .arg(&file)
        .arg("x")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "[\"x\"]\n1\n");
}

#[test]
fn un_error_de_ejecucion_muestra_la_cadena_de_llamadas() {
    let dir = std::env::temp_dir().join("and23_traza");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.jb");
    std::fs::write(
        &file,
        "fn a(n: Int) -> Int { return 10 / n }\nfn b(n: Int) -> Int { return a(n) + 1 }\nshow(b(0))\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg(&file)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("t.jb:1:"), "{err}");
    assert!(err.contains("llamado desde `b` ("), "{err}");
    assert!(err.contains("t.jb:2:"), "{err}");
    assert!(err.contains("llamado desde el nivel superior ("), "{err}");
    assert!(err.contains("t.jb:3:"), "{err}");
}

/// Cada bloque ```jb del tutorial es un programa completo: tiene que ejecutarse sin errores.
#[test]
fn los_ejemplos_del_tutorial_se_ejecutan() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tutorial = std::fs::read_to_string(root.join("docs/tutorial.md")).unwrap();
    let dir = std::env::temp_dir().join("and23_tutorial");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut count = 0;
    for (i, chunk) in tutorial.split("```jb\n").skip(1).enumerate() {
        let code = chunk.split("```").next().unwrap();
        let file = dir.join(format!("bloque{i}.jb"));
        std::fs::write(&file, code).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_and23"))
            .arg(&file)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "bloque {i}:\n{code}\n{}",
            stderr(&out)
        );
        count += 1;
    }
    assert!(count >= 10, "solo {count} bloques");
    let _ = std::fs::remove_dir_all(&dir);
}

fn run_prueba(src: &str) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!("and23_prueba_{}", src.len()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("p.jb");
    std::fs::write(&file, src).unwrap();
    Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg("--prueba")
        .arg(&file)
        .output()
        .unwrap()
}

#[test]
fn las_pruebas_que_pasan_terminan_con_exito() {
    let out =
        run_prueba("prueba \"suma\" { afirmaIgual(1 + 1, 2) }\nprueba \"dos\" { afirma(true) }\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "prueba \"suma\" ... ok\nprueba \"dos\" ... ok\n\n2 prueba(s): 2 ok, 0 fallaron\n"
    );
}

#[test]
fn una_prueba_que_falla_se_informa_y_las_demas_siguen() {
    let out = run_prueba(
        "prueba \"mala\" { afirmaIgual(1, 2) }\nprueba \"buena\" { afirma(true) }\nprueba \"rota\" { show(1 / 0) }\n",
    );
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("prueba \"mala\" ... FALLÓ"), "{text}");
    assert!(text.contains("se esperaba 2, pero fue 1"), "{text}");
    assert!(text.contains("prueba \"buena\" ... ok"), "{text}");
    assert!(text.contains("prueba \"rota\" ... FALLÓ"), "{text}");
    assert!(text.contains("división entre cero"), "{text}");
    assert!(text.contains("3 prueba(s): 1 ok, 2 fallaron"), "{text}");
}

#[test]
fn sin_la_opcion_prueba_los_bloques_no_se_ejecutan_y_main_si() {
    let dir = std::env::temp_dir().join("and23_prueba_normal");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("p.jb");
    std::fs::write(
        &file,
        "prueba \"x\" { show(\"no\") }\nfn main() { show(\"main\") }\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(stdout(&out), "main\n");
    // En modo prueba `main` no se llama.
    let out = run_prueba("prueba \"x\" { show(\"dentro\") }\nfn main() { show(\"main\") }\n");
    assert_eq!(
        stdout(&out),
        "dentro\nprueba \"x\" ... ok\n\n1 prueba(s): 1 ok, 0 fallaron\n"
    );
}

#[test]
fn un_archivo_sin_pruebas_lo_dice() {
    let out = run_prueba("show(1)\n");
    assert!(out.status.success());
    assert!(stdout(&out).contains("no hay pruebas"), "{}", stdout(&out));
}

#[test]
fn el_ejemplo_de_pruebas_pasa() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg("--prueba")
        .arg("examples/pruebas.jb")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}{}", stdout(&out), stderr(&out));
    assert!(stdout(&out).contains("4 prueba(s): 4 ok, 0 fallaron"));
}

#[test]
fn las_pruebas_de_los_modulos_importados_no_se_ejecutan() {
    let dir = std::env::temp_dir().join("and23_prueba_modulos");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("lib.jb"),
        "open fn uno() -> Int = 1\nprueba \"de la libreria\" { afirma(false) }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.jb"),
        "use \"lib\"\nprueba \"propia\" { afirmaIgual(uno(), 1) }\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg("--prueba")
        .arg(dir.join("main.jb"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}{}", stdout(&out), stderr(&out));
    assert!(
        stdout(&out).contains("1 prueba(s): 1 ok"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn fmt_reescribe_el_archivo_y_check_solo_avisa() {
    let dir = std::env::temp_dir().join("and23_fmt");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("f.jb");
    std::fs::write(&file, "fn f() {\n\tshow(1)   \n\n\n\nshow(2)\n}\nf()\n").unwrap();
    let check = Command::new(env!("CARGO_BIN_EXE_and23"))
        .args(["fmt", "--check"])
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(1));
    assert!(stdout(&check).contains("sin formato"));
    // --check no escribe.
    assert!(std::fs::read_to_string(&file).unwrap().contains('\t'));
    let fmt = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg("fmt")
        .arg(&file)
        .output()
        .unwrap();
    assert!(fmt.status.success(), "{}", stderr(&fmt));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "fn f() {\n    show(1)\n\n    show(2)\n}\nf()\n"
    );
    let again = Command::new(env!("CARGO_BIN_EXE_and23"))
        .args(["fmt", "--check"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(again.status.success());
    assert_eq!(stdout(&again), "");
    // El programa formateado sigue haciendo lo mismo.
    let run = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(stdout(&run), "1\n2\n");
}

#[test]
fn fmt_sin_archivos_o_con_archivo_inexistente_es_un_error() {
    let none = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg("fmt")
        .output()
        .unwrap();
    assert_eq!(none.status.code(), Some(2));
    let missing = Command::new(env!("CARGO_BIN_EXE_and23"))
        .args(["fmt", "no_existe_qq.jb"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(stderr(&missing).contains("no se pudo leer"));
}

fn jb_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            jb_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "jb") {
            out.push(path);
        }
    }
}

/// Mn, paso 1: el lexer escrito en And23 (`mn/lexer.jb`) tiene que producir exactamente los mismos
/// tokens (y los mismos errores) que el de Rust en todos los ejemplos, el preludio, él mismo y los
/// casos límite de `mn/casos`.
#[test]
fn el_lexer_escrito_en_and23_coincide_con_el_de_rust() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![
        root.join("crates/and23-syntax/prelude.jb"),
        root.join("mn/lexer.jb"),
    ];
    jb_files(&root.join("examples"), &mut files);
    jb_files(&root.join("mn/casos"), &mut files);
    assert!(files.len() > 30, "{} archivos", files.len());
    for file in files {
        let rust = Command::new(env!("CARGO_BIN_EXE_and23"))
            .arg("--tokens")
            .arg(&file)
            .output()
            .unwrap();
        let and23 = Command::new(env!("CARGO_BIN_EXE_and23"))
            .arg("mn/lexer.jb")
            .arg(&file)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(and23.status.success(), "{file:?}: {}", stderr(&and23));
        assert_eq!(
            stdout(&and23),
            stdout(&rust),
            "tokens distintos en {file:?}"
        );
    }
}

/// Mn, etapa 2: el parser escrito en And23 (`mn/parser.jb`) produce el mismo árbol (y los mismos errores)
/// que el de Rust (`--ast`) en el preludio, los ejemplos, los dos archivos de `mn`, los casos del lexer y los
/// casos del parser (cada construcción y cada mensaje de error).
#[test]
fn el_parser_escrito_en_and23_coincide_con_el_de_rust() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![
        root.join("crates/and23-syntax/prelude.jb"),
        root.join("mn/lexer.jb"),
        root.join("mn/parser.jb"),
    ];
    jb_files(&root.join("examples"), &mut files);
    jb_files(&root.join("mn/casos"), &mut files);
    jb_files(&root.join("mn/casos_parser"), &mut files);
    assert!(files.len() > 100, "{} archivos", files.len());
    for file in files {
        let rust = Command::new(env!("CARGO_BIN_EXE_and23"))
            .arg("--ast")
            .arg(&file)
            .output()
            .unwrap();
        let and23 = Command::new(env!("CARGO_BIN_EXE_and23"))
            .arg("mn/parser.jb")
            .arg(&file)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(and23.status.success(), "{file:?}: {}", stderr(&and23));
        assert_eq!(
            stdout(&and23),
            stdout(&rust),
            "árboles distintos en {file:?}"
        );
    }
}

/// Mn, etapa 3: el verificador de tipos escrito en And23 (`mn/checker.jb`) da el mismo resultado que el de Rust
/// (`--check`): `ok` o el mismo error con el mismo mensaje y la misma posición. Se prueba con el preludio, los
/// ejemplos, los archivos de `mn` y los casos de `mn/casos_checker` (los programas de los tests del verificador y
/// uno por cada mensaje de error). Se reparte entre hilos; el verificador de And23 recibe cada grupo de archivos en un solo proceso.
#[test]
fn el_verificador_escrito_en_and23_coincide_con_el_de_rust() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![
        root.join("crates/and23-syntax/prelude.jb"),
        root.join("mn/lexer.jb"),
        root.join("mn/parser.jb"),
        root.join("mn/checker.jb"),
        root.join("mn/modulos.jb"),
    ];
    jb_files(&root.join("examples"), &mut files);
    jb_files(&root.join("mn/casos_checker"), &mut files);
    // Los tres archivos de Mn juntos y sin `use`: el verificador de And23 se verifica a sí mismo (y al lexer y
    // al parser) con el mismo resultado que el de Rust.
    let strip_main = |text: String| match text.find(
        "
fn main() {",
    ) {
        Some(i) => {
            text[..i].to_string()
                + "
"
        }
        None => text,
    };
    let read = |name: &str| std::fs::read_to_string(root.join(name)).unwrap();
    let combined = strip_main(read("mn/lexer.jb"))
        + &strip_main(read("mn/parser.jb")).replace(
            "use \"lexer.jb\"
",
            "",
        )
        + &read("mn/checker.jb")
            .replace(
                "use \"lexer.jb\"
",
                "",
            )
            .replace(
                "use \"parser.jb\"
",
                "",
            );
    let combined_path = std::env::temp_dir().join("and23_mn_combinado.jb");
    std::fs::write(&combined_path, combined).unwrap();
    files.push(combined_path);
    assert!(files.len() > 300, "{} archivos", files.len());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let chunk = files.len().div_ceil(threads);
    std::thread::scope(|scope| {
        for part in files.chunks(chunk) {
            let root = &root;
            scope.spawn(move || {
                // El verificador de And23 recibe todos los archivos del grupo de una vez: comprueba el preludio una
                // sola vez y, entre archivo y archivo, vuelve a su estado (también tras un error).
                let and23 = Command::new(env!("CARGO_BIN_EXE_and23"))
                    .arg("mn/checker.jb")
                    .args(part)
                    .current_dir(root)
                    .output()
                    .unwrap();
                assert!(and23.status.success(), "{}", stderr(&and23));
                let out = stdout(&and23);
                let lines: Vec<&str> = out.lines().collect();
                assert_eq!(
                    lines.len(),
                    part.len(),
                    "una línea por archivo:
{out}"
                );
                for (file, line) in part.iter().zip(lines) {
                    let rust = Command::new(env!("CARGO_BIN_EXE_and23"))
                        .arg("--check")
                        .arg(file)
                        .output()
                        .unwrap();
                    assert_eq!(
                        line,
                        stdout(&rust).trim_end(),
                        "resultados distintos en {file:?}"
                    );
                }
                // Además de `ok` o el error, lo que el verificador decidió para el compilador (campos, llamadas
                // virtuales, redirecciones...): una línea por decisión y `---` al final de cada archivo.
                let and23 = Command::new(env!("CARGO_BIN_EXE_and23"))
                    .arg("mn/checker.jb")
                    .arg("--resoluciones")
                    .args(part)
                    .current_dir(root)
                    .output()
                    .unwrap();
                assert!(and23.status.success(), "{}", stderr(&and23));
                let mut esperado = String::new();
                for file in part {
                    let rust = Command::new(env!("CARGO_BIN_EXE_and23"))
                        .arg("--resuelve")
                        .arg(file)
                        .output()
                        .unwrap();
                    esperado.push_str(&stdout(&rust));
                    esperado.push_str(
                        "---
",
                    );
                }
                assert_eq!(
                    stdout(&and23),
                    esperado,
                    "resoluciones distintas en {part:?}"
                );
            });
        }
    });
}

fn raiz_del_repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn la_tabla_de_nativas_de_mn_es_la_de_natives_rs() {
    // `mn/checker.jb` repite la tabla de `natives.rs` como texto; si cambia una, esta prueba obliga a cambiar la otra.
    let esperado: Vec<String> = and23_types::natives::NATIVES
        .iter()
        .map(|n| {
            let ps: Vec<String> = n.params.iter().map(|p| format!("{p:?}")).collect();
            format!("{}:{}:{:?}", n.name, ps.join(","), n.ret)
        })
        .collect();
    let fuente = std::fs::read_to_string(raiz_del_repo().join("mn/checker.jb")).unwrap();
    let ini = fuente
        .find("fix NATIVAS_TEXTO = \"")
        .expect("NATIVAS_TEXTO")
        + "fix NATIVAS_TEXTO = \"".len();
    let fin = ini + fuente[ini..].find('"').unwrap();
    assert_eq!(
        fuente[ini..fin],
        esperado.join(" "),
        "mn/checker.jb: NATIVAS_TEXTO no coincide con natives.rs"
    );
}

#[test]
fn la_gramatica_de_vscode_conoce_todas_las_funciones_predefinidas() {
    let gramatica = std::fs::read_to_string(
        raiz_del_repo().join("editors/vscode/syntaxes/and23.tmLanguage.json"),
    )
    .unwrap();
    let ini = gramatica.find("\"builtin\"").unwrap();
    let lista = &gramatica[ini..ini + gramatica[ini..].find("(?=").unwrap()];
    let mut nombres: Vec<String> = and23_types::BUILTINS
        .iter()
        .map(|s| s.to_string())
        .collect();
    nombres.push("show".into());
    nombres.extend(
        and23_types::natives::NATIVES
            .iter()
            .map(|n| n.name.to_string()),
    );
    // Funciones públicas del preludio (las que empiezan en la columna 0).
    let preludio =
        std::fs::read_to_string(raiz_del_repo().join("crates/and23-syntax/prelude.jb")).unwrap();
    for l in preludio.lines() {
        if let Some(r) = l.strip_prefix("fn ").or_else(|| l.strip_prefix("open fn ")) {
            nombres.push(r.split(['(', '<']).next().unwrap().to_string());
        }
    }
    let faltan: Vec<_> = nombres
        .iter()
        .filter(|n| !n.starts_with('_'))
        .filter(|n| {
            !lista.contains(&format!("|{n}|"))
                && !lista.contains(&format!("({n}|"))
                && !lista.contains(&format!("|{n})"))
        })
        .collect();
    assert!(
        faltan.is_empty(),
        "la gramática de VS Code no conoce: {faltan:?}"
    );
}

/// `mn/modulos.jb` (el cargador de `use`) escribe lo mismo que `and23 --unido`: el programa unido con las posiciones
/// sobre los archivos juntos y los avisos de «es privado», o el mismo error. Cubre `examples/proyecto`, los propios
/// archivos de Mn (que usan `use`) y `mn/casos_modulos` (un caso por cada error del cargador).
#[test]
fn el_cargador_de_modulos_escrito_en_and23_coincide_con_el_de_rust() {
    let root = raiz_del_repo();
    let mut entradas = vec![
        "examples/proyecto/main.jb".to_string(),
        "mn/checker.jb".to_string(),
        "mn/modulos.jb".to_string(),
        "mn/parser.jb".to_string(),
    ];
    let mut todos = Vec::new();
    jb_files(&root.join("mn/casos_modulos"), &mut todos);
    for f in todos {
        if f.file_name().is_some_and(|n| n == "main.jb") {
            let rel = f.strip_prefix(&root).unwrap_or(&f);
            entradas.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    assert!(entradas.len() > 20, "{} entradas", entradas.len());
    let hilos = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let trozo = entradas.len().div_ceil(hilos);
    std::thread::scope(|scope| {
        for parte in entradas.chunks(trozo) {
            let root = &root;
            scope.spawn(move || {
                for entrada in parte {
                    let rust = Command::new(env!("CARGO_BIN_EXE_and23"))
                        .arg("--unido")
                        .arg(entrada)
                        .current_dir(root)
                        .output()
                        .unwrap();
                    let and23 = Command::new(env!("CARGO_BIN_EXE_and23"))
                        .arg("mn/modulos.jb")
                        .arg(entrada)
                        .current_dir(root)
                        .output()
                        .unwrap();
                    assert!(and23.status.success(), "{entrada}: {}", stderr(&and23));
                    assert_eq!(
                        stdout(&and23),
                        stdout(&rust),
                        "resultados distintos en {entrada}"
                    );
                }
            });
        }
    });
}

#[test]
fn dump_vuelca_el_modulo_con_posiciones_en_caracteres() {
    // Es el texto con el que se compara el compilador escrito en And23: tablas, funciones, instrucciones y la
    // posición (en caracteres) de cada una. Los números de función dependen del preludio, así que no se fijan.
    let src = "show(\"ñ\" + \"b\")\nfn f(x: Int) -> Int = x * 2\nthing P(fix a: Int)\n";
    let out = run_dump(src);
    assert!(out.starts_with("Modulo\nCadenas "), "{out}");
    assert!(out.contains("\nClases "), "{out}");
    assert!(out.contains(" P campos=[a] supers=["), "{out}");
    assert!(
        out.contains("f/1 @19-20\n  0000 GetLocal 0 @38-39\n  0001 Int 2 @42-43\n  0002 Mul @38-43\n  0003 Return @38-43\n"),
        "{out}"
    );
    // Los números de las cadenas nuevas dependen del tamaño del preludio: se leen del propio volcado.
    let primera: u32 = out
        .lines()
        .find_map(|l| l.strip_prefix("  ")?.strip_suffix(" \"ñ\"")?.parse().ok())
        .expect("la cadena \"ñ\" está en la tabla");
    let esperado = format!(
        "<script>/0 @0-0\n  0000 Str {primera} @5-8\n  0001 Str {} @11-14\n  0002 Add @5-14\n  0003 Show @0-15\n",
        primera + 1
    );
    assert!(out.contains(&esperado), "{out}");
    // Un programa solo vuelca lo suyo: el preludio tiene su propio volcado.
    let prelude = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg("--dump-preludio")
        .output()
        .unwrap();
    let prelude = stdout(&prelude);
    assert!(
        prelude.contains("\nfn 0 "),
        "el preludio empieza en la función 0"
    );
    assert!(!out.contains("\nfn 0 "), "{out}");
}

fn run_dump(src: &str) -> String {
    let path = std::env::temp_dir().join("and23_dump_prueba.jb");
    std::fs::write(&path, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg("--dump")
        .arg(&path)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

/// Mn, etapa 4: el compilador escrito en And23 (`mn/compilador.jb`) produce el mismo módulo que el de Rust
/// (`and23 --dump`): tablas, funciones, instrucciones y posiciones, o el mismo error. Cubre el preludio, los
/// ejemplos, los casos del verificador y de los módulos, y los propios archivos de Mn (él mismo incluido).
#[test]
fn el_compilador_escrito_en_and23_coincide_con_el_de_rust() {
    let root = raiz_del_repo();
    // El preludio entero.
    let rust = Command::new(env!("CARGO_BIN_EXE_and23"))
        .arg("--dump-preludio")
        .output()
        .unwrap();
    let and23 = Command::new(env!("CARGO_BIN_EXE_and23"))
        .args(["mn/compilador.jb", "--preludio"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(and23.status.success(), "{}", stderr(&and23));
    assert_eq!(
        stdout(&and23).trim_end(),
        stdout(&rust).trim_end(),
        "el preludio compila distinto"
    );
    let mut files = Vec::new();
    jb_files(&root.join("examples"), &mut files);
    jb_files(&root.join("mn/casos_checker"), &mut files);
    jb_files(&root.join("mn/casos_modulos"), &mut files);
    files.retain(|f| {
        let path = f.to_string_lossy().replace('\\', "/");
        // Los programas de varios archivos se cargan por su principal.
        !path.contains("/casos_modulos/") || f.file_name().is_some_and(|n| n == "main.jb")
    });
    files.retain(|f| {
        let path = f.to_string_lossy().replace('\\', "/");
        !path.contains("/examples/proyecto/") || f.file_name().is_some_and(|n| n == "main.jb")
    });
    for name in [
        "lexer.jb",
        "parser.jb",
        "checker.jb",
        "modulos.jb",
        "compilador.jb",
    ] {
        files.push(root.join("mn").join(name));
    }
    assert!(files.len() > 300, "{} archivos", files.len());
    let rel = |f: &PathBuf| {
        f.strip_prefix(&root)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let hilos = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let trozo = files.len().div_ceil(hilos);
    std::thread::scope(|scope| {
        for parte in files.chunks(trozo) {
            let (root, rel) = (&root, &rel);
            scope.spawn(move || {
                let nombres: Vec<String> = parte.iter().map(rel).collect();
                let and23 = Command::new(env!("CARGO_BIN_EXE_and23"))
                    .arg("mn/compilador.jb")
                    .args(&nombres)
                    .current_dir(root)
                    .output()
                    .unwrap();
                assert!(and23.status.success(), "{}", stderr(&and23));
                let mut esperado = String::new();
                for nombre in &nombres {
                    let rust = Command::new(env!("CARGO_BIN_EXE_and23"))
                        .arg("--dump")
                        .arg(nombre)
                        .current_dir(root)
                        .output()
                        .unwrap();
                    esperado.push_str(&stdout(&rust));
                    esperado.push_str("---\n");
                }
                let obtenido = stdout(&and23);
                if obtenido != esperado {
                    let (a, b) = (obtenido.lines(), esperado.lines());
                    let linea = a.zip(b).position(|(x, y)| x != y).unwrap_or(0);
                    panic!(
                        "el compilador de And23 difiere en el grupo {nombres:?} (línea {linea}):\n{:?}\n{:?}",
                        obtenido.lines().nth(linea),
                        esperado.lines().nth(linea)
                    );
                }
            });
        }
    });
}

/// Mn, etapa 5: los módulos que compila `mn/compilador.jb` se ejecutan en la VM (`and23 --carga`) y se comportan
/// como los de Rust. Los ejemplos dan la misma salida; el verificador de Mn compilado por Mn da los mismos
/// resultados; y el compilador de Mn compilado por Mn llega a un punto fijo: compilándose a sí mismo (y al preludio)
/// escribe exactamente lo mismo que el compilado por Rust.
#[test]
fn el_compilador_de_and23_compilado_por_el_mismo_llega_a_un_punto_fijo() {
    let root = raiz_del_repo();
    let dir = std::env::temp_dir().join(format!("and23-etapa5-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_and23"))
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        stdout(&out)
    };
    let write = |name: &str, text: &str| {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path.to_string_lossy().into_owned()
    };
    // Primera generación: el compilador de Mn, ejecutado por la VM porque lo compiló Rust.
    let prelude = write("preludio.txt", &run(&["mn/compilador.jb", "--preludio"]));
    let gen1 = run(&["mn/compilador.jb", "mn/compilador.jb"]);
    let compilador = write("compilador.txt", &gen1);

    // Segunda generación: ese mismo compilador, ya compilado por Mn, compila el preludio y a sí mismo.
    let pre2 = run(&["--carga", &prelude, &compilador, "--preludio"]);
    assert_eq!(
        pre2,
        std::fs::read_to_string(&prelude).unwrap(),
        "el preludio cambia"
    );
    let gen2 = run(&["--carga", &prelude, &compilador, "mn/compilador.jb"]);
    assert_eq!(
        gen2, gen1,
        "el compilador compilado por sí mismo es distinto"
    );

    // Y compila otros archivos igual que Rust.
    let archivos = [
        "examples/clases.jb",
        "examples/genericos.jb",
        "examples/lambdas.jb",
        "mn/lexer.jb",
        "mn/parser.jb",
        "mn/checker.jb",
    ];
    let mut args = vec!["--carga", prelude.as_str(), compilador.as_str()];
    args.extend(archivos);
    let por_mn = run(&args);
    let mut esperado = String::new();
    for a in archivos {
        esperado.push_str(&run(&["--dump", a]));
        esperado.push_str("---\n");
    }
    assert_eq!(
        por_mn, esperado,
        "el compilador compilado por Mn compila distinto"
    );

    // Los ejemplos (sin entrada) dan la misma salida ejecutados desde el volcado.
    for nombre in [
        "aritmetica",
        "cadenas",
        "clases",
        "control",
        "enumerados",
        "excepciones",
        "funciones",
        "genericos",
        "lambdas",
        "notas",
        "palabras",
        "polimorfismo",
        "tareas",
    ] {
        let fuente = format!("examples/{nombre}.jb");
        let volcado = write(
            &format!("{nombre}.txt"),
            &run(&["mn/compilador.jb", &fuente]),
        );
        assert_eq!(
            run(&["--carga", &prelude, &volcado]),
            run(&[&fuente]),
            "{nombre} se comporta distinto desde el volcado"
        );
    }

    // El verificador de Mn, compilado por Mn, da lo mismo que ejecutado como programa de Rust.
    let verificador = write("checker.txt", &run(&["mn/compilador.jb", "mn/checker.jb"]));
    let casos = [
        "examples/clases.jb",
        "examples/enumerados.jb",
        "mn/parser.jb",
    ];
    let mut con_mn = vec!["--carga", prelude.as_str(), verificador.as_str()];
    con_mn.extend(casos);
    let mut con_rust = vec!["mn/checker.jb"];
    con_rust.extend(casos);
    assert_eq!(run(&con_mn), run(&con_rust));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Mn, 0.1: el `Fallo` de una tarea que nadie espera no se pierde: se avisa al terminar y el código de salida es 1.
#[test]
fn el_fallo_de_una_tarea_que_nadie_espera_se_avisa_al_terminar() {
    let out = run_source(
        "tarea_perdida",
        "fn main() {
    fix a = lanza { fail Fallo(\"uno\") }
    fix b = lanzaDe { fail Fallo(\"dos\")\n 3 }
    fix c = lanza { fail Fallo(\"tres\") }
    attempt { a.espera() } rescue e: Fallo { show(\"capturado ${e.msg}\") }
    attempt { b.espera() } rescue e: Fallo { show(\"capturado ${e.msg}\") }
    show(\"fin\")
}",
    );
    assert_eq!(
        stdout(&out),
        "capturado uno\ncapturado dos\nfin\n",
        "{}",
        stderr(&out)
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr(&out),
        "error: 1 tarea(s) fallaron y nadie las esperó: tres\n"
    );
}

#[test]
fn si_todas_las_tareas_que_fallan_se_esperan_no_hay_aviso() {
    let out = run_source(
        "tarea_vista",
        "fn main() {
    fix a = lanza { fail Fallo(\"uno\") }
    attempt { a.espera() } rescue e: Fallo { show(e.msg) }
}",
    );
    assert_eq!(stdout(&out), "uno\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stderr(&out), "");
}

#[test]
fn una_prueba_con_una_tarea_perdida_falla() {
    let out = run_prueba(
        "prueba \"huerfana\" {
    lanza { fail Fallo(\"x\") }
    cede()
}
prueba \"limpia\" { afirma(true) }\n",
    );
    let text = stdout(&out);
    assert!(text.contains("prueba \"huerfana\" ... FALLÓ"), "{text}");
    assert!(text.contains("nadie las esperó: x"), "{text}");
    assert!(text.contains("prueba \"limpia\" ... ok"), "{text}");
    assert_eq!(out.status.code(), Some(1));
}

/// Mn con el recolector en cada punto seguro: el lexer y el parser escritos en And23 (programas grandes, con mucha asignación)
/// dan lo mismo con `--gc-stress` que sin él. El verificador y el compilador de Mn tardan minutos así (revisan el preludio
/// entero en cada punto seguro), por eso no entran.
#[test]
fn el_lexer_y_el_parser_de_mn_dan_lo_mismo_con_gc_stress() {
    let root = raiz_del_repo();
    for programa in ["mn/lexer.jb", "mn/parser.jb"] {
        let ejecuta = |stress: bool| {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_and23"));
            if stress {
                cmd.arg("--gc-stress");
            }
            let out = cmd
                .args([programa, "examples/aritmetica.jb"])
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(out.status.success(), "{programa}: {}", stderr(&out));
            stdout(&out)
        };
        let normal = ejecuta(false);
        assert!(normal.lines().count() > 20, "{programa}: salida corta");
        assert_eq!(normal, ejecuta(true), "{programa} cambia con --gc-stress");
    }
}

/// Los programas de `benchmarks/` (los que mide `scripts/rendimiento.sh`) tienen que seguir ejecutándose sin error.
#[test]
fn los_benchmarks_se_ejecutan() {
    let root = raiz_del_repo();
    for nombre in ["bucle", "colecciones", "cadenas", "tareas"] {
        let out = Command::new(env!("CARGO_BIN_EXE_and23"))
            .arg(format!("benchmarks/{nombre}.jb"))
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(out.status.success(), "{nombre}: {}", stderr(&out));
        assert!(!stdout(&out).is_empty(), "{nombre} no escribe nada");
    }
}

/// Dividir un texto grande en muchos trozos es lineal: con 60 000 trozos tarda décimas de segundo (cuando costaba O(n·k)
/// eran varios segundos, y con los 100 000 del benchmark, 24 s).
#[test]
fn partir_un_texto_grande_no_es_cuadratico() {
    let inicio = std::time::Instant::now();
    let out = run_source(
        "parte_grande",
        "fn main() {
    fix p = Lista<Str>()
    mut i = 0
    while i < 60000 {
        p.agrega(\"elemento ${i}\")
        i += 1
    }
    fix t = une(p, \",\")
    show(t.parte(\",\").tam())
    show(t.parte(\"\").tam())
}",
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "60000\n888889\n");
    assert!(
        inicio.elapsed() < std::time::Duration::from_secs(5),
        "tardó {:?}",
        inicio.elapsed()
    );
}

/// Paso 2.1: una lambda local con el tipo anotado puede llamarse a sí misma (en funciones, bloques y métodos), también
/// capturando otras variables, anidada, con `mut`, y con el recolector en cada punto seguro.
#[test]
fn una_lambda_local_puede_llamarse_a_si_misma() {
    let fuente =
        std::fs::read_to_string(raiz_del_repo().join("mn/casos_checker/x_lambda_rec_varias.jb"))
            .unwrap();
    for stress in [false, true] {
        let path =
            std::env::temp_dir().join(format!("and23_lrec_{stress}_{}.jb", std::process::id()));
        std::fs::write(&path, &fuente).unwrap();
        let mut cmd = and23();
        if stress {
            cmd.arg("--gc-stress");
        }
        let out = cmd.arg(&path).output().unwrap();
        std::fs::remove_file(&path).ok();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(
            stdout(&out),
            "3628800\n110\n5\n3\n42\n8\n[6, 24]\n",
            "stress={stress}"
        );
    }
    let metodo =
        std::fs::read_to_string(raiz_del_repo().join("mn/casos_checker/x_lambda_rec_metodo.jb"))
            .unwrap();
    assert_eq!(stdout(&run_source("lrec_metodo", &metodo)), "20\n");
}

#[test]
fn una_lambda_sin_tipo_anotado_no_puede_llamarse_a_si_misma_y_el_error_lo_explica() {
    let out = run_source(
        "lrec_sin_tipo",
        "fn main() {\n    fix f = { n: Int -> if n <= 0 { return 0 } else { return f(n - 1) } }\n    show(f(1))\n}\n",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("la función `f` no está definida (para que una lambda se llame a sí misma, anota su tipo: `fix f: (Int) -> Int = { ... }`)"),
        "{}",
        stderr(&out)
    );
}

/// Paso 2.3: `x.formato(decimales)` escribe un Float con ese número de decimales (redondeo exacto del valor binario).
#[test]
fn float_formato_escribe_con_los_decimales_pedidos() {
    let out = and23()
        .arg(raiz_del_repo().join("mn/casos_checker/x_float_formato.jb"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "3.14\n3\n3.14159\n2.67\n2\n4\n-1.00\n0.10000000000000000555\n0.3333\n100000000000000000000.0\n100.00 €\ntotal: 59.97\nNaN\n"
    );
}

#[test]
fn float_formato_rechaza_decimales_fuera_de_rango() {
    for decimales in ["-1", "101"] {
        let out = run_source(
            "formato_rango",
            &format!("fn main() {{ show(1.5.formato({decimales})) }}"),
        );
        assert_eq!(out.status.code(), Some(1));
        assert!(
            stderr(&out).contains(&format!(
                "formato: los decimales deben estar entre 0 y 100, pero son {decimales}"
            )),
            "{}",
            stderr(&out)
        );
    }
}

/// `?.` sobre un `Str?` o un `Float?` llama al método si hay valor y da `nil` si no (antes solo valía con objetos).
#[test]
fn acceso_seguro_sobre_str_y_float_opcionales() {
    let esperado = "4\nnil\n-1\n3\n-1\nHOLA\ntrue\nfalse\n[\"h\", \"la\"]\nol\n2.5\nnil\nsin valor\nANA\nnil\n4.2\n-\n3\nnil\n";
    for stress in [false, true] {
        let mut cmd = and23();
        if stress {
            cmd.arg("--gc-stress");
        }
        let out = cmd
            .arg(raiz_del_repo().join("mn/casos_checker/x_safe_str_float.jb"))
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), esperado, "stress={stress}");
    }
}

/// Paso 2.5: `x ?: return`, `x ?: fail ...` y `x!!`.
#[test]
fn elvis_con_return_o_fail_y_bang_bang() {
    let esperado =
        "40\n-1\n4\nerror: falta z\n8\nel valor es nil, pero se esperaba uno (!!)\nANA\n6\n4\n";
    for stress in [false, true] {
        let mut cmd = and23();
        if stress {
            cmd.arg("--gc-stress");
        }
        let out = cmd
            .arg(raiz_del_repo().join("mn/casos_checker/x_exit_elvis_bang.jb"))
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), esperado, "stress={stress}");
    }
    // Un `!!` sobre nil sin capturar termina el programa con el mensaje.
    let out = run_source(
        "bang_nil",
        "fn main() {\n    fix v: Int? = nil\n    show(v!!)\n}\n",
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("el valor es nil, pero se esperaba uno (!!)"),
        "{}",
        stderr(&out)
    );
}

/// Paso 2.9: un `if`/`else` (o una cadena de `else if`) como último valor de una lambda.
#[test]
fn if_else_como_valor_final_de_una_lambda() {
    let casos = [
        (
            "x_lambda_if_valor.jb",
            "1\n10\n720\nnegativo\ncero\npositivo\nANA\nnadie\n6\n17\n[3, 4]\n[1, 2]\n[1, 4, 5]\n2\nnil\n",
        ),
        (
            "x_lambda_if_anidado.jb",
            "[\"muy negativo\", \"negativo\", \"chico\", \"chico\", \"mediano\", \"grande\"]\n",
        ),
    ];
    for (archivo, esperado) in casos {
        for stress in [false, true] {
            let mut cmd = and23();
            if stress {
                cmd.arg("--gc-stress");
            }
            let out = cmd
                .arg(raiz_del_repo().join("mn/casos_checker").join(archivo))
                .output()
                .unwrap();
            assert!(out.status.success(), "{archivo}: {}", stderr(&out));
            assert_eq!(stdout(&out), esperado, "{archivo} stress={stress}");
        }
    }
}

/// El REPL con `use` (paso 2.6): carga módulos relativos a la carpeta actual, recuerda nombres y alias entre entradas, carga cada
/// módulo una sola vez y muestra los errores con el archivo que corresponde.
#[test]
fn repl_con_use_de_modulos() {
    let dir = std::env::temp_dir().join(format!("and23_repl_use_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(
        dir.join("lib/mate.jb"),
        "open fn doble(x: Int) -> Int = x * 2\nopen fn parte(a: Int, b: Int) -> Int = a / b\nfn secreto() -> Int = 7\nopen fix PI = 3\nshow(\"mate cargado\")\n",
    )
    .unwrap();
    let mut child = and23()
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let entrada = "\
use \"lib/mate\"
doble(21)
use \"lib/mate\" como m
m.doble(4) + PI
secreto()
m.secreto()
parte(1, 0)
use \"lib/no_existe\"
fix x = doble(1)
x + 1
";
    child
        .stdin
        .take()
        .unwrap()
        .write_all(entrada.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let (salida, errores) = (stdout(&out), stderr(&out));
    std::fs::remove_dir_all(&dir).ok();
    // El módulo se carga una sola vez aunque se importe dos veces; los valores de las expresiones salen como siempre.
    assert_eq!(salida, "mate cargado\n42\n11\n3\n", "{errores}");
    assert!(
        errores.contains("la variable `secreto` no está definida") || errores.contains("secreto"),
        "{errores}"
    );
    assert!(errores.contains("división entre cero"), "{errores}");
    // Un error dentro del módulo señala el archivo del módulo, no la entrada.
    assert!(errores.contains("lib/mate.jb"), "{errores}");
    assert!(
        errores.contains("no se encontró el archivo `lib/no_existe.jb`"),
        "{errores}"
    );
}

/// Paso 2.7: constructores (`Persona`) y métodos (`obj.metodo`) usados como valores.
#[test]
fn constructores_y_metodos_como_valores() {
    let esperado = "Ana\n25\n3\n25\n18\n18\n21\n36\n[\"a\", \"b\"]\n[2, 3, 4]\n";
    for stress in [false, true] {
        let mut cmd = and23();
        if stress {
            cmd.arg("--gc-stress");
        }
        let out = cmd
            .arg(raiz_del_repo().join("mn/casos_checker/x_ref_metodos_ctor.jb"))
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), esperado, "stress={stress}");
    }
    // Un campo de tipo función gana sobre un método del mismo nombre; el método sigue disponible con otro nombre.
    let out = and23()
        .arg(raiz_del_repo().join("mn/casos_checker/x_ref_campo_gana.jb"))
        .output()
        .unwrap();
    assert_eq!(stdout(&out), "101\n8\n");
}

/// `docs/biblioteca.md` menciona cada función, clase y método público del preludio y cada función nativa pública: si se añade
/// uno y no se documenta, este test lo avisa.
#[test]
fn la_referencia_de_la_biblioteca_documenta_todo_lo_publico() {
    let root = raiz_del_repo();
    let doc = std::fs::read_to_string(root.join("docs/biblioteca.md")).unwrap();
    let prelude = std::fs::read_to_string(root.join("crates/and23-syntax/prelude.jb")).unwrap();
    let palabras: std::collections::HashSet<&str> = doc
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect();
    // Internos aunque no empiecen por `_`: no son API.
    let internos = [
        "Entrada",
        "cubetaDe",
        "crece",
        "compacta",
        "corre",
        "FalloDeTarea",
    ];
    let mut faltan: Vec<String> = Vec::new();
    let mut revisa = |nombre: &str| {
        if !nombre.starts_with('_') && !internos.contains(&nombre) && !palabras.contains(nombre) {
            faltan.push(nombre.to_string());
        }
    };
    let mut en_clase = false;
    for line in prelude.lines() {
        if let Some(rest) = line.strip_prefix("fn ") {
            en_clase = false;
            let nombre: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // `_str_largo` es el método `largo` de `Str`; `_int_parse` y `_float_parse`, `parse`.
            let publico = ["_str_", "_int_", "_float_"]
                .iter()
                .find_map(|p| nombre.strip_prefix(p).map(str::to_string));
            revisa(&publico.unwrap_or(nombre));
        } else if line.starts_with("thing ")
            || line.starts_with("open thing ")
            || line.starts_with("kind ")
        {
            let nombre: String = line
                .split_whitespace()
                .find(|w| w.chars().next().is_some_and(char::is_uppercase))
                .unwrap_or("")
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            en_clase = !nombre.is_empty() && !internos.contains(&nombre.as_str());
            revisa(&nombre);
        } else if let Some(rest) = line.strip_prefix("    fn ") {
            if en_clase {
                let nombre: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                revisa(&nombre);
            }
        } else if line.starts_with('}') {
            en_clase = false;
        }
    }
    for native in and23_types::NATIVES.iter() {
        revisa(native.name);
    }
    for integrada in and23_types::BUILTINS {
        revisa(integrada);
    }
    faltan.sort();
    faltan.dedup();
    assert!(
        faltan.is_empty(),
        "docs/biblioteca.md no menciona: {faltan:?}"
    );
}

/// Los bloques de código de la documentación: cada bloque ```jb es un programa que verifica sin errores, y cada bloque ```jb-error
/// es un programa que el verificador rechaza. Así los ejemplos de los documentos no se desactualizan.
fn bloques_de_codigo(texto: &str) -> Vec<(String, String, usize)> {
    let mut bloques = Vec::new();
    let mut actual: Option<(String, String, usize)> = None;
    for (n, linea) in texto.lines().enumerate() {
        match (&mut actual, linea.trim_end()) {
            (None, l) if l.starts_with("```jb") => {
                actual = Some((l[3..].to_string(), String::new(), n + 1));
            }
            (Some(_), "```") => bloques.push(actual.take().unwrap()),
            (Some((_, codigo, _)), l) => {
                codigo.push_str(l);
                codigo.push('\n');
            }
            _ => {}
        }
    }
    bloques
}

fn verifica_texto(src: &str) -> Result<(), String> {
    let mut checker = and23_types::Checker::new();
    let prelude = and23_syntax::parse(and23_syntax::PRELUDE).map_err(|d| d.message)?;
    checker.check(&prelude).map_err(|d| d.message)?;
    checker.seal_prelude();
    let program = and23_syntax::parse(src).map_err(|d| d.message)?;
    checker.check(&program).map(|_| ()).map_err(|d| d.message)
}

#[test]
fn los_bloques_de_codigo_de_la_documentacion_son_validos() {
    let root = raiz_del_repo();
    let mut errores = Vec::new();
    let mut total = 0;
    for doc in ["docs/tutorial.md", "docs/lenguaje.md", "docs/errores.md"] {
        let Ok(texto) = std::fs::read_to_string(root.join(doc)) else {
            continue;
        };
        for (tipo, codigo, linea) in bloques_de_codigo(&texto) {
            total += 1;
            let resultado = verifica_texto(&codigo);
            let esperaba_error = tipo == "jb-error";
            match (resultado, esperaba_error) {
                (Err(e), false) => errores.push(format!("{doc}:{linea}: debía ser válido: {e}")),
                (Ok(()), true) => {
                    errores.push(format!("{doc}:{linea}: debía dar un error de tipos"))
                }
                _ => {}
            }
        }
    }
    assert!(errores.is_empty(), "{}", errores.join("\n"));
    assert!(total > 10, "solo {total} bloques");
}

/// `salir(n)` termina con ese código, sin poder capturarse, y `muestraError` escribe por la salida de errores.
#[test]
fn salir_y_muestra_error() {
    let out = run_source(
        "salir1",
        "fn main() {\n    show(\"antes\")\n    muestraError(\"un error\")\n    attempt { salir(3) } rescue e: Fallo { show(\"no debe salir\") }\n    show(\"despues\")\n}\n",
    );
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(stdout(&out), "antes\n");
    assert_eq!(stderr(&out), "un error\n");
    // Una salida desde el nivel superior no llama a `main`; el código solo cuenta los 8 bits bajos.
    let out = run_source(
        "salir2",
        "show(\"a\")\nsalir(256 + 7)\nshow(\"b\")\nfn main() { show(\"main\") }\n",
    );
    assert_eq!(out.status.code(), Some(7));
    assert_eq!(stdout(&out), "a\n");
    // `salir(0)` también termina con éxito sin ejecutar lo que sigue.
    let out = run_source("salir3", "show(1)\nsalir(0)\nshow(2)\n");
    assert!(out.status.success());
    assert_eq!(stdout(&out), "1\n");
}

/// El buscador (`proyectos/buscador`) como herramienta de línea de comandos: busca, cuenta y usa los códigos de salida.
#[test]
fn el_buscador_de_proyectos_funciona_como_herramienta() {
    let root = raiz_del_repo();
    let ejecuta = |args: &[&str]| {
        and23()
            .current_dir(&root)
            .arg("proyectos/buscador/buscador.jb")
            .args(args)
            .output()
            .unwrap()
    };
    let con = ejecuta(&["Fallo", "examples/excepciones.jb", "-n"]);
    assert_eq!(con.status.code(), Some(0), "{}", stderr(&con));
    assert!(
        stdout(&con).contains("examples/excepciones.jb:"),
        "{}",
        stdout(&con)
    );
    assert!(
        stdout(&con).trim_end().ends_with("en 1 archivo(s)"),
        "{}",
        stdout(&con)
    );
    let sin = ejecuta(&[
        "esto_no_aparece_en_ningun_lado_zzz",
        "examples/excepciones.jb",
    ]);
    assert_eq!(sin.status.code(), Some(1));
    assert!(stdout(&sin).contains("0 coincidencia(s)"));
    let mal = ejecuta(&["x", "--rara"]);
    assert_eq!(mal.status.code(), Some(2));
    assert!(
        stderr(&mal).contains("opción desconocida: --rara"),
        "{}",
        stderr(&mal)
    );
    assert_eq!(stdout(&mal), "");
    // Sus pruebas.
    let pruebas = and23()
        .current_dir(&root)
        .args(["--prueba", "proyectos/buscador/buscador_prueba.jb"])
        .output()
        .unwrap();
    assert!(
        pruebas.status.success(),
        "{}{}",
        stdout(&pruebas),
        stderr(&pruebas)
    );
    assert!(
        stdout(&pruebas).contains("5 prueba(s): 5 ok, 0 fallaron"),
        "{}",
        stdout(&pruebas)
    );
}

/// La calculadora (`proyectos/calculadora`): una sesión por la entrada estándar, una expresión por argumento y sus pruebas.
#[test]
fn la_calculadora_de_proyectos_funciona() {
    let root = raiz_del_repo();
    let mut hijo = and23()
        .current_dir(&root)
        .arg("proyectos/calculadora/calc.jb")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    hijo.stdin
        .take()
        .unwrap()
        .write_all(b"r = 5\npi * r ^ 2\n2 ^ 3 ^ 2\n-3 + 10 / 4\n1 / 0\nx + 1\n2 +\nsalir\n99\n")
        .unwrap();
    let out = hijo.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "78.5398\n512.0000\n-0.5000\nerror: división entre cero\nerror: la variable `x` no existe\nerror: la expresión está incompleta\n"
    );
    let uno = and23()
        .current_dir(&root)
        .args(["proyectos/calculadora/calc.jb", "raiz(16) + 1"])
        .output()
        .unwrap();
    assert_eq!(stdout(&uno), "5.0000\n");
    let malo = and23()
        .current_dir(&root)
        .args(["proyectos/calculadora/calc.jb", "1 +"])
        .output()
        .unwrap();
    assert_eq!(malo.status.code(), Some(1));
    assert!(
        stderr(&malo).contains("error: la expresión está incompleta"),
        "{}",
        stderr(&malo)
    );
    let pruebas = and23()
        .current_dir(&root)
        .args(["--prueba", "proyectos/calculadora/calc_prueba.jb"])
        .output()
        .unwrap();
    assert!(
        pruebas.status.success(),
        "{}{}",
        stdout(&pruebas),
        stderr(&pruebas)
    );
    assert!(
        stdout(&pruebas).contains("6 prueba(s): 6 ok, 0 fallaron"),
        "{}",
        stdout(&pruebas)
    );
}
