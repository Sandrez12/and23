//! `Mapa` recuerda el orden de inserción (paso 2.2 de 1.0): `claves`, `valores`, `paraCada` y `show` lo recorren en ese orden,
//! con y sin el recolector en cada punto seguro.

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

/// Ejecuta normal y con `--gc-stress` y exige la misma salida.
fn ok(src: &str) -> String {
    let normal = run_with(src, false).unwrap_or_else(|e| panic!("falló: {e}\n{src}"));
    let stress = run_with(src, true).unwrap_or_else(|e| panic!("falló con stress: {e}\n{src}"));
    assert_eq!(normal, stress, "el modo stress cambió el resultado:\n{src}");
    normal
}

#[test]
fn recorre_en_el_orden_de_insercion() {
    let src = r#"
fn main() {
    fix m = Mapa<Str, Int>()
    m.pone("zeta", 1)
    m.pone("alfa", 2)
    m.pone("mu", 3)
    show(m)
    show(m.claves())
    show(m.valores())
    m.paraCada { k, v -> show("${k}=${v}") }
}
"#;
    assert_eq!(
        ok(src),
        "{\"zeta\": 1, \"alfa\": 2, \"mu\": 3}\n[\"zeta\", \"alfa\", \"mu\"]\n[1, 2, 3]\nzeta=1\nalfa=2\nmu=3\n"
    );
}

#[test]
fn poner_una_clave_que_ya_esta_conserva_su_lugar() {
    let src = r#"
fn main() {
    fix m = Mapa<Str, Int>()
    m.pone("b", 1)
    m.pone("a", 2)
    m.pone("b", 10)
    show(m)
    show(m.tam())
}
"#;
    assert_eq!(ok(src), "{\"b\": 10, \"a\": 2}\n2\n");
}

#[test]
fn quitar_y_volver_a_poner_deja_la_clave_al_final() {
    let src = r#"
fn main() {
    fix m = Mapa<Int, Str>()
    m.pone(1, "uno")
    m.pone(2, "dos")
    m.pone(3, "tres")
    show(m.quita(2))
    show(m.quita(9))
    show(m)
    m.pone(2, "DOS")
    show(m)
    show(m.contiene(2))
    show(m.obtiene(2))
    show(m.tam())
}
"#;
    assert_eq!(
        ok(src),
        "true\nfalse\n{1: \"uno\", 3: \"tres\"}\n{1: \"uno\", 3: \"tres\", 2: \"DOS\"}\ntrue\nDOS\n3\n"
    );
}

#[test]
fn el_orden_sobrevive_al_crecimiento_y_a_la_compactacion() {
    // 200 claves (el mapa crece varias veces), se quitan las pares (se compacta) y se añaden otras al final.
    let src = r#"
fn main() {
    fix m = Mapa<Int, Int>()
    mut i = 0
    while i < 200 {
        m.pone(i * 7919 % 1009, i)
        i += 1
    }
    i = 0
    while i < 200 {
        if i % 2 == 0 { m.quita(i * 7919 % 1009) }
        i += 1
    }
    i = 200
    while i < 210 {
        m.pone(i * 7919 % 1009, i)
        i += 1
    }
    fix vs = m.valores()
    mut bien = true
    mut j = 1
    while j < vs.tam() {
        if vs.obtiene(j - 1) >= vs.obtiene(j) { bien = false }
        j += 1
    }
    show(m.tam())
    show(vs.tam())
    show(bien)
    show(vs.obtiene(0))
    show(vs.obtiene(vs.tam() - 1))
    show(m.claves().tam())
}
"#;
    assert_eq!(ok(src), "110\n110\ntrue\n1\n209\n110\n");
}

#[test]
fn un_mapa_con_claves_de_objeto_y_el_gc() {
    let src = r#"
thing Punto(fix x: Int, fix y: Int)

fn main() {
    fix m = Mapa<Int, Punto>()
    mut i = 0
    while i < 60 {
        m.pone(60 - i, Punto(i, i * 2))
        i += 1
    }
    mut suma = 0
    m.paraCada { k, p -> suma += k * 1000 + p.y }
    show(suma)
    show(m.claves().obtiene(0))
    show(m.claves().obtiene(59))
}
"#;
    // claves 60..1 en ese orden; k*1000 suma 1830*1000, p.y suma 2*(0+..+59)
    assert_eq!(ok(src), "1833540\n60\n1\n");
}

#[test]
fn cambiar_el_mapa_mientras_se_recorre_no_rompe_nada() {
    let src = r#"
fn main() {
    fix m = Mapa<Int, Int>()
    mut i = 1
    while i <= 5 {
        m.pone(i, i * 10)
        i += 1
    }
    // quita una clave que aún no se visitó y añade otra: la quitada se salta y la nueva no se visita
    m.paraCada { k, v ->
        show("${k}=${v}")
        if k == 2 {
            m.quita(4)
            m.pone(99, 0)
        }
    }
    show(m)
}
"#;
    assert_eq!(
        ok(src),
        "1=10\n2=20\n3=30\n5=50\n{1: 10, 2: 20, 3: 30, 5: 50, 99: 0}\n"
    );
}

#[test]
fn show_de_un_mapa_con_valores_mapas_y_listas() {
    let src = r#"
fn main() {
    fix m = Mapa<Str, Lista<Int>>()
    fix a = Lista<Int>()
    a.agrega(1)
    a.agrega(2)
    m.pone("dos", a)
    m.pone("uno", Lista<Int>())
    show(m)
    fix vacio = Mapa<Str, Int>()
    show(vacio)
}
"#;
    assert_eq!(ok(src), "{\"dos\": [1, 2], \"uno\": []}\n{}\n");
}
