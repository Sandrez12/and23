//! Biblioteca estándar (M10): métodos de Str, conversiones, matemáticas y archivos.

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

fn err(src: &str) -> String {
    match run(src) {
        Ok(out) => panic!("debía fallar, pero imprimió:\n{out}\n{src}"),
        Err(e) => e,
    }
}

/// Archivo temporal único (con barras normales, para escribirlo en un literal del lenguaje).
fn temp_path(tag: &str) -> String {
    std::env::temp_dir()
        .join(format!("and23_std_{tag}_{}.txt", std::process::id()))
        .to_string_lossy()
        .replace('\\', "/")
}

// ---------------------------------------------------------------- texto

#[test]
fn largo_mayusculas_minusculas_y_recorte() {
    let src = r#"
show("  Hola ñandú  ".recorta())
show("Hola ñandú".mayus())
show("HOLA ÑANDÚ".minus())
show("ñandú".largo())
show("".vacia())
show("a".vacia())
"#;
    assert_eq!(
        ok(src),
        "Hola ñandú\nHOLA ÑANDÚ\nhola ñandú\n5\ntrue\nfalse\n"
    );
}

#[test]
fn busqueda_prefijos_y_reemplazo() {
    let src = r#"
show("hola mundo".contiene("mun"))
show("hola mundo".contiene("zz"))
show("hola mundo".indiceDe("mundo"))
show("ñandú pequeño".indiceDe("pequeño"))
show("hola".indiceDe("zz"))
show("hola".empiezaCon("ho"))
show("hola".empiezaCon("la"))
show("hola".terminaCon("la"))
show("a-b-c".reemplaza("-", "+"))
show("abc".reemplaza("", "x"))
show("aaa".reemplaza("a", "bb"))
"#;
    assert_eq!(
        ok(src),
        "true\nfalse\n5\n6\n-1\ntrue\nfalse\ntrue\na+b+c\nabc\nbbbbbb\n"
    );
}

#[test]
fn trozos_e_indices_de_caracteres() {
    let src = r#"
show("ñandú".trozo(1, 3))
show("hola".trozo(0, 100))
show("hola".trozo(3, 1))
show("hola".trozo(-5, 2))
show("ñandú"[0] + "ñandú"[4])
show("abc"[1])
"#;
    assert_eq!(ok(src), "an\nhola\n\nho\nñú\nb\n");
}

#[test]
fn indices_fuera_de_rango_y_cadenas_inmutables() {
    assert!(err("show(\"abc\"[3])")
        .contains("índice fuera de rango: 3 en una cadena de 3 carácter(es)"));
    assert!(err("show(\"abc\"[0 - 1])").contains("índice fuera de rango: -1"));
    assert!(err("fix s = \"abc\"\ns[0] = \"x\"").contains("las cadenas no se pueden modificar"));
}

#[test]
fn parte_y_lineas() {
    let src = r#"
show("a,b,,c".parte(",").texto())
show("a,b".parte(",,").texto())
show("abc".parte("").texto())
show("".parte(",").texto())
show(",".parte(",").texto())
show("uno\ndos\r\ntres\n".lineas().texto())
show("uno\n\ndos".lineas().texto())
show("".lineas().tam())
show("sin salto".lineas().texto())
"#;
    assert_eq!(
        ok(src),
        "[\"a\", \"b\", \"\", \"c\"]\n[\"a,b\"]\n[\"a\", \"b\", \"c\"]\n[\"\"]\n[\"\", \"\"]\n[\"uno\", \"dos\", \"tres\"]\n[\"uno\", \"\", \"dos\"]\n0\n[\"sin salto\"]\n"
    );
}

#[test]
fn une_cadenas() {
    let src = r#"
show(une("a-b-c".parte("-"), ", "))
show(une(Lista<Str>(), "x") == "")
fix uno = Lista<Str>()
uno.agrega("solo")
show(une(uno, "--"))
"#;
    assert_eq!(ok(src), "a, b, c\ntrue\nsolo\n");
}

#[test]
fn los_metodos_de_str_funcionan_en_lambdas_y_con_resultados_encadenados() {
    let src = r#"
fix palabras = "uno dos tres".parte(" ")
show(palabras.map { p -> p.largo() }.texto())
show(palabras.map { p -> p.mayus() }.texto())
show("  a b  ".recorta().parte(" ").tam())
fn cuantas(s: Str) -> Int = s.parte(" ").filter { w -> !w.vacia() }.tam()
show(cuantas("a  b   c"))
"#;
    assert_eq!(ok(src), "[3, 3, 4]\n[\"UNO\", \"DOS\", \"TRES\"]\n2\n3\n");
}

#[test]
fn errores_de_tipos_en_los_metodos_de_str() {
    assert!(err("show(\"a\".nada())").contains("Str no tiene el método `nada`"));
    assert!(err("show(\"a\".parte())").contains("espera 1 argumento(s), pero recibió 0"));
    assert!(err("show(\"a\".contiene(3))").contains("se esperaba Str, pero es Int"));
    assert!(err("fix n = 5\nshow(n.largo())").contains("solo los objetos tienen métodos"));
}

// ---------------------------------------------------------------- números

#[test]
fn conversion_de_texto_a_numeros() {
    let src = r#"
show(Int.parse("42"))
show(Int.parse("-7"))
show(Int.parse("  8 "))
show(Int.parse("+5"))
show(Int.parse("4.5"))
show(Int.parse("abc"))
show(Int.parse(""))
show(Int.parse("99999999999999999999"))
show(Float.parse("2.5"))
show(Float.parse("1e3"))
show(Float.parse("7"))
show(Float.parse("nan"))
show(Float.parse("inf"))
show(Float.parse("x"))
show((Int.parse("12") ?: 0) + 1)
"#;
    assert_eq!(
        ok(src),
        "42\n-7\n8\n5\nnil\nnil\nnil\nnil\n2.5\n1000.0\n7.0\nnil\nnil\nnil\n13\n"
    );
}

#[test]
fn el_resultado_de_parse_se_estrecha_como_cualquier_opcional() {
    let src = r#"
fn suma(a: Str, b: Str) -> Int {
    fix x = Int.parse(a)
    fix y = Int.parse(b)
    if x == nil || y == nil { return 0 - 1 }
    return x + y
}
show(suma("2", "3"))
show(suma("2", "?"))
"#;
    assert_eq!(ok(src), "5\n-1\n");
}

#[test]
fn matematicas() {
    let src = r#"
show(absoluto(0 - 5))
show(absoluto(5))
show(minimo(3, 9))
show(maximo(3, 9))
show(absolutoF(0.0 - 1.5))
show(minimoF(1.5, 2.5))
show(maximoF(1.5, 2.5))
show(raiz(16.0))
show(potencia(2.0, 10.0))
show(piso(2.7))
show(piso(0.0 - 2.5))
show(techo(2.1))
show(techo(0.0 - 2.1))
show(redondea(2.5))
show(redondea(0.0 - 2.5))
show(redondea(2.4))
show(aFloat(3) / 2.0)
"#;
    assert_eq!(
        ok(src),
        "5\n5\n3\n9\n1.5\n1.5\n2.5\n4.0\n1024.0\n2\n-3\n3\n-2\n3\n-3\n2\n1.5\n"
    );
}

#[test]
fn convertir_un_float_fuera_de_rango_a_int_es_un_error() {
    assert!(err("show(piso(potencia(10.0, 400.0)))").contains("no se puede convertir inf a Int"));
    assert!(err("show(redondea(potencia(10.0, 30.0)))").contains("no se puede convertir"));
}

#[test]
fn las_funciones_integradas_no_se_redefinen_ni_son_valores() {
    assert!(err("fn existe(r: Str) -> Bool = true").contains("función integrada"));
    assert!(err("fix f = raiz").contains("función integrada"));
    assert!(err("show(raiz(4))").contains("no hay conversión implícita"));
    assert!(err("show(raiz(1.0, 2.0))").contains("espera 1 argumento(s)"));
}

// ---------------------------------------------------------------- archivos

#[test]
fn escribir_leer_agregar_y_borrar() {
    let path = temp_path("rw");
    let src = format!(
        r#"
fix ruta = "{path}"
show(existe(ruta))
escribeArchivo(ruta, "uno\ndos\n")
show(existe(ruta))
agregaArchivo(ruta, "tres\n")
show(leeArchivo(ruta).largo())
show(leeLineas(ruta).texto())
escribeArchivo(ruta, "reemplazado")
show(leeArchivo(ruta))
borraArchivo(ruta)
show(existe(ruta))
"#
    );
    assert_eq!(
        ok(&src),
        "false\ntrue\n13\n[\"uno\", \"dos\", \"tres\"]\nreemplazado\nfalse\n"
    );
}

#[test]
fn los_archivos_con_unicode_y_con_bom_se_leen_bien() {
    let path = temp_path("utf8");
    std::fs::write(&path, "\u{feff}ñandú\r\nhola\r\n").unwrap();
    let src = format!(
        r#"
fix lineas = leeLineas("{path}")
show(lineas.texto())
show(lineas.obtiene(0).largo())
"#
    );
    assert_eq!(ok(&src), "[\"ñandú\", \"hola\"]\n5\n");
    std::fs::remove_file(&path).ok();
}

#[test]
fn los_errores_de_archivos_lanzan_error_es() {
    let path = temp_path("no_existe");
    let src = format!(
        r#"
attempt {{
    show(leeArchivo("{path}"))
}} rescue e: ErrorES {{
    show("ES: ${{e.msg.contiene("no se pudo leer")}}")
}}
attempt {{
    borraArchivo("{path}")
}} rescue e: Fallo {{
    show("Fallo: ${{e.msg.contiene("no se pudo borrar")}}")
}}
attempt {{
    escribeArchivo("{path}.dir/x.txt", "a")
}} rescue e: ErrorES {{
    show("ES: ${{e.msg.contiene("no se pudo escribir")}}")
}}
"#
    );
    assert_eq!(ok(&src), "ES: true\nFallo: true\nES: true\n");
}

#[test]
fn un_error_de_archivo_sin_atrapar_senala_la_llamada() {
    let path = temp_path("sin_atrapar");
    let msg = err(&format!("show(leeArchivo(\"{path}\"))"));
    assert!(msg.contains("no se pudo leer"), "{msg}");
}

#[test]
fn error_es_hereda_de_fallo_y_se_puede_lanzar() {
    let src = r#"
attempt { fail ErrorES("a mano") } rescue e: Fallo { show(e.msg) }
"#;
    assert_eq!(ok(src), "a mano\n");
}

#[test]
fn el_recolector_no_pierde_los_textos_de_las_nativas() {
    let src = r#"
mut total = 0
mut i = 0
while i < 2000 {
    fix partes = "a,bb,ccc,dddd".parte(",")
    total += partes.map { p -> p.mayus().largo() }.reduce(0, { a, n -> a + n })
    i += 1
}
show(total)
"#;
    assert_eq!(ok(src), "20000\n");
}

#[test]
fn un_programa_puede_redefinir_una_funcion_del_preludio() {
    let src = r#"
fn minimo(a: Str, b: Str) -> Str = a + b
fn une(xs: Lista<Str>, sep: Str) -> Str = "mía"
show(minimo("x", "y"))
show(une("a,b".parte(","), "-"))
show("a,b".parte(",").tam())
"#;
    assert_eq!(ok(src), "xy\nmía\n2\n");
    // Lo del preludio que usaba la original sigue usándola.
    let src = r#"
fn _str_parte(s: Str, sep: Str) -> Lista<Str> = Lista<Str>()
show("a b".lineas().tam())
show("a b".parte(" ").tam())
"#;
    assert_eq!(ok(src), "1\n0\n");
    // Dos definiciones en el mismo programa siguen siendo un error.
    assert!(err("fn minimo() { }\nfn minimo() { }").contains("ya está definida"));
}
