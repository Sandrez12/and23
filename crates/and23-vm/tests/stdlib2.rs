//! M12: biblioteca estándar 2 (argumentos, entorno, tiempo, azar, rutas, carpetas, exponentes).

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

// ---------------------------------------------------------------- literales

#[test]
fn los_decimales_aceptan_exponente() {
    let src = r#"
show(1e3)
show(2.5E-2)
show(1_0e+1)
show(1e3 == 1000.0)
show(3e0 + 0.5)
"#;
    assert_eq!(ok(src), "1000.0\n0.025\n100.0\ntrue\n3.5\n");
}

#[test]
fn una_e_sin_digitos_no_es_exponente() {
    // `1e` no es un número: el lexer deja `1` y luego el identificador `e`.
    assert!(err("show(1e)\n").contains("e"));
    assert_eq!(ok("fix e = 2\nshow(3 + e)\n"), "5\n");
}

// ---------------------------------------------------------------- entorno y tiempo

#[test]
fn sin_argumentos_la_lista_esta_vacia() {
    assert_eq!(
        ok("show(argumentos())\nshow(argumentos().tam())\n"),
        "[]\n0\n"
    );
}

#[test]
fn las_variables_de_entorno_inexistentes_dan_nil() {
    assert_eq!(ok("show(entorno(\"AND23_NO_EXISTE_JAMAS\"))\n"), "nil\n");
}

#[test]
fn ahora_avanza_y_duerme_espera_al_menos_lo_pedido() {
    let src = r#"
fix a = ahora()
duerme(40)
fix b = ahora()
show(a > 1700000000000)
show(b - a >= 40)
duerme(0)
duerme(-5)
"#;
    assert_eq!(ok(src), "true\ntrue\n");
}

#[test]
fn duerme_deja_avanzar_a_otras_tareas() {
    let src = r#"
fix c = Canal<Int>(1)
fix t = lanza {
    duerme(20)
    c.envia(7)
}
show(c.recibe())
t.espera()
fix lento = lanza { duerme(60); show("lento") }
fix rapido = lanza { duerme(10); show("rapido") }
lento.espera()
rapido.espera()
"#;
    assert_eq!(ok(src), "7\nrapido\nlento\n");
}

// ---------------------------------------------------------------- azar

#[test]
fn la_misma_semilla_da_la_misma_secuencia() {
    let src = r#"
fn secuencia() -> Lista<Int> {
    semilla(2026)
    fix l = Lista<Int>()
    mut i = 0
    while i < 20 {
        l.agrega(azar(1000))
        i += 1
    }
    return l
}
fix a = secuencia()
fix b = secuencia()
mut iguales = true
mut i = 0
while i < 20 {
    if a.obtiene(i) != b.obtiene(i) { iguales = false }
    i += 1
}
show(iguales)
semilla(1)
fix x = azar(1000)
semilla(2)
fix y = azar(1000)
semilla(1)
show(x == azar(1000))
"#;
    assert_eq!(ok(src), "true\ntrue\n");
}

#[test]
fn azar_queda_dentro_del_rango_y_cubre_los_valores() {
    let src = r#"
semilla(7)
fix vistos = Mapa<Int, Bool>()
mut i = 0
mut dentro = true
while i < 400 {
    fix n = azar(6)
    if n < 0 || n >= 6 { dentro = false }
    vistos.pone(n, true)
    i += 1
}
show(dentro)
show(vistos.tam())
mut f = 0
mut ok = true
while f < 200 {
    fix x = azarF()
    if x < 0.0 || x >= 1.0 { ok = false }
    f += 1
}
show(ok)
show(azar(1))
"#;
    assert_eq!(ok(src), "true\n6\ntrue\n0\n");
}

#[test]
fn azar_con_limite_no_positivo_es_un_error() {
    assert!(err("show(azar(0))\n").contains("límite positivo"));
    assert!(err("show(azar(-3))\n").contains("límite positivo"));
}

// ---------------------------------------------------------------- rutas

#[test]
fn las_rutas_se_unen_y_se_descomponen() {
    let src = r#"
show(unirRuta("a", "b.txt"))
show(unirRuta("a/", "b"))
show(unirRuta("", "b"))
show(unirRuta("a", "/abs"))
show(nombreDe("x/y/c.tar.gz"))
show(nombreDe("solo"))
show(carpetaDe("x/y/c.tar.gz"))
show(carpetaDe("solo"))
show(carpetaDe("/raiz"))
show(extension("x/y/c.tar.gz"))
show(extension("sin"))
show(extension("x/.oculto"))
show(nombreDe("a\\b\\c.jb"))
"#;
    assert_eq!(
        ok(src),
        "a/b.txt\na/b\nb\n/abs\nc.tar.gz\nsolo\nx/y\n\n/\ngz\n\n\nc.jb\n"
    );
}

// ---------------------------------------------------------------- carpetas

#[test]
fn se_crean_listan_y_reconocen_carpetas() {
    let dir = std::env::temp_dir().join("and23_m12_dirs");
    let _ = std::fs::remove_dir_all(&dir);
    let base = dir.to_string_lossy().replace('\\', "/");
    let src = format!(
        r#"
fix base = "{base}"
creaDir(unirRuta(base, "b/sub"))
creaDir(unirRuta(base, "a"))
escribeArchivo(unirRuta(base, "z.txt"), "x")
show(esDir(unirRuta(base, "b/sub")))
show(esDir(unirRuta(base, "z.txt")))
show(listaDir(base))
attempt {{ listaDir(unirRuta(base, "no_existe")) }} rescue e: ErrorES {{ show("ErrorES") }}
attempt {{ listaDir(unirRuta(base, "z.txt")) }} rescue e: Fallo {{ show("Fallo") }}
"#
    );
    let out = ok(&src);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        out,
        "true\nfalse\n[\"a\", \"b\", \"z.txt\"]\nErrorES\nFallo\n"
    );
}

// ---------------------------------------------------------------- errores con traza

#[test]
fn un_error_dentro_del_preludio_se_atribuye_a_quien_lo_llama() {
    let src = "fix l = Lista<Int>()\nshow(\"a\")\nshow(l.obtiene(3))\n";
    let program = parse(src).unwrap();
    let mut session = Session::new(Vec::new());
    let e = session.run(&program, false).unwrap_err();
    assert!(e.message.contains("fuera de rango"));
    let start = e.span.start;
    assert_eq!(&src[start..start + 9], "l.obtiene", "{e:?}");
}

#[test]
fn un_error_lleva_la_cadena_de_llamadas() {
    let src = "fn a(n: Int) -> Int { return 10 / n }\nfn b(n: Int) -> Int { return a(n) + 1 }\nshow(b(0))\n";
    let program = parse(src).unwrap();
    let mut session = Session::new(Vec::new());
    let e = session.run(&program, false).unwrap_err();
    let nombres: Vec<&str> = e.trace.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(
        nombres,
        ["llamado desde `b`", "llamado desde el nivel superior"]
    );
}

#[test]
fn la_traza_de_una_recursion_infinita_se_resume() {
    let src = "fn r(n: Int) -> Int { return r(n + 1) + 1 }\nshow(r(0))\n";
    let program = parse(src).unwrap();
    let mut session = Session::new(Vec::new()).with_max_depth(500);
    let e = session.run(&program, false).unwrap_err();
    assert!(e.message.contains("desbordamiento de pila"));
    assert!(e.trace.len() <= 8 + 1 + 3, "{}", e.trace.len());
    assert!(e
        .trace
        .iter()
        .any(|(s, t)| s.is_none() && t.contains("llamadas más")));
}
