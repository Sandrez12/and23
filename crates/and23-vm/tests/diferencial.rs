//! La VM debe comportarse igual que el intérprete de árbol: misma salida y mismos errores.

use and23_eval::Interpreter;
use and23_syntax::parse;
use and23_vm::Session;
use std::path::Path;

type Outcome = Result<String, String>;

fn tree(src: &str, echo: bool) -> Outcome {
    // El intérprete de árbol recurre en la pila nativa: necesita un hilo con pila grande.
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || tree_inner(&src, echo))
        .unwrap()
        .join()
        .unwrap()
}

fn tree_inner(src: &str, echo: bool) -> Outcome {
    let program = parse(src).map_err(|d| d.message)?;
    let mut interp = Interpreter::new(Vec::new());
    let result = interp
        .run(&program, echo)
        .and_then(|()| interp.run_main())
        .map_err(|d| d.message);
    result.map(|()| String::from_utf8(interp.out.clone()).unwrap())
}

fn vm(src: &str, echo: bool) -> Outcome {
    let program = parse(src).map_err(|d| d.message)?;
    let mut session = Session::new(Vec::new());
    let result = session
        .run(&program, echo)
        .and_then(|()| session.run_main())
        .map_err(|d| d.message);
    result.map(|()| String::from_utf8(session.out().clone()).unwrap())
}

/// Los programas que el verificador de tipos rechaza ya no llegan a ejecutarse.
fn well_typed(src: &str) -> bool {
    parse(src).is_ok_and(|p| and23_types::Checker::new().check(&p).is_ok())
}

fn same(src: &str) {
    if !well_typed(src) {
        assert!(
            vm(src, false).is_err(),
            "debía rechazarse:
{src}"
        );
        return;
    }
    for echo in [false, true] {
        assert_eq!(vm(src, echo), tree(src, echo), "programa:\n{src}");
    }
}

#[test]
fn los_ejemplos_coinciden() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "jb") {
            let src = std::fs::read_to_string(&path).unwrap();
            assert!(well_typed(&src), "{path:?}");
            let expected = tree(&src, false);
            // El intérprete de árbol no conoce las cadenas ni los arreglos (M3).
            if expected
                .as_ref()
                .is_err_and(|e| e.contains("no soporta cadenas"))
            {
                assert!(vm(&src, false).is_ok(), "{path:?}");
                continue;
            }
            assert!(expected.is_ok(), "{path:?}: {expected:?}");
            assert_eq!(vm(&src, false), expected, "{path:?}");
            count += 1;
        }
    }
    assert!(count >= 3);
}

#[test]
fn criterios_del_hito() {
    let fib = "fn fib(n: Int) -> Int {\n if n < 2 { return n }\n return fib(n - 1) + fib(n - 2)\n}\nshow(fib(20))\n";
    assert_eq!(vm(fib, false).unwrap(), "6765\n");
    let fact = "fn fact(n: Int) -> Int = if_ok(n)\nfn if_ok(n: Int) -> Int {\n if n <= 1 { return 1 }\n return n * fact(n - 1)\n}\nshow(fact(10))\n";
    assert_eq!(vm(fact, false).unwrap(), "3628800\n");
}

#[test]
fn programas_variados() {
    let programs = [
        "show(1 + 2 * 3)\nshow(10 / 4)\nshow(10 / 4.0)\nshow(-7 % 3)\nshow(1 == 1.0)",
        "show(1 / 0)",
        "show(1.0 / 0.0)",
        "show(9223372036854775807 + 1)",
        "show(-(0 - 9223372036854775807 - 1))",
        "show(1 + true)",
        "show(!1)",
        "show(-true)",
        "show(true && 1)",
        "show(false && 1)",
        "show(true || 1)",
        "show(1 || true)",
        "show(true && false || true)",
        "mut a = 0\nfn t() -> Bool { a += 1\n return true }\nshow(false && t())\nshow(true || t())\nshow(a)",
        "fix x = 1\nx = 2",
        "mut x = 1\nx += 2\nx *= 5\nshow(x)",
        "show(y)",
        "fn f() = f2()\nshow(f())",
        "fn f(a: Int) = a\nshow(f())",
        "fix x = 1\nfix x = 2",
        "fn f() = 1\nfn f() = 2",
        "if 1 { show(1) }",
        "while 1 { }",
        "mut i = 0\nwhile i < 5 { i += 1\n if i == 2 { continue }\n if i == 4 { break }\n show(i) }\nshow(i)",
        "fn f(n: Int) -> Int {\n mut s = 0\n mut i = 0\n while i < n {\n  fix k = i * 2\n  if k > 6 { break }\n  mut j = 0\n  while j < 3 { fix w = j\n   if w == 1 { j += 1\n continue }\n   s += k + w\n   j += 1 }\n  i += 1 }\n return s }\nshow(f(10))",
        "fn f(x: Int) { fix x = x + 1\n show(x) }\nf(1)",
        "fn f(x: Int) { x = 2 }\nf(1)",
        "fn main() { show(2) }\nshow(1)",
        "fn main(a: Int) { }",
        "fn f() { return }\nshow(f())",
        "fn f() { }\nf()\n1 + 2\nf()",
        "fn f(n: Int) -> Int = f(n + 1)\nshow(f(0))",
        "mut g = 10\nfn bump() { g += 1 }\nbump()\nbump()\nshow(g)",
        "fn f() = g\nfix g = 5\nshow(f())",
        "fn f() = g\nshow(f())\nfix g = 5",
        "fix a = 1\nif true { fix a = 2\n show(a) }\nshow(a)",
        "show(1.5 + 1.5)\nshow(2 < 3.5)\nshow(3 >= 3)\nshow(true == true)\nshow(true == 1)",
        "mut i = 0\nwhile i < 3 { fix a = i\n i += 1 }\nshow(i)",
    ];
    for src in programs {
        same(src);
    }
}

#[test]
fn estado_persistente_como_en_el_repl() {
    let inputs = [
        "mut total = 1",
        "fn doble(x: Int) = x * 2",
        "total = doble(total) + 1",
        "total",
        "fix r = nada",
        "fix r = 7",
        "r + total",
        "fn doble(x: Int) = 0",
        "1 / 0",
        "total",
    ];
    let mut interp = Interpreter::new(Vec::new());
    let mut session = Session::new(Vec::new());
    for input in inputs {
        let program = parse(input).unwrap();
        let a = interp.run(&program, true).map_err(|d| d.message);
        let b = session.run(&program, true).map_err(|d| d.message);
        assert_eq!(b, a, "entrada: {input}");
    }
    assert_eq!(session.out().clone(), interp.out.clone());
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
