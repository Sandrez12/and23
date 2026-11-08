//! Semántica de la VM fijada con salidas esperadas: operadores, variables, ámbitos, bucles, funciones, errores de ejecución y el estado
//! persistente del REPL. (Hasta M11 estas salidas se comparaban con las de un intérprete de árbol; al retirarlo en 1.0 se fijaron aquí.)

use and23_syntax::parse;
use and23_vm::Session;
use std::path::Path;

type Outcome = Result<String, String>;
/// Lo que se espera de un programa: su salida o su mensaje de error.
type Expected = Result<&'static str, &'static str>;

/// Límite de llamadas con el que se fijaron las salidas esperadas (el de entonces).
const DEPTH: usize = 10_000;

fn vm(src: &str, echo: bool) -> Outcome {
    let program = parse(src).map_err(|d| d.message)?;
    let mut session = Session::new(Vec::new()).with_max_depth(DEPTH);
    let result = session
        .run(&program, echo)
        .and_then(|()| session.run_main())
        .map_err(|d| d.message);
    result.map(|()| String::from_utf8(session.out().clone()).unwrap())
}

/// Los programas que el verificador de tipos rechaza ya no llegan a ejecutarse.
fn well_typed(src: &str) -> bool {
    let mut checker = and23_types::Checker::new();
    let prelude = parse(and23_syntax::PRELUDE).expect("preludio válido");
    checker.check(&prelude).expect("preludio válido");
    checker.seal_prelude();
    parse(src).is_ok_and(|p| checker.check(&p).is_ok())
}

fn owned(r: Expected) -> Outcome {
    r.map(String::from).map_err(String::from)
}

/// Programa, y lo que da sin y con `echo` (el modo REPL, que imprime también las expresiones sueltas).
const PROGRAMAS: &[(&str, Expected, Expected)] = &[
    ("show(1 + 2 * 3)\nshow(10 / 4)\nshow(10 / 4.0)\nshow(-7 % 3)\nshow(1 == 1.0)", Ok("7\n2\n2.5\n-1\ntrue\n"), Ok("7\n2\n2.5\n-1\ntrue\n")),
    ("show(1 / 0)", Err("división entre cero"), Err("división entre cero")),
    ("show(1.0 / 0.0)", Err("división entre cero"), Err("división entre cero")),
    ("show(9223372036854775807 + 1)", Err("desbordamiento de entero (i64)"), Err("desbordamiento de entero (i64)")),
    ("show(-(0 - 9223372036854775807 - 1))", Err("desbordamiento de entero (i64)"), Err("desbordamiento de entero (i64)")),
    ("show(1 + true)", Err("la operación `+` no admite Int y Bool"), Err("la operación `+` no admite Int y Bool")),
    ("show(!1)", Err("el operador `!` no admite Int"), Err("el operador `!` no admite Int")),
    ("show(-true)", Err("el operador `-` no admite Bool"), Err("el operador `-` no admite Bool")),
    ("show(true && 1)", Err("el operador `&&` requiere Bool, pero recibió Int"), Err("el operador `&&` requiere Bool, pero recibió Int")),
    ("show(false && 1)", Err("el operador `&&` requiere Bool, pero recibió Int"), Err("el operador `&&` requiere Bool, pero recibió Int")),
    ("show(true || 1)", Err("el operador `||` requiere Bool, pero recibió Int"), Err("el operador `||` requiere Bool, pero recibió Int")),
    ("show(1 || true)", Err("el operador `||` requiere Bool, pero recibió Int"), Err("el operador `||` requiere Bool, pero recibió Int")),
    ("show(true && false || true)", Ok("true\n"), Ok("true\n")),
    ("mut a = 0\nfn t() -> Bool { a += 1\n return true }\nshow(false && t())\nshow(true || t())\nshow(a)", Ok("false\ntrue\n0\n"), Ok("false\ntrue\n0\n")),
    ("fix x = 1\nx = 2", Err("no se puede asignar a `x` porque es inmutable; decláralo con `mut`"), Err("no se puede asignar a `x` porque es inmutable; decláralo con `mut`")),
    ("mut x = 1\nx += 2\nx *= 5\nshow(x)", Ok("15\n"), Ok("15\n")),
    ("show(y)", Err("la variable `y` no está definida"), Err("la variable `y` no está definida")),
    ("fn f() = f2()\nshow(f())", Err("la función `f2` no está definida"), Err("la función `f2` no está definida")),
    ("fn f(a: Int) = a\nshow(f())", Err("la función `f` espera 1 argumento(s), pero recibió 0"), Err("la función `f` espera 1 argumento(s), pero recibió 0")),
    ("fix x = 1\nfix x = 2", Err("`x` ya está declarada en este ámbito"), Err("`x` ya está declarada en este ámbito")),
    ("fn f() = 1\nfn f() = 2", Err("la función `f` ya está definida"), Err("la función `f` ya está definida")),
    ("if 1 { show(1) }", Err("la condición debe ser Bool, pero es Int"), Err("la condición debe ser Bool, pero es Int")),
    ("while 1 { }", Err("la condición debe ser Bool, pero es Int"), Err("la condición debe ser Bool, pero es Int")),
    ("mut i = 0\nwhile i < 5 { i += 1\n if i == 2 { continue }\n if i == 4 { break }\n show(i) }\nshow(i)", Ok("1\n3\n4\n"), Ok("1\n3\n4\n")),
    ("fn f(n: Int) -> Int {\n mut s = 0\n mut i = 0\n while i < n {\n  fix k = i * 2\n  if k > 6 { break }\n  mut j = 0\n  while j < 3 { fix w = j\n   if w == 1 { j += 1\n continue }\n   s += k + w\n   j += 1 }\n  i += 1 }\n return s }\nshow(f(10))", Ok("32\n"), Ok("32\n")),
    ("fn f(x: Int) { fix x = x + 1\n show(x) }\nf(1)", Ok("2\n"), Ok("2\n")),
    ("fn f(x: Int) { x = 2 }\nf(1)", Err("no se puede asignar a `x` porque es inmutable; decláralo con `mut`"), Err("no se puede asignar a `x` porque es inmutable; decláralo con `mut`")),
    ("fn main() { show(2) }\nshow(1)", Ok("1\n2\n"), Ok("1\n2\n")),
    ("fn main(a: Int) { }", Err("`main` no puede tener parámetros"), Err("`main` no puede tener parámetros")),
    ("fn f() { return }\nshow(f())", Ok("()\n"), Ok("()\n")),
    ("fn f() { }\nf()\n1 + 2\nf()", Ok(""), Ok("3\n")),
    ("fn f(n: Int) -> Int = f(n + 1)\nshow(f(0))", Err("desbordamiento de pila: más de 10000 llamadas anidadas"), Err("desbordamiento de pila: más de 10000 llamadas anidadas")),
    ("mut g = 10\nfn bump() { g += 1 }\nbump()\nbump()\nshow(g)", Ok("12\n"), Ok("12\n")),
    ("fn f() = g\nfix g = 5\nshow(f())", Ok("5\n"), Ok("5\n")),
    ("fn f() = g\nshow(f())\nfix g = 5", Err("la variable `g` no está definida"), Err("la variable `g` no está definida")),
    ("fix a = 1\nif true { fix a = 2\n show(a) }\nshow(a)", Ok("2\n1\n"), Ok("2\n1\n")),
    ("show(1.5 + 1.5)\nshow(2 < 3.5)\nshow(3 >= 3)\nshow(true == true)\nshow(true == 1)", Err("la operación `==` no admite Bool y Int"), Err("la operación `==` no admite Bool y Int")),
    ("mut i = 0\nwhile i < 3 { fix a = i\n i += 1 }\nshow(i)", Ok("3\n"), Ok("3\n")),
];

#[test]
fn programas_variados() {
    for (src, plain, echo) in PROGRAMAS {
        assert_eq!(
            vm(src, false),
            owned(*plain),
            "programa:
{src}"
        );
        assert_eq!(
            vm(src, true),
            owned(*echo),
            "programa con echo:
{src}"
        );
        if vm(src, false).is_ok() {
            assert!(
                well_typed(src),
                "debía rechazarse:
{src}"
            );
        }
    }
}

/// Cada ejemplo del repositorio es un programa bien tipado que se ejecuta sin error.
#[test]
fn los_ejemplos_se_ejecutan() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    // Los ejemplos que leen archivos usan rutas relativas a la raíz del repositorio. Ningún otro
    // test de este archivo toca el disco, así que cambiar el directorio no los afecta.
    std::env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "jb") {
            let src = std::fs::read_to_string(&path).unwrap();
            assert!(well_typed(&src), "{path:?}");
            assert!(vm(&src, false).is_ok(), "{path:?}: {:?}", vm(&src, false));
            count += 1;
        }
    }
    assert!(count >= 10, "{count} ejemplos");
}

#[test]
fn criterios_del_hito() {
    let fib = "fn fib(n: Int) -> Int {\n if n < 2 { return n }\n return fib(n - 1) + fib(n - 2)\n}\nshow(fib(20))\n";
    assert_eq!(vm(fib, false).unwrap(), "6765\n");
    let fact = "fn fact(n: Int) -> Int = if_ok(n)\nfn if_ok(n: Int) -> Int {\n if n <= 1 { return 1 }\n return n * fact(n - 1)\n}\nshow(fact(10))\n";
    assert_eq!(vm(fact, false).unwrap(), "3628800\n");
}

const REPL: &[(&str, Result<(), &str>)] = &[
    ("mut total = 1", Ok(())),
    ("fn doble(x: Int) = x * 2", Ok(())),
    ("total = doble(total) + 1", Ok(())),
    ("total", Ok(())),
    ("fix r = nada", Err("la variable `nada` no está definida")),
    ("fix r = 7", Ok(())),
    ("r + total", Ok(())),
    (
        "fn doble(x: Int) = 0",
        Err("la función `doble` ya está definida"),
    ),
    ("1 / 0", Err("división entre cero")),
    ("total", Ok(())),
];
const REPL_SALIDA: &str = "3\n10\n3\n";

#[test]
fn estado_persistente_como_en_el_repl() {
    let mut session = Session::new(Vec::new());
    for (input, expected) in REPL {
        let program = parse(input).unwrap();
        let got = session.run(&program, true).map_err(|d| d.message);
        assert_eq!(got, expected.map_err(String::from), "entrada: {input}");
    }
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        REPL_SALIDA
    );
}

#[test]
fn el_limite_de_recursion_es_configurable() {
    let program = parse("fn f(n: Int) -> Int = f(n + 1)\nf(0)").unwrap();
    let mut session = Session::new(Vec::new()).with_max_depth(50);
    let err = session.run(&program, false).unwrap_err();
    assert!(
        err.message.contains("más de 50 llamadas"),
        "{}",
        err.message
    );
}

#[test]
fn recursion_profunda_no_usa_la_pila_nativa() {
    let src = "fn suma(n: Int) -> Int {\n if n == 0 { return 0 }\n return n + suma(n - 1)\n}\nshow(suma(9000))\n";
    assert_eq!(vm(src, false).unwrap(), "40504500\n");
}
