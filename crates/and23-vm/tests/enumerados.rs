//! Enumerados, `match` e `if` como valor.

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

fn run(src: &str) -> Result<String, String> {
    let normal = run_with(src, false);
    let stress = run_with(src, true);
    assert_eq!(normal, stress, "el modo stress cambió el resultado:\n{src}");
    normal
}

fn ok(src: &str) -> String {
    run(src).unwrap_or_else(|e| panic!("falló: {e}\n{src}"))
}

fn err(src: &str) -> String {
    match run(src) {
        Ok(out) => panic!("debía fallar, pero imprimió:\n{out}\n{src}"),
        Err(e) => e,
    }
}

// ---------------------------------------------------------------- enumerados

#[test]
fn un_enumerado_tiene_variantes_con_nombre_y_orden() {
    let src = r#"
enum Color { Rojo, Verde, Azul }
show(Color.Rojo.nombre)
show(Color.Azul.orden)
show(Color.todos.tam())
show(Color.todos.obtiene(1).nombre)
fix c: Color = Color.Verde
show(c == Color.Verde)
show(c == Color.Rojo)
show(c.orden)
"#;
    assert_eq!(ok(src), "Rojo\n2\n3\nVerde\ntrue\nfalse\n1\n");
}

#[test]
fn las_variantes_se_separan_con_comas_o_lineas() {
    let src = r#"
enum A { X, Y, Z }
enum B {
    P
    Q,
    R
}
show(A.todos.tam() + B.todos.tam())
"#;
    assert_eq!(ok(src), "6\n");
}

#[test]
fn los_errores_de_un_enumerado() {
    assert!(err("enum E { }\n").contains("al menos una variante"));
    assert!(err("enum E { A, A }\n").contains("repetida"));
    assert!(err("enum E { todos }\n").contains("reservado"));
    assert!(err("enum E { A B }\n").contains("`,`"));
    assert!(err("fn f() {\n enum E { A }\n}\n").contains("nivel superior"));
    assert!(err("enum E { A }\nshow(E.B)\n").contains("B"));
}

// ---------------------------------------------------------------- match como sentencia

#[test]
fn match_elige_el_caso_de_un_enumerado() {
    let src = r#"
enum Dia { Lun, Mar, Mie }
fn tipo(d: Dia) -> Str {
    match d {
        Dia.Lun -> return "lunes"
        Dia.Mar, Dia.Mie -> return "otro"
    }
}
show(tipo(Dia.Lun))
show(tipo(Dia.Mie))
"#;
    assert_eq!(ok(src), "lunes\notro\n");
}

#[test]
fn match_con_literales_y_comodin() {
    let src = r#"
fn f(n: Int) {
    match n {
        0 -> show("cero")
        1, 2 -> show("uno o dos")
        -1 -> show("menos uno")
        _ -> show("otro")
    }
}
f(0)
f(2)
f(-1)
f(9)
fix s = "b"
match s {
    "a" -> show(1)
    "b" -> { show(2); show(3) }
}
"#;
    assert_eq!(ok(src), "cero\nuno o dos\nmenos uno\notro\n2\n3\n");
}

#[test]
fn match_por_tipo_liga_el_valor() {
    let src = r#"
kind Figura { fn area() -> Float }
thing Circulo(fix r: Float) : Figura { fn area() -> Float = 3.0 * r * r }
thing Cuadro(fix l: Float) : Figura { fn area() -> Float = l * l }
fn d(f: Figura) {
    match f {
        c: Circulo -> show("círculo ${c.r}")
        q: Cuadro -> show("cuadro ${q.l}")
    }
}
d(Circulo(2.0))
d(Cuadro(3.0))
"#;
    assert_eq!(ok(src), "círculo 2.0\ncuadro 3.0\n");
}

#[test]
fn match_dentro_de_un_bucle_admite_break_y_continue() {
    let src = r#"
mut i = 0
while i < 6 {
    match i {
        1 -> { i += 1; continue }
        4 -> break
        _ -> show("i=${i}")
    }
    i += 1
}
"#;
    assert_eq!(ok(src), "i=0\ni=2\ni=3\n");
}

#[test]
fn sin_caso_que_coincida_falla_con_un_fallo() {
    let src = r#"
fn f(n: Int) {
    match n {
        1 -> show("uno")
    }
}
f(1)
attempt { f(2) } rescue e: Fallo { show(e.msg) }
"#;
    assert_eq!(
        ok(src),
        "uno\nningún caso del `match` coincide con el valor\n"
    );
}

#[test]
fn la_variable_temporal_de_match_no_se_ve_fuera() {
    let src = "match 1 { _ -> show(1) }\nshow(_m1)\n";
    assert!(err(src).contains("_m1"));
}

#[test]
fn varios_match_en_el_mismo_bloque() {
    let src = r#"
fn f(a: Int, b: Int) {
    match a { 1 -> show("a1")
              _ -> show("a?") }
    match b { 1 -> show("b1")
              _ -> show("b?") }
}
f(1, 2)
"#;
    assert_eq!(ok(src), "a1\nb?\n");
}

// ---------------------------------------------------------------- match como valor

#[test]
fn match_como_valor() {
    let src = r#"
enum Color { Rojo, Verde }
fn hex(c: Color) -> Str = match c {
    Color.Rojo -> "rojo"
    Color.Verde -> "verde"
}
show(hex(Color.Verde))
fix n = 5
fix t = match n { 5 -> "cinco"
                  _ -> "otro" }
show(t)
show("x" + match n { 5 -> "5"
                    _ -> "?" })
fix l = Lista<Int>()
l.agrega(match n { 5 -> 50
                   _ -> 0 })
show(l)
"#;
    assert_eq!(ok(src), "verde\ncinco\nx5\n[50]\n");
}

#[test]
fn match_como_valor_por_tipo() {
    let src = r#"
kind K { fn n() -> Int }
thing A() : K { fn n() -> Int = 1 }
thing B(fix v: Int) : K { fn n() -> Int = v }
fn f(k: K) -> Int = match k {
    a: A -> a.n()
    b: B -> b.v * 10
}
show(f(A()))
show(f(B(4)))
"#;
    assert_eq!(ok(src), "1\n40\n");
}

#[test]
fn match_anidados() {
    let src = r#"
fn f(a: Int, b: Int) -> Str = match a {
    1 -> match b { 1 -> "11"
                   _ -> "1?" }
    _ -> "??"
}
show(f(1, 1))
show(f(1, 5))
show(f(2, 1))
"#;
    assert_eq!(ok(src), "11\n1?\n??\n");
}

#[test]
fn errores_de_match() {
    assert!(err("match 1 { }\n").contains("al menos un caso"));
    assert!(err("match 1 { _ -> show(1)\n 2 -> show(2) }\n").contains("debe ser el último"));
    assert!(err("match 1 { 1, _ -> show(1) }\n").contains("solo pueden ser valores"));
    assert!(err("match 1 { 1 show(1) }\n").contains("`->`"));
    assert!(err("match 1 { 1 -> show(1)\n").contains("no se cierra"));
}

// ---------------------------------------------------------------- if como valor

#[test]
fn if_como_valor() {
    let src = r#"
fn tam(n: Int) -> Str = if n < 10 { "chico" } else if n < 100 { "medio" } else { "grande" }
show(tam(1))
show(tam(50))
show(tam(500))
fix x = if true { 1 } else { 2 }
show(x + 1)
fix y = 3
show(if y > 2 {
    "mayor"
} else {
    "menor"
})
"#;
    assert_eq!(ok(src), "chico\nmedio\ngrande\n2\nmayor\n");
}

#[test]
fn un_if_como_valor_necesita_else() {
    assert!(err("fix x = if true { 1 }\n").contains("necesita `else`"));
}

#[test]
fn el_if_de_una_sentencia_sigue_siendo_sentencia() {
    let src = "if true { show(1) }\nif false { show(2) } else { show(3) }\n";
    assert_eq!(ok(src), "1\n3\n");
}

// ---------------------------------------------------------------- dentro de interpolaciones

#[test]
fn match_e_if_dentro_de_una_interpolacion() {
    let src = r#"
fix n = 2
show("n es ${match n { 1 -> "uno"
                       _ -> "otro" }}")
show("${if n == 2 { "dos" } else { "no" }}")
"#;
    assert_eq!(ok(src), "n es otro\ndos\n");
}

// ---------------------------------------------------------------- cobertura

#[test]
fn un_match_sobre_un_enumerado_debe_cubrir_todas_las_variantes() {
    let base = "enum C { A, B, D }\nfn f(c: C) -> Int = ";
    let e = err(&format!("{base}match c {{ C.A -> 1\n C.B -> 2 }}\n"));
    assert!(e.contains("no cubre `C.D`"), "{e}");
    // Con `_`, o nombrándolas todas, se acepta.
    ok(&format!("{base}match c {{ C.A -> 1\n _ -> 2 }}\n"));
    ok(&format!(
        "{base}match c {{ C.A, C.B -> 1\n C.D -> 2 }}\nshow(f(C.D))\n"
    ));
}
