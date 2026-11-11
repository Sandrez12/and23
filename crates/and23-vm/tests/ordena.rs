//! `Lista.ordena` (paso 2.4 de 1.0): mezcla estable en O(n log n). Se comprueba contra una inserción de referencia escrita en el
//! propio programa, y que sea rápida, estable y no deje la lista a medias si el comparador falla.

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

#[test]
fn da_lo_mismo_que_la_insercion_de_referencia_en_muchos_casos() {
    let src = r#"
thing Reg(fix clave: Int, fix orden: Int)

/// La inserción estable que había antes: la referencia.
fn referencia(l: Lista<Reg>) {
    mut i = 1
    while i < l.tam() {
        fix x = l.obtiene(i)
        mut j = i - 1
        while j >= 0 && x.clave < l.obtiene(j).clave {
            l.pone(j + 1, l.obtiene(j))
            j -= 1
        }
        l.pone(j + 1, x)
        i += 1
    }
}

fn main() {
    mut semilla = 12345
    mut caso = 0
    mut malos = 0
    mut comprobados = 0
    while caso < 150 {
        // tamaños 0..74 y pocas claves distintas (muchos repetidos) o muchas
        fix n = caso % 75
        fix rango = if caso % 3 == 0 { 3 } else if caso % 3 == 1 { 10 } else { 1000 }
        fix a = Lista<Reg>()
        fix b = Lista<Reg>()
        mut i = 0
        while i < n {
            semilla = (semilla * 1103515245 + 12345) % 2147483648
            fix r = Reg(semilla / 65536 % rango, i)
            a.agrega(r)
            b.agrega(r)
            i += 1
        }
        a.ordena { x, y -> x.clave < y.clave }
        referencia(b)
        mut k = 0
        while k < n {
            if a.obtiene(k) != b.obtiene(k) { malos += 1 }
            k += 1
        }
        comprobados += n
        caso += 1
    }
    show(malos)
    show(comprobados > 4000)
}
"#;
    for stress in [false, true] {
        assert_eq!(
            run_with(src, stress).unwrap(),
            "0\ntrue\n",
            "stress={stress}"
        );
    }
}

#[test]
fn ordena_cien_mil_elementos_en_tiempo_razonable() {
    let src = r#"
fn main() {
    fix l = Lista<Int>()
    mut semilla = 7
    mut i = 0
    while i < 100000 {
        semilla = (semilla * 1103515245 + 12345) % 2147483648
        l.agrega(semilla % 1000000)
        i += 1
    }
    l.ordena { a, b -> a < b }
    mut bien = true
    mut j = 1
    while j < l.tam() {
        if l.obtiene(j - 1) > l.obtiene(j) { bien = false }
        j += 1
    }
    show(bien)
    show(l.tam())
}
"#;
    let start = std::time::Instant::now();
    assert_eq!(run_with(src, false).unwrap(), "true\n100000\n");
    // La inserción tardaba minutos con 100 000 elementos; la mezcla, un par de segundos.
    assert!(
        start.elapsed() < std::time::Duration::from_secs(30),
        "tardó {:?}",
        start.elapsed()
    );
}

#[test]
fn si_el_comparador_falla_la_lista_queda_como_estaba() {
    let src = r#"
fn main() {
    fix l = Lista<Int>()
    mut i = 0
    while i < 40 {
        l.agrega((i * 17) % 23)
        i += 1
    }
    fix antes = "${l}"
    mut llamadas = 0
    attempt {
        l.ordena { a, b ->
            llamadas += 1
            if llamadas == 60 { fail Fallo("comparador roto") }
            return a < b
        }
    } rescue e: Fallo {
        show(e.msg)
    }
    show("${l}" == antes)
}
"#;
    assert_eq!(run_with(src, false).unwrap(), "comparador roto\ntrue\n");
}

#[test]
fn ordena_listas_de_textos_y_deja_intacto_el_orden_de_los_iguales() {
    let src = r#"
fn main() {
    fix l = listaDe(["pera", "Fig", "uva", "kiwi", "fig", "UVA", "Pera"])
    // por largo: los de igual largo conservan su orden original
    l.ordena { a, b -> a.largo() < b.largo() }
    show(l)
    l.ordena { a, b -> a.minus() < b.minus() }
    show(l)
}
"#;
    assert_eq!(
        run_with(src, false).unwrap(),
        "[\"Fig\", \"uva\", \"fig\", \"UVA\", \"pera\", \"kiwi\", \"Pera\"]\n[\"Fig\", \"fig\", \"kiwi\", \"pera\", \"Pera\", \"uva\", \"UVA\"]\n"
    );
}
