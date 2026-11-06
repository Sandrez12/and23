//! Cadenas, arreglos y recolector de basura (M3).

use and23_syntax::parse;
use and23_vm::Session;

fn run_with(src: &str, stress: bool) -> Result<String, String> {
    let program = parse(src).map_err(|d| d.message)?;
    let mut session = Session::new(Vec::new()).with_gc_stress(stress);
    session
        .run(&program, false)
        .and_then(|()| session.run_main())
        .map_err(|d| d.message)?;
    Ok(String::from_utf8(session.out().clone()).unwrap())
}

/// Ejecuta en modo normal y en modo stress (GC en cada punto seguro) y exige el mismo resultado.
fn run(src: &str) -> Result<String, String> {
    let normal = run_with(src, false);
    let stress = run_with(src, true);
    assert_eq!(normal, stress, "el modo stress cambió el resultado:\n{src}");
    normal
}

fn ok(src: &str) -> String {
    run(src).unwrap_or_else(|e| panic!("falló: {e}\n{src}"))
}

#[test]
fn fizzbuzz() {
    let src = r#"
fn fizz(n: Int) -> Str {
    if n % 15 == 0 { return "FizzBuzz" }
    if n % 3 == 0 { return "Fizz" }
    if n % 5 == 0 { return "Buzz" }
    return "${n}"
}
mut i = 1
while i <= 15 { show(fizz(i))
    i += 1 }
"#;
    let expected = "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n";
    assert_eq!(ok(src), expected);
}

#[test]
fn cadenas() {
    assert_eq!(ok(r#"show("hola " + "mundo")"#), "hola mundo\n");
    assert_eq!(
        ok(r#"fix n = 3
fix f = 1.5
fix b = true
show("n=${n} f=${f} b=${b} suma=${n + 1}")"#),
        "n=3 f=1.5 b=true suma=4\n"
    );
    assert_eq!(ok(r#"show(len("ñandú"))"#), "5\n");
    assert_eq!(
        ok(r#"show("a\tb\n\"c\" \u{1F600} \$")"#),
        "a\tb\n\"c\" 😀 $\n"
    );
    assert_eq!(
        ok(r#"show("abc" < "abd")
show("b" > "abc")
show("x" == "x")
show("x" != "x")"#),
        "true\ntrue\ntrue\nfalse\n"
    );
    assert_eq!(
        ok("mut s = \"a\"\ns += \"b\"\ns += \"${1}\"\nshow(s)"),
        "ab1\n"
    );
    // dueño y ñandú: identificadores Unicode con cadenas
    assert_eq!(
        ok("fix dueño = \"Ana\"\nshow(\"hola ${dueño}\")"),
        "hola Ana\n"
    );
}

#[test]
fn arreglos() {
    assert_eq!(
        ok("fix a = [10, 20, 30]\nshow(a[1])\nshow(len(a))\nshow(a)"),
        "20\n3\n[10, 20, 30]\n"
    );
    assert_eq!(
        ok("mut a = [1, 2]\na[0] = 5\na[1] += 10\nshow(a)"),
        "[5, 12]\n"
    );
    assert_eq!(
        ok("fix a: [Int] = []\npush(a, 1)\npush(a, 2)\nshow(a)\nshow(len(a))"),
        "[1, 2]\n2\n"
    );
    assert_eq!(
        ok(r#"fix a = ["x", "y\n"]
show(a)
show("${a}")"#),
        "[\"x\", \"y\\n\"]\n[\"x\", \"y\\n\"]\n"
    );
    assert_eq!(
        ok("fix m = [[1, 2], [3]]\nshow(m[0][1])\nshow(m)\nm[1][0] = 9\nshow(m)"),
        "2\n[[1, 2], [3]]\n[[1, 2], [9]]\n"
    );
    assert_eq!(ok("fix e: [[Str]] = []\nshow(e)\nshow(len(e))"), "[]\n0\n");
    assert_eq!(
        ok("show([1.5, 2.0])\nshow([true, false])"),
        "[1.5, 2.0]\n[true, false]\n"
    );
}

#[test]
fn los_arreglos_son_referencias() {
    let src = "fix a = [1]\nfix b = a\nb[0] = 9\nshow(a[0])\nfn toca(v: [Int]) { v[0] = 7 }\ntoca(a)\nshow(b[0])";
    assert_eq!(ok(src), "9\n7\n");
}

#[test]
fn indice_fuera_de_rango() {
    let err = run("fix a = [1, 2]\nshow(a[2])").unwrap_err();
    assert_eq!(
        err,
        "índice fuera de rango: 2 en un arreglo de 2 elemento(s)"
    );
    let err = run("fix a = [1, 2]\nshow(a[0 - 1])").unwrap_err();
    assert!(err.contains("índice fuera de rango: -1"), "{err}");
    let err = run("fix a: [Int] = []\na[0] = 1").unwrap_err();
    assert!(
        err.contains("índice fuera de rango: 0 en un arreglo de 0"),
        "{err}"
    );
}

#[test]
fn el_error_de_indice_apunta_al_acceso() {
    let src = "fix a = [1]\nshow(a[5])";
    let program = parse(src).unwrap();
    let mut session = Session::new(Vec::new());
    let d = session.run(&program, false).unwrap_err();
    assert_eq!(&src[d.span.start..d.span.end], "a[5]");
}

#[test]
fn un_arreglo_vivo_sobrevive_a_muchas_recolecciones() {
    let src = r#"
mut guardados: [Str] = []
mut i = 0
while i < 3000 {
    push(guardados, "item ${i}")
    fix basura = "tmp ${i}" + "x"
    i += 1
}
mut total = 0
mut j = 0
while j < len(guardados) { total += len(guardados[j])
    j += 1 }
show(total)
show(guardados[2999])
"#;
    assert_eq!(ok(src), "25890\nitem 2999\n");
}

#[test]
fn estructuras_anidadas_y_temporales_en_la_pila() {
    // Las cadenas intermedias de una expresión larga siguen vivas mientras se evalúa el resto.
    let src = r#"
fn f(n: Int) -> Str = "<${n}>"
show(f(1) + f(2) + f(3) + "${f(4)}${f(5)}" + f(6))
fix m = [[f(1), f(2)], [f(3) + f(4)]]
show(m)
"#;
    assert_eq!(
        ok(src),
        "<1><2><3><4><5><6>\n[[\"<1>\", \"<2>\"], [\"<3><4>\"]]\n"
    );
}

#[test]
fn diez_millones_de_asignaciones_sin_agotar_memoria() {
    let src = r#"
mut i = 0
mut total = 0
while i < 10000000 {
    fix s = "n${i}"
    total += len(s)
    i += 1
}
show(total)
"#;
    let program = parse(src).unwrap();
    let mut session = Session::new(Vec::new());
    session.run(&program, false).unwrap();
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        "78888890\n"
    );
    let stats = session.heap_stats();
    assert!(stats.allocated >= 10_000_000, "{stats:?}");
    assert!(stats.collections > 10, "{stats:?}");
    // El heap nunca tuvo más que unas decenas de miles de objetos a la vez.
    assert!(stats.slots < 200_000, "{stats:?}");
    assert!(stats.live_objects < 200_000, "{stats:?}");
}

#[test]
fn modo_stress_libera_y_reutiliza_ranuras() {
    let src = "mut i = 0\nwhile i < 2000 { fix s = \"x${i}\"\n i += 1 }\nshow(i)";
    let program = parse(src).unwrap();
    let mut session = Session::new(Vec::new()).with_gc_stress(true);
    session.run(&program, false).unwrap();
    let stats = session.heap_stats();
    assert!(stats.slots <= 8, "{stats:?}");
    assert!(stats.freed >= 1999, "{stats:?}");
}

#[test]
fn el_estado_del_gc_persiste_entre_entradas_del_repl() {
    let mut session = Session::new(Vec::new()).with_gc_stress(true);
    for input in [
        "mut nombres: [Str] = []",
        "push(nombres, \"ana\")",
        "fix tmp = \"basura\" + \"1\"",
        "push(nombres, \"luis\" + \"!\")",
        "nombres",
    ] {
        let program = parse(input).unwrap();
        session.run(&program, true).unwrap();
    }
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        "[\"ana\", \"luis!\"]\n"
    );
}
