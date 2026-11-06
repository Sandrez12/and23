//! Excepciones (`attempt`/`rescue`/`fail`) y opcionales (`T?`, `nil`, `?.`, `?:`) (M6).

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

/// Ejecuta en modo normal y en modo stress y exige el mismo resultado.
fn run(src: &str) -> Result<String, String> {
    let normal = run_with(src, false);
    let stress = run_with(src, true);
    assert_eq!(normal, stress, "el modo stress cambió el resultado:\n{src}");
    normal
}

fn ok(src: &str) -> String {
    run(src).unwrap_or_else(|e| panic!("falló: {e}\n{src}"))
}

const ERRORES: &str = r#"
thing Malo(fix codigo: Int) : Fallo("malo ${codigo}")
thing Peor(codigo: Int) : Malo(codigo)
thing Otro(msg: Str) : Fallo(msg)
"#;

// ---------------------------------------------------------------- excepciones

#[test]
fn captura_basica_y_el_cuerpo_sin_error_no_ejecuta_el_rescue() {
    let src = format!(
        "{ERRORES}\nattempt {{ show(1) }}\nrescue e: Malo {{ show(\"no\") }}\nattempt {{ fail Malo(7)\n show(\"no llega\") }}\nrescue e: Malo {{ show(e.msg)\n show(e.codigo) }}\nshow(\"fin\")"
    );
    assert_eq!(ok(&src), "1\nmalo 7\n7\nfin\n");
}

#[test]
fn fallo_es_la_clase_base_y_trae_msg() {
    let src = "attempt { fail Fallo(\"hola\") }\nrescue e: Fallo { show(e.msg)\n show(e) }";
    assert_eq!(ok(src), "hola\nFallo(msg=\"hola\")\n");
}

#[test]
fn rescue_de_una_clase_base_captura_a_las_subclases() {
    let src = format!(
        "{ERRORES}\nattempt {{ fail Peor(3) }}\nrescue e: Malo {{ show(\"malo: ${{e.codigo}}\")\n show(e is Peor) }}"
    );
    assert_eq!(ok(&src), "malo: 3\ntrue\n");
}

#[test]
fn gana_la_primera_clausula_que_coincide() {
    let src = format!(
        "{ERRORES}\nfn prueba(f: Int) {{\n attempt {{\n if f == 1 {{ fail Peor(1) }}\n if f == 2 {{ fail Malo(2) }}\n fail Otro(\"x\")\n }}\n rescue e: Peor {{ show(\"peor\") }}\n rescue e: Malo {{ show(\"malo\") }}\n rescue e: Fallo {{ show(\"fallo ${{e.msg}}\") }}\n}}\nprueba(1)\nprueba(2)\nprueba(3)"
    );
    assert_eq!(ok(&src), "peor\nmalo\nfallo x\n");
}

#[test]
fn si_ninguna_clausula_coincide_la_excepcion_sube() {
    let src = format!(
        "{ERRORES}\nattempt {{\n attempt {{ fail Otro(\"se escapa\") }}\n rescue e: Malo {{ show(\"no\") }}\n}}\nrescue e: Otro {{ show(\"afuera: ${{e.msg}}\") }}"
    );
    assert_eq!(ok(&src), "afuera: se escapa\n");
}

#[test]
fn se_puede_relanzar_desde_un_rescue() {
    let src = format!(
        "{ERRORES}\nattempt {{\n attempt {{ fail Malo(1) }}\n rescue e: Malo {{ show(\"interno\")\n fail Otro(\"relanzado ${{e.codigo}}\") }}\n}}\nrescue e: Otro {{ show(e.msg) }}\nattempt {{ attempt {{ fail Malo(2) }}\n rescue e: Malo {{ fail e }} }}\nrescue e: Malo {{ show(\"mismo objeto: ${{e.codigo}}\") }}"
    );
    assert_eq!(ok(&src), "interno\nrelanzado 1\nmismo objeto: 2\n");
}

#[test]
fn la_excepcion_atraviesa_llamadas_y_restaura_la_pila() {
    let src = format!(
        "{ERRORES}\nfn hondo(n: Int) -> Int {{\n fix local = n * 2\n if n == 0 {{ fail Malo(99) }}\n return hondo(n - 1) + local\n}}\nfn caller() -> Int {{\n fix a = 5\n attempt {{ return hondo(50) }}\n rescue e: Malo {{ return a + e.codigo }}\n}}\nshow(caller())\nshow(caller() + 1)"
    );
    assert_eq!(ok(&src), "104\n105\n");
}

#[test]
fn la_excepcion_atraviesa_metodos() {
    let src = format!(
        "{ERRORES}\nthing A() {{ fn f() {{ fail Malo(4) }}\n fn g() {{ f() }} }}\nthing B() : A() {{ fn f() {{ show(\"B.f\")\n super.f() }} }}\nattempt {{ B().g() }}\nrescue e: Malo {{ show(\"capturado ${{e.codigo}}\") }}"
    );
    assert_eq!(ok(&src), "B.f\ncapturado 4\n");
}

#[test]
fn las_variables_locales_del_attempt_no_se_filtran() {
    let src = format!(
        "{ERRORES}\nfn f() -> Int {{\n fix antes = 1\n attempt {{\n fix dentro = 10\n fix otra = 20\n fail Malo(dentro + otra) }}\n rescue e: Malo {{ fix despues = e.codigo\n return antes + despues }}\n}}\nshow(f())"
    );
    assert_eq!(ok(&src), "31\n");
}

#[test]
fn un_return_dentro_del_attempt_no_deja_un_manejador_colgado() {
    // Si el manejador de `f` siguiera instalado, el `fail` final sería capturado por él.
    let src = format!(
        "{ERRORES}\nfn f() -> Int {{\n attempt {{ return 1 }}\n rescue e: Malo {{ return 2 }}\n}}\nshow(f())\nfail Malo(5)"
    );
    let err = run(&src).unwrap_err();
    assert_eq!(err, "excepción sin capturar `Malo`: malo 5");
}

#[test]
fn break_y_continue_dentro_del_attempt_cierran_el_manejador() {
    let src = format!(
        "{ERRORES}\nmut i = 0\nmut vistos = 0\nwhile i < 6 {{\n i += 1\n attempt {{\n if i == 2 {{ continue }}\n if i == 5 {{ break }}\n vistos += 1\n if i == 3 {{ fail Malo(i) }}\n }}\n rescue e: Malo {{ show(\"capturado ${{e.codigo}}\") }}\n}}\nshow(\"i=${{i}} vistos=${{vistos}}\")\nfail Malo(0)"
    );
    let err = run(&src).unwrap_err();
    assert_eq!(err, "excepción sin capturar `Malo`: malo 0");
    // La salida previa también es la esperada.
    let program = parse(&src).unwrap();
    let mut session = Session::new(Vec::new());
    let _ = session.run(&program, false);
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        "capturado 3\ni=5 vistos=3\n"
    );
}

#[test]
fn un_error_por_vuelta_dentro_de_un_bucle() {
    let src = format!(
        "{ERRORES}\nmut total = 0\nmut i = 0\nwhile i < 1000 {{\n attempt {{\n if i % 3 == 0 {{ fail Malo(i) }}\n total += 1\n }}\n rescue e: Malo {{ total += 100 }}\n i += 1\n}}\nshow(total)"
    );
    // 334 múltiplos de 3 (+100) y 666 vueltas normales (+1)
    assert_eq!(ok(&src), "34066\n");
}

#[test]
fn los_objetos_lanzados_sobreviven_al_gc_mientras_suben() {
    let src = format!(
        "{ERRORES}\nthing Carga(fix datos: [Str])\nfn produce(n: Int) {{\n mut xs: [Str] = []\n mut i = 0\n while i < n {{ push(xs, \"dato ${{i}}\")\n i += 1 }}\n fail Peor(len(xs))\n}}\nfn nivel(n: Int) {{ if n == 0 {{ produce(50) }} else {{ fix basura = [\"a${{n}}\", \"b${{n}}\"]\n nivel(n - 1) }} }}\nattempt {{ nivel(30) }}\nrescue e: Malo {{ show(e.msg)\n show(e) }}"
    );
    assert_eq!(ok(&src), "malo 50\nPeor(msg=\"malo 50\", codigo=50)\n");
}

#[test]
fn excepcion_sin_capturar_da_error_con_el_nombre_y_el_mensaje() {
    let src = format!("{ERRORES}\nfn f() {{ fail Otro(\"boom\") }}\nf()");
    let err = run(&src).unwrap_err();
    assert_eq!(err, "excepción sin capturar `Otro`: boom");
}

#[test]
fn el_error_sin_capturar_apunta_al_fail() {
    let src = "fix a = 1\nfail Fallo(\"x\")\n";
    let program = parse(src).unwrap();
    let mut session = Session::new(Vec::new());
    let d = session.run(&program, false).unwrap_err();
    assert_eq!(&src[d.span.start..d.span.end], "fail Fallo(\"x\")");
}

#[test]
fn una_funcion_con_retorno_puede_terminar_en_fail_o_en_attempt() {
    let src = format!(
        "{ERRORES}\nfn a(x: Int) -> Int {{\n if x > 0 {{ return x }}\n fail Malo(x)\n}}\nfn b(x: Int) -> Int {{\n attempt {{ return a(x) }}\n rescue e: Malo {{ return 0 - 1 }}\n}}\nshow(a(2))\nshow(b(3))\nshow(b(0))"
    );
    assert_eq!(ok(&src), "2\n3\n-1\n");
}

#[test]
fn el_repl_sigue_tras_una_excepcion_sin_capturar() {
    let mut session = Session::new(Vec::new());
    for (input, fails) in [
        ("thing E() : Fallo(\"x\")", false),
        ("fail E()", true),
        ("attempt { fail E() }\nrescue e: E { show(e.msg) }", false),
        ("fail E()", true),
        ("1 + 1", false),
    ] {
        let program = parse(input).unwrap();
        assert_eq!(session.run(&program, true).is_err(), fails, "{input}");
    }
    assert_eq!(String::from_utf8(session.out().clone()).unwrap(), "x\n2\n");
}

// ---------------------------------------------------------------- opcionales

#[test]
fn nil_se_imprime_y_se_compara() {
    let src = "fix a: Int? = nil\nfix b: Int? = 5\nshow(a)\nshow(b)\nshow(a == nil)\nshow(b != nil)\nshow(nil == a)\nshow(\"${a} y ${b}\")";
    assert_eq!(ok(src), "nil\n5\ntrue\ntrue\ntrue\nnil y 5\n");
}

#[test]
fn acceso_seguro_a_campos_y_metodos() {
    let src = r#"
thing P(fix nombre: Str, mut edad: Int) {
    fn cumple() { edad += 1 }
    fn doble() -> Int = edad * 2
}
fix p: P? = P("Rex", 3)
fix n: P? = nil
show(p?.nombre)
show(n?.nombre)
show(p?.doble())
show(n?.doble())
p?.cumple()
n?.cumple()
show(p?.edad)
show(n?.edad)
"#;
    assert_eq!(ok(src), "Rex\nnil\n6\nnil\n4\nnil\n");
}

#[test]
fn los_argumentos_de_una_llamada_segura_no_se_evaluan_si_es_nil() {
    let src = r#"
fn traza(s: Str) -> Int {
    show("evalúa ${s}")
    return 1
}
thing P() { fn f(n: Int) -> Int = n }
fix p: P? = P()
fix n: P? = nil
show(p?.f(traza("p")))
show(n?.f(traza("n")))
"#;
    assert_eq!(ok(src), "evalúa p\n1\nnil\n");
}

#[test]
fn cadenas_de_acceso_seguro() {
    let src = r#"
thing C(fix v: Int)
thing B(fix c: C?)
thing A(fix b: B?)
fix a1: A? = A(B(C(7)))
fix a2: A? = A(B(nil))
fix a3: A? = A(nil)
fix a4: A? = nil
show(a1?.b?.c?.v)
show(a2?.b?.c?.v)
show(a3?.b?.c?.v)
show(a4?.b?.c?.v)
"#;
    assert_eq!(ok(src), "7\nnil\nnil\nnil\n");
}

#[test]
fn el_operador_elvis() {
    let src = r#"
fix a: Int? = nil
fix b: Int? = 4
show(a ?: 0)
show(b ?: 0)
show((a ?: 1) + (b ?: 1))
fix c: Int? = a ?: b
show(c)
show(a ?: b ?: 9)
fix s: Str? = nil
show(s ?: "por defecto")
"#;
    assert_eq!(ok(src), "0\n4\n5\n4\n4\npor defecto\n");
}

#[test]
fn el_lado_derecho_del_elvis_solo_se_evalua_si_hace_falta() {
    let src = r#"
fn traza() -> Int {
    show("evaluado")
    return 0
}
fix a: Int? = 3
fix b: Int? = nil
show(a ?: traza())
show(b ?: traza())
"#;
    assert_eq!(ok(src), "3\nevaluado\n0\n");
}

#[test]
fn estrechamiento_con_if_else_y_salida_temprana() {
    let src = r#"
thing P(fix n: Int)
fn f(p: P?) -> Int {
    if p != nil { return p.n }
    return 0 - 1
}
fn g(p: P?) -> Int {
    if p == nil { return 0 - 1 }
    return p.n * 10
}
fn h(p: P?) -> Str {
    if p == nil { return "nada" } else { return "n=${p.n}" }
}
fn k(p: P?) -> Str {
    if p != nil { return "n=${p.n}" } else { return "nada" }
}
show(f(P(3)))
show(f(nil))
show(g(P(3)))
show(g(nil))
show(h(P(4)))
show(h(nil))
show(k(P(5)))
show(k(nil))
"#;
    assert_eq!(ok(src), "3\n-1\n30\n-1\nn=4\nnada\nn=5\nnada\n");
}

#[test]
fn estrechamiento_con_y_o_y_negacion() {
    let src = r#"
thing P(fix n: Int)
fn a(p: P?) -> Bool = p != nil && p.n > 1
fn b(p: P?) -> Bool = p == nil || p.n > 1
fn c(p: P?) -> Bool {
    if !(p == nil) { return p.n == 1 }
    return false
}
show(a(P(2)))
show(a(P(1)))
show(a(nil))
show(b(P(2)))
show(b(P(1)))
show(b(nil))
show(c(P(1)))
show(c(nil))
"#;
    assert_eq!(
        ok(src),
        "true\nfalse\nfalse\ntrue\nfalse\ntrue\ntrue\nfalse\n"
    );
}

#[test]
fn recorrer_una_lista_enlazada_con_un_mut_estrechado() {
    let src = r#"
thing Nodo(fix v: Int, mut sig: Nodo?)
fn suma(cabeza: Nodo?) -> Int {
    mut cur = cabeza
    mut total = 0
    while cur != nil {
        total += cur.v
        cur = cur.sig
    }
    return total
}
fn largo(cabeza: Nodo?) -> Int {
    mut cur = cabeza
    mut n = 0
    while cur != nil {
        n += 1
        cur = cur.sig
    }
    return n
}
fn agrega(cabeza: Nodo, v: Int) {
    mut cur = cabeza
    while cur.sig != nil {
        cur = cur.sig ?: cur
    }
    cur.sig = Nodo(v, nil)
}
fix l = Nodo(1, nil)
agrega(l, 2)
agrega(l, 3)
agrega(l, 4)
show(suma(l))
show(largo(l))
show(suma(nil))
show(l)
"#;
    assert_eq!(
        ok(src),
        "10\n4\n0\nNodo(v=1, sig=Nodo(v=2, sig=Nodo(v=3, sig=Nodo(v=4, sig=nil))))\n"
    );
}

#[test]
fn un_mut_estrechado_vuelve_a_ser_opcional_tras_asignarle_nil() {
    let src = r#"
thing P(fix n: Int)
fn f(p: P?) -> Int {
    mut q = p
    if q != nil {
        fix antes = q.n
        q = nil
        return antes + (q?.n ?: 100)
    }
    return 0
}
show(f(P(5)))
show(f(nil))
"#;
    assert_eq!(ok(src), "105\n0\n");
}

#[test]
fn opcionales_en_arreglos_campos_parametros_y_retornos() {
    let src = r#"
thing Caja(fix v: Int?, mut otra: Caja?)
fn primero(xs: [Int?]) -> Int = xs[0] ?: 0 - 1
fn busca(xs: [Str], s: Str) -> Int? {
    mut i = 0
    while i < len(xs) {
        if xs[i] == s { return i }
        i += 1
    }
    return nil
}
fix xs: [Int?] = [1, nil, 3]
show(xs)
show(primero(xs))
show(primero([nil, 2]))
fix nombres = ["a", "b"]
show(busca(nombres, "b"))
show(busca(nombres, "z"))
show(busca(nombres, "z") ?: 0 - 1)
fix c = Caja(nil, nil)
c.otra = Caja(5, nil)
show(c)
show(c.otra?.v)
"#;
    assert_eq!(
        ok(src),
        "[1, nil, 3]\n1\n-1\n1\nnil\n-1\nCaja(v=nil, otra=Caja(v=5, otra=nil))\n5\n"
    );
}

#[test]
fn la_llamada_segura_sobre_un_metodo_virtual() {
    let src = r#"
kind Habla { fn dice() -> Str }
thing Perro() : Habla { fn dice() -> Str = "guau" }
thing Gato() : Habla { fn dice() -> Str = "miau" }
fix a: Habla? = Perro()
fix b: Habla? = Gato()
fix c: Habla? = nil
show(a?.dice())
show(b?.dice())
show(c?.dice())
show((c?.dice()) ?: "silencio")
"#;
    assert_eq!(ok(src), "guau\nmiau\nnil\nsilencio\n");
}

#[test]
fn opcionales_dentro_de_attempt_y_rescue() {
    let src = r#"
thing E() : Fallo("e")
fn busca(n: Int) -> Int? {
    if n < 0 { fail E() }
    if n == 0 { return nil }
    return n
}
fn seguro(n: Int) -> Int {
    attempt { return busca(n) ?: 0 }
    rescue e: E { return 0 - 1 }
}
show(seguro(5))
show(seguro(0))
show(seguro(0 - 3))
"#;
    assert_eq!(ok(src), "5\n0\n-1\n");
}

#[test]
fn los_opcionales_sobreviven_al_gc() {
    let src = r#"
thing N(fix v: Int, mut sig: N?)
fn construye(n: Int) -> N? {
    mut cabeza: N? = nil
    mut i = 0
    while i < n {
        cabeza = N(i, cabeza)
        i += 1
    }
    return cabeza
}
fn suma(l: N?) -> Int {
    mut cur = l
    mut t = 0
    while cur != nil {
        t += cur.v
        cur = cur.sig
    }
    return t
}
show(suma(construye(500)))
"#;
    assert_eq!(ok(src), "124750\n");
}

#[test]
fn el_ejemplo_de_excepciones_corre() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/excepciones.jb"
    ))
    .unwrap();
    let out = ok(&src);
    assert!(out.contains("faltan 430\n"), "{out}");
    assert!(out.contains("vueltas con error: 2\n"), "{out}");
    assert!(
        out.contains("afuera: la cuenta de Ana está cerrada\n"),
        "{out}"
    );
    assert!(out.ends_with("6\n0\n3\nnil\n"), "{out}");
}
