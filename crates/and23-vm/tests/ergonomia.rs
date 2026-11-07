//! Mini-hito de ergonomía tras M10: `show` legible de `Lista`/`Mapa` y estrechamiento de las
//! variables `mut` globales.

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

// ---------------------------------------------------------------- show de colecciones

#[test]
fn una_lista_se_muestra_como_un_arreglo() {
    let src = r#"
fix xs = Lista<Str>()
xs.agrega("a")
xs.agrega("b")
show(xs)
show(Lista<Int>())
fix anidada = Lista<Lista<Int>>()
anidada.agrega(Lista<Int>())
show(anidada)
show("lista: ${xs}")
"#;
    assert_eq!(ok(src), "[\"a\", \"b\"]\n[]\n[[]]\nlista: [\"a\", \"b\"]\n");
}

#[test]
fn un_mapa_se_muestra_con_llaves() {
    let src = r#"
fix m = Mapa<Str, Int>()
show(m)
m.pone("uno", 1)
show(m)
fix l = Mapa<Int, Lista<Int>>()
l.pone(1, Lista<Int>())
show(l)
show("mapa: ${m}")
"#;
    assert_eq!(ok(src), "{}\n{\"uno\": 1}\n{1: []}\nmapa: {\"uno\": 1}\n");
}

#[test]
fn las_colecciones_se_muestran_con_varios_elementos_y_dentro_de_otros_objetos() {
    let src = r#"
fix m = Mapa<Str, Int>()
m.pone("a", 1)
m.pone("b", 2)
m.pone("c", 3)
fix texto = "${m}"
show(texto.largo())
show(texto.contiene("\"a\": 1"))
show(texto.contiene("\"b\": 2"))
show(texto.empiezaCon("{") && texto.terminaCon("}"))
thing Caja(fix xs: Lista<Int>, fix m: Mapa<Str, Int>)
fix xs = Lista<Int>()
xs.agrega(7)
show(Caja(xs, m).xs)
fix ps = Lista<Caja>()
show(ps)
"#;
    assert_eq!(
        ok(src),
        "24
true\ntrue\ntrue\n[7]\n[]\n"
    );
}

#[test]
fn una_lista_con_un_ciclo_no_se_cuelga() {
    let src = r#"
thing N(mut hijos: Lista<N>)
fix a = N(Lista<N>())
a.hijos.agrega(a)
show(a)
"#;
    assert_eq!(ok(src), "N(hijos=[...])\n");
}

// ---------------------------------------------------------------- estrechamiento de mut globales

#[test]
fn una_global_mut_se_estrecha_en_un_while() {
    let src = r#"
thing Nodo(fix v: Int, mut sig: Nodo?)
mut actual: Nodo? = Nodo(1, Nodo(2, Nodo(3, nil)))
mut total = 0
while actual != nil {
    total += actual.v
    actual = actual.sig
}
show(total)
"#;
    assert_eq!(ok(src), "6\n");
}

#[test]
fn una_global_mut_se_estrecha_en_un_if_y_se_ensancha_al_asignar() {
    let src = r#"
mut x: Int? = 4
if x != nil { show(x + 1) }
if x == nil { show("nil") } else { show(x * 2) }
if x != nil {
    x = nil
    show(x)
}
"#;
    assert_eq!(ok(src), "5\n8\nnil\n");
}

#[test]
fn una_global_mut_asignada_en_una_funcion_no_se_estrecha() {
    let src = r#"
mut x: Int? = 3
fn limpia() { x = nil }
if x != nil {
    limpia()
    show(x + 1)
}
"#;
    assert!(err(src).contains("no admite"));
}

#[test]
fn una_global_mut_asignada_en_una_lambda_o_un_metodo_no_se_estrecha() {
    let lambda = r#"
mut x: Int? = 3
fix f = { x = nil }
if x != nil {
    f()
    show(x + 1)
}
"#;
    assert!(err(lambda).contains("no admite"));
    let metodo = r#"
mut x: Int? = 3
thing C() { fn corta() { x = nil } }
if x != nil {
    C().corta()
    show(x + 1)
}
"#;
    assert!(err(metodo).contains("no admite"));
}

#[test]
fn el_estrechamiento_de_una_global_mut_no_sobrevive_fuera_del_bloque() {
    let src = r#"
mut x: Int? = 3
if x == nil { fail Fallo("x") }
x = nil
show(x)
"#;
    assert_eq!(ok(src), "nil\n");
    let fuera = r#"
mut x: Int? = 3
if x != nil { show(1) }
show(x + 1)
"#;
    assert!(err(fuera).contains("no admite"));
}

#[test]
fn las_variables_locales_mut_se_estrechan_en_un_while_como_antes() {
    let src = r#"
fn cuenta(xs: [Int?]) -> Int {
    mut i = 0
    mut actual = xs[0]
    mut n = 0
    while actual != nil {
        n += actual
        i += 1
        actual = if_ok(xs, i)
    }
    return n
}
fn if_ok(xs: [Int?], i: Int) -> Int? {
    if i < len(xs) { return xs[i] }
    return nil
}
show(cuenta([1, 2, 3, nil, 9]))
"#;
    assert_eq!(ok(src), "6\n");
}

#[test]
fn una_local_mut_asignada_en_un_while_anidado_sigue_sin_estrecharse() {
    let src = r#"
fn f(y: Int?) -> Int {
    mut x = y
    if x != nil {
        mut k = 0
        while k < 2 {
            k += x
            x = nil
        }
    }
    return 0
}
"#;
    assert!(err(src).contains("no admite"));
}

// ---------------------------------------------------------------- identidad de los objetos

/// El hash de un objeto no puede depender de la casilla del heap: el recolector las reutiliza, y el
/// orden de un `Mapa` con claves-objeto cambiaría según cuándo corra. `run` compara el modo normal
/// con `--gc-stress`.
#[test]
fn el_orden_de_un_mapa_con_claves_objeto_no_depende_del_recolector() {
    let src = r#"
thing K(fix n: Int)
fix m = Mapa<K, Int>()
mut i = 0
while i < 8 {
    mut j = 0
    while j < 40 {
        fix basura = ["x${j}", "y${j}"]
        j += 1
    }
    m.pone(K(i), i)
    i += 1
}
fix orden = Lista<Int>()
m.claves().paraCada { k -> orden.agrega(k.n) }
show(orden)
show(orden.tam())
"#;
    let out = ok(src);
    assert!(out.ends_with("8\n"), "{out}");
}

#[test]
fn el_hash_de_un_objeto_es_el_mismo_con_y_sin_recolector() {
    let src = r#"
thing P(fix n: Int)
mut i = 0
while i < 60 {
    fix basura = ["a${i}", "b${i}"]
    i += 1
}
fix a = P(1)
fix b = P(2)
show(hash(a) == hash(a))
show(hash(a) == hash(b))
show(hash(a))
show(hash(b))
"#;
    // `ok` ya exige la misma salida en modo normal y en modo stress.
    let out = ok(src);
    assert!(out.starts_with("true\nfalse\n"), "{out}");
}
