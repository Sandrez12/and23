//! M11: tareas cooperativas y canales.

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

// ---------------------------------------------------------------- tareas

#[test]
fn una_tarea_nueva_empieza_cuando_la_actual_cede_o_espera() {
    let src = r#"
fix t = lanza { show("tarea") }
show("antes")
t.espera()
show("despues")
"#;
    assert_eq!(ok(src), "antes\ntarea\ndespues\n");
}

#[test]
fn cede_reparte_el_turno_en_orden_fifo() {
    let src = r#"
fn trabajador(nombre: Str) -> Tarea {
    return lanza {
        mut i = 0
        while i < 3 {
            show("${nombre}${i}")
            cede()
            i += 1
        }
    }
}
fix a = trabajador("a")
fix b = trabajador("b")
a.espera()
b.espera()
"#;
    assert_eq!(ok(src), "a0\nb0\na1\nb1\na2\nb2\n");
}

#[test]
fn cede_sin_otras_tareas_no_hace_nada() {
    assert_eq!(ok("cede()\nshow(1)\n"), "1\n");
}

#[test]
fn una_tarea_con_resultado_lo_entrega_en_espera() {
    let src = r#"
fix t = lanzaDe<Int> { 6 * 7 }
fix s = lanzaDe<Str> { "hola" }
show(t.espera() + 1)
show(s.espera())
show(t.terminada())
"#;
    assert_eq!(ok(src), "43\nhola\ntrue\n");
}

#[test]
fn espera_relanza_el_fallo_de_la_tarea() {
    let src = r#"
fix t = lanza { fail Fallo("roto") }
attempt {
    t.espera()
    show("no")
} rescue e: Fallo {
    show("capturado: ${e.msg}")
}
fix u = lanzaDe<Int> { fail ErrorES("disco") }
attempt { u.espera() } rescue e: ErrorES { show(e.msg) }
"#;
    assert_eq!(ok(src), "capturado: roto\ndisco\n");
}

#[test]
fn un_error_de_ejecucion_en_una_tarea_aborta_el_programa() {
    let src = r#"
fix t = lanza { show(1 / 0) }
t.espera()
"#;
    assert!(err(src).contains("división"), "{}", err(src));
}

#[test]
fn las_tareas_comparten_variables_sin_carreras() {
    let src = r#"
mut total = 0
fix tareas = Lista<Tarea>()
mut i = 0
while i < 50 {
    tareas.agrega(lanza {
        mut j = 0
        while j < 20 {
            total += 1
            cede()
            j += 1
        }
    })
    i += 1
}
tareas.paraCada { t -> t.espera() }
show(total)
"#;
    assert_eq!(ok(src), "1000\n");
}

#[test]
fn una_tarea_puede_lanzar_otras() {
    let src = r#"
fn fib(n: Int) -> Int {
    if n < 2 { return n }
    fix a = lanzaDe<Int> { fib(n - 1) }
    fix b = lanzaDe<Int> { fib(n - 2) }
    return a.espera() + b.espera()
}
show(fib(10))
"#;
    assert_eq!(ok(src), "55\n");
}

#[test]
fn al_terminar_la_principal_las_demas_se_descartan() {
    let src = r#"
lanza { show("nunca") }
show("fin")
"#;
    assert_eq!(ok(src), "fin\n");
}

// ---------------------------------------------------------------- canales

#[test]
fn un_canal_entrega_en_orden_y_se_cierra() {
    let src = r#"
fix c = Canal<Int>(2)
fix prod = lanza {
    mut i = 0
    while i < 6 {
        c.envia(i)
        i += 1
    }
    c.cierra()
}
mut suma = 0
mut x = c.recibe()
while x != nil {
    show(x)
    suma += x
    x = c.recibe()
}
show("suma ${suma}")
prod.espera()
"#;
    assert_eq!(ok(src), "0\n1\n2\n3\n4\n5\nsuma 15\n");
}

#[test]
fn enviar_a_un_canal_cerrado_es_un_error_atrapable() {
    let src = r#"
fix c = Canal<Int>(1)
c.cierra()
attempt { c.envia(1) } rescue e: ErrorCanal { show(e.msg) }
show(c.recibe())
show(c.estaCerrado())
"#;
    assert_eq!(ok(src), "canal cerrado\nnil\ntrue\n");
}

#[test]
fn varios_productores_y_un_consumidor() {
    let src = r#"
fix c = Canal<Int>(3)
fix hechos = Canal<Bool>(10)
fix n = 4
mut p = 0
while p < n {
    fix base = p * 100
    lanza {
        mut i = 0
        while i < 5 {
            c.envia(base + i)
            i += 1
        }
        hechos.envia(true)
    }
    p += 1
}
fix cierre = lanza {
    mut k = 0
    while k < n {
        hechos.recibe()
        k += 1
    }
    c.cierra()
}
mut cuenta = 0
mut suma = 0
mut x = c.recibe()
while x != nil {
    cuenta += 1
    suma += x
    x = c.recibe()
}
cierre.espera()
show(cuenta)
show(suma)
"#;
    assert_eq!(ok(src), "20\n3040\n");
}

#[test]
fn un_canal_de_capacidad_cero_se_trata_como_uno() {
    let src = r#"
fix c = Canal<Str>(0)
lanza { c.envia("a")
        c.envia("b")
        c.cierra() }
show(c.recibe())
show(c.recibe())
show(c.recibe())
"#;
    assert_eq!(ok(src), "a\nb\nnil\n");
}

// ---------------------------------------------------------------- interbloqueo y recolector

#[test]
fn recibir_de_un_canal_vacio_sin_nadie_que_envie_es_un_interbloqueo() {
    let src = r#"
fix c = Canal<Int>(1)
show(c.recibe())
"#;
    assert!(err(src).contains("interbloqueo"));
}

#[test]
fn dos_tareas_que_se_esperan_son_un_interbloqueo() {
    let src = r#"
fix a = Canal<Int>(1)
fix b = Canal<Int>(1)
fix t = lanza { a.recibe(); b.envia(1) }
b.recibe()
t.espera()
"#;
    assert!(err(src).contains("interbloqueo"));
}

#[test]
fn el_recolector_no_pierde_lo_que_guardan_las_tareas_suspendidas() {
    let src = r#"
fix c = Canal<Str>(1)
fix t = lanzaDe<Int> {
    fix local = ["x", "y", "z"]
    mut suma = 0
    mut s = c.recibe()
    while s != nil {
        suma += len(s) + len(local)
        s = c.recibe()
    }
    return suma
}
mut i = 0
while i < 200 {
    fix basura = ["b${i}", "c${i}"]
    c.envia("m${i}")
    i += 1
}
c.cierra()
show(t.espera())
"#;
    // 200 mensajes: 690 letras en total + 3 por mensaje.
    assert_eq!(ok(src), "1290\n");
}
