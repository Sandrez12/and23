//! Closures y lambdas (M8): captura por valor y por referencia, funciones como valores, métodos
//! genéricos y los métodos de `Lista`/`Mapa` que reciben lambdas.

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

// ---------------------------------------------------------------- lambdas básicas

#[test]
fn lambda_en_una_variable() {
    let src = r#"
fix doble = { x: Int -> x * 2 }
show(doble(21))
fix suma = { a: Int, b: Int -> a + b }
show(suma(2, 3))
fix saluda = { -> "hola" }
show(saluda())
fix f: (Int) -> Str = { n -> "n=${n}" }
show(f(7))
show(doble)
"#;
    assert_eq!(ok(src), "42\n5\nhola\nn=7\n<función>\n");
}

#[test]
fn funciones_con_nombre_como_valores() {
    let src = r#"
fn triple(n: Int) -> Int = n * 3
fn aplica(f: (Int) -> Int, v: Int) -> Int = f(v)
fix g = triple
show(g(5))
show(aplica(triple, 4))
show(aplica({ n -> n + 1 }, 4))
show(aplica(g, 2))
"#;
    assert_eq!(ok(src), "15\n12\n5\n6\n");
}

#[test]
fn llamadas_encadenadas_y_funciones_que_devuelven_funciones() {
    let src = r#"
fn compone(f: (Int) -> Int, g: (Int) -> Int) -> (Int) -> Int = { x -> g(f(x)) }
fn sumador(k: Int) -> (Int) -> (Int) -> Int = { a -> { b -> a + b + k } }
fn main() {
    fix h = compone({ x -> x + 1 }, { x -> x * 10 })
    show(h(4))
    show(sumador(100)(1)(2))
    fix s = sumador(1)
    show(s(2)(3))
}
"#;
    assert_eq!(ok(src), "50\n103\n6\n");
}

#[test]
fn return_dentro_de_una_lambda_sale_de_la_lambda() {
    let src = r#"
fn main() {
    fix f = { x: Int -> if x > 0 { return "positivo" }
        "no positivo" }
    show(f(1))
    show(f(0))
    show("sigue")
}
"#;
    assert_eq!(ok(src), "positivo\nno positivo\nsigue\n");
}

// ---------------------------------------------------------------- capturas

#[test]
fn captura_de_fix_por_valor() {
    let src = r#"
fn main() {
    fix k = 10
    fix f = { x: Int -> x + k }
    show(f(1))
    fix fs: [() -> Int] = []
    mut i = 0
    while i < 3 {
        fix j = i * 10
        push(fs, { j + 1 })
        i += 1
    }
    show(fs[0]() + fs[1]() + fs[2]())
}
"#;
    assert_eq!(ok(src), "11\n33\n");
}

#[test]
fn captura_de_mut_por_referencia() {
    let src = r#"
fn main() {
    mut x = 0
    fix inc = { x += 1 }
    fix get = { x }
    inc()
    inc()
    show(get())
    x = 50
    show(get())
    show(x)
    mut acc = 10
    fix mas = { n: Int -> acc += n
        acc }
    show(mas(5))
    show(acc)
}
"#;
    assert_eq!(ok(src), "2\n50\n50\n15\n15\n");
}

#[test]
fn un_contador_que_sobrevive_a_su_funcion() {
    let src = r#"
fn contador() -> () -> Int {
    mut n = 0
    return { n += 1
        n }
}
fn main() {
    fix c1 = contador()
    fix c2 = contador()
    show("${c1()} ${c1()} ${c2()} ${c1()}")
}
"#;
    assert_eq!(ok(src), "1 2 1 3\n");
}

#[test]
fn captura_a_traves_de_varias_lambdas() {
    let src = r#"
fn main() {
    mut total = 0
    fix fuera = { a: Int ->
        fix dentro = { b: Int -> total += a * b }
        dentro(1)
        dentro(2)
    }
    fuera(10)
    fuera(100)
    show(total)
}
"#;
    assert_eq!(ok(src), "330\n");
}

#[test]
fn las_globales_del_nivel_superior_se_usan_sin_capturar() {
    let src = r#"
mut cuenta = 0
fix inc = { cuenta += 1 }
inc()
inc()
show(cuenta)
fn lee() -> Int = cuenta
show(lee())
"#;
    assert_eq!(ok(src), "2\n2\n");
}

#[test]
fn una_lambda_captura_this_y_los_campos() {
    let src = r#"
thing Contador(fix paso: Int) {
    mut cuenta = 0
    fn creaIncremento() -> () -> Int = { cuenta += paso
        cuenta }
    fn conAviso(f: (Int) -> Int) -> Int {
        fix g = { x: Int -> f(x) + cuenta }
        return g(10)
    }
}
fn main() {
    fix c = Contador(5)
    fix inc = c.creaIncremento()
    show(inc())
    show(inc())
    show(c.cuenta)
    show(c.conAviso({ x -> x * 2 }))
}
"#;
    assert_eq!(ok(src), "5\n10\n10\n30\n");
}

// ---------------------------------------------------------------- campos de tipo función

#[test]
fn campos_que_guardan_funciones() {
    let src = r#"
thing Boton(fix nombre: Str, mut alClic: (Str) -> Unit) {
    mut veces = 0
    fn pulsa() {
        veces += 1
        alClic("${nombre}#${veces}")
    }
}
fn main() {
    mut log = ""
    fix b = Boton("ok", { t -> log = log + t + ";" })
    b.pulsa()
    b.pulsa()
    show(log)
    b.alClic = { t -> show("nuevo ${t}") }
    b.pulsa()
    b.alClic("directo")
}
"#;
    assert_eq!(ok(src), "ok#1;ok#2;\nnuevo ok#3\nnuevo directo\n");
}

#[test]
fn funciones_opcionales() {
    let src = r#"
fn main() {
    mut cb: ((Int) -> Int)? = nil
    show(cb == nil)
    cb = { x -> x + 1 }
    if cb != nil {
        show(cb(1))
    }
}
"#;
    assert_eq!(ok(src), "true\n2\n");
}

// ---------------------------------------------------------------- Lista y Mapa con lambdas

#[test]
fn lista_map_filter_reduce() {
    let src = r#"
fn main() {
    fix l = Lista<Int>()
    l.agrega(3)
    l.agrega(1)
    l.agrega(2)
    show(l.map { x -> x * 2 }.texto())
    show(l.filter { x -> x > 1 }.texto())
    show(l.reduce(0, { acc, x -> acc + x }))
    show(l.map { x -> "n${x}" }.texto())
    show(l.map { x -> x > 1 }.texto())
    show(l.map { x -> x * 1.5 }.texto())
    mut suma = 0
    l.paraCada { x -> suma += x }
    show(suma)
    show(l.busca { x -> x > 5 })
    show(l.busca { x -> x > 1 })
    show(l.alguno { x -> x == 2 })
    show(l.todos { x -> x > 1 })
    l.ordena { a, b -> a < b }
    show(l.texto())
    l.ordena { a, b -> a > b }
    show(l.texto())
}
"#;
    assert_eq!(
        ok(src),
        "[6, 2, 4]\n[3, 2]\n6\n[\"n3\", \"n1\", \"n2\"]\n[true, false, true]\n[4.5, 1.5, 3.0]\n6\nnil\n3\ntrue\nfalse\n[1, 2, 3]\n[3, 2, 1]\n"
    );
}

#[test]
fn lista_ordena_objetos_y_es_estable() {
    let src = r#"
thing Persona(fix nombre: Str, fix edad: Int)
fn main() {
    fix l = Lista<Persona>()
    l.agrega(Persona("ana", 30))
    l.agrega(Persona("beto", 20))
    l.agrega(Persona("caro", 30))
    l.agrega(Persona("dani", 20))
    l.ordena { a, b -> a.edad < b.edad }
    show(l.map { p -> p.nombre }.texto())
}
"#;
    assert_eq!(ok(src), "[\"beto\", \"dani\", \"ana\", \"caro\"]\n");
}

#[test]
fn mapa_para_cada_y_contar_con_lambdas() {
    let src = r#"
fn main() {
    fix m = Mapa<Str, Int>()
    fix palabras = Lista<Str>()
    palabras.agrega("a")
    palabras.agrega("b")
    palabras.agrega("a")
    palabras.paraCada { p -> m.pone(p, (m.obtiene(p) ?: 0) + 1) }
    mut total = 0
    m.paraCada { k, v -> total += v }
    show(total)
    show(m.obtiene("a"))
}
"#;
    assert_eq!(ok(src), "3\n2\n");
}

#[test]
fn metodos_genericos_propios() {
    let src = r#"
thing Caja<T>(fix valor: T) {
    fn mapea<R>(f: (T) -> R) -> Caja<R> = Caja(f(valor))
    fn combina<U, R>(otra: Caja<U>, f: (T, U) -> R) -> Caja<R> = Caja(f(valor, otra.valor))
}
fn main() {
    fix c = Caja(20)
    show(c.mapea { x -> x + 1 }.valor)
    show(c.mapea { x -> "v${x}" }.valor)
    show(c.combina(Caja("z"), { n, s -> "${s}${n}" }).valor)
    show(c.mapea<Float> { x -> x * 0.5 }.valor)
}
"#;
    assert_eq!(ok(src), "21\nv20\nz20\n10.0\n");
}

#[test]
fn funciones_genericas_con_lambdas() {
    let src = r#"
fn aplica<A, B>(x: A, f: (A) -> B) -> B = f(x)
fn veces<T>(n: Int, f: (Int) -> T) -> Lista<T> {
    fix l = Lista<T>()
    mut i = 0
    while i < n {
        l.agrega(f(i))
        i += 1
    }
    return l
}
fn main() {
    show(aplica(4, { x -> x * x }))
    show(aplica("hola", { s -> len(s) }))
    show(veces(3, { i -> i * 10 }).texto())
    show(veces(2, { i -> "x${i}" }).map { s -> s + "!" }.texto())
}
"#;
    assert_eq!(ok(src), "16\n4\n[0, 10, 20]\n[\"x0!\", \"x1!\"]\n");
}

#[test]
fn lambdas_anidadas_en_map() {
    let src = r#"
fn main() {
    fix filas = Lista<Int>()
    filas.agrega(1)
    filas.agrega(2)
    fix tabla = filas.map { f -> filas.map { c -> f * c } }
    show(tabla.map { fila -> fila.texto() }.texto())
}
"#;
    assert_eq!(ok(src), "[\"[1, 2]\", \"[2, 4]\"]\n");
}

// ---------------------------------------------------------------- recolector

#[test]
fn muchas_lambdas_y_celdas_sobreviven_al_gc() {
    let src = r#"
fn crea(k: Int) -> () -> Int {
    mut n = k
    return { n += 1
        n }
}
fn main() {
    fix fs = Lista<() -> Int>()
    mut i = 0
    while i < 300 {
        fs.agrega(crea(i))
        i += 1
    }
    mut total = 0
    fs.paraCada { f -> total += f() + f() }
    show(total)
    show(fs.map { f -> f() }.reduce(0, { a, b -> a + b }))
}
"#;
    // sum_i (2i + 3) = 2 * 44850 + 900 ; segunda: sum_i (i + 3) = 44850 + 900
    assert_eq!(ok(src), "90600\n45750\n");
}

// ---------------------------------------------------------------- errores de ejecución y de tipos

#[test]
fn una_excepcion_cruza_las_lambdas() {
    let src = r#"
thing Malo(fix m: Str) : Fallo(m)
fn main() {
    fix l = Lista<Int>()
    l.agrega(1)
    l.agrega(2)
    attempt {
        l.paraCada { x -> if x == 2 { fail Malo("dos") }
            show(x) }
    } rescue e: Malo {
        show("rescatado ${e.msg}")
    }
}
"#;
    assert_eq!(ok(src), "1\nrescatado dos\n");
}

#[test]
fn errores_de_tipos_en_lambdas() {
    assert!(err("fix f = { x -> x }").contains("no se puede deducir el tipo del parámetro `x`"));
    assert!(err("fix f: (Int) -> Int = { a, b -> a }")
        .contains("tiene 2 parámetro(s), pero aquí se espera una función de 1"));
    assert!(err("fix f: (Int) -> Int = { a -> \"s\" }").contains("se esperaba Int, pero es Str"));
    assert!(err("fix f: (Int) -> Str = { a -> a }").contains("se esperaba Str, pero es Int"));
    assert!(err("fix f = { x: Int -> x }\nshow(f(\"a\"))").contains("se esperaba Int, pero es Str"));
    assert!(err("fix f = { x: Int -> x }\nshow(f(1, 2))").contains("espera 1 argumento(s)"));
    assert!(err("fix n = 5\nshow(n(3))").contains("no es una función"));
    assert!(err("fix l = Lista<Int>()\nl.agrega(1)\nfix r = l.map { x -> nil }").contains("`nil`"));
    assert!(err("fn f() { fix g: () -> Int = { fix a = 1 }\n }").contains("debe devolver"));
}

#[test]
fn el_estrechamiento_no_cruza_una_lambda_que_cambia_la_variable() {
    // La lambda ve el tipo declarado de una variable `mut` (puede ser nil cuando se ejecute).
    let ok_src = r#"
fn main() {
    mut y: Int? = 5
    if y != nil {
        fix f = { y }
        y = nil
        show(f())
    }
}
"#;
    assert_eq!(ok(ok_src), "nil\n");
    let bad = r#"
fn main() {
    mut y: Int? = 5
    if y != nil {
        fix f = { y + 1 }
        show(f())
    }
}
"#;
    assert!(err(bad).contains("no admite Int? y Int"));
    // Una lambda que asigna la variable la deja sin estrechar en el resto del bloque.
    let widened = r#"
fn main() {
    mut y: Int? = 5
    if y != nil {
        fix f = { y = nil }
        f()
        show(y + 1)
    }
}
"#;
    assert!(err(widened).contains("no admite Int? y Int"));
    // Una variable `fix` estrechada sigue estrechada dentro de la lambda.
    let fixed = r#"
fn main() {
    fix y: Int? = 5
    if y != nil {
        fix f = { y + 1 }
        show(f())
    }
}
"#;
    assert_eq!(ok(fixed), "6\n");
}

#[test]
fn las_llamadas_a_valores_se_comprueban_en_ejecucion() {
    // Una función que no existe como valor no se puede usar así: envuélvela.
    assert!(err("fix f = len").contains("función integrada"));
    assert!(err("fn id<T>(x: T) -> T = x\nfix f = id").contains("genérica"));
}

#[test]
fn una_lambda_global_puede_ser_recursiva() {
    let src = r#"
fix fact: (Int) -> Int = { n -> if n <= 1 { return 1 }
    return n * fact(n - 1) }
show(fact(10))
"#;
    assert_eq!(
        ok(src),
        "3628800
"
    );
    // Dentro de una función la variable aún no existe cuando se crea la lambda.
    let local = r#"
fn main() {
    fix fact: (Int) -> Int = { n -> if n <= 1 { return 1 }
        return n * fact(n - 1) }
    show(fact(5))
}
"#;
    assert!(err(local).contains("`fact` no está definida"));
}

#[test]
fn las_lambdas_persisten_entre_entradas_del_repl() {
    let mut session = Session::new(Vec::new());
    for line in [
        "mut n = 0",
        "fix inc = { n += 1\n n }",
        "inc()",
        "inc()",
        "fn dos(f: (Int) -> Int) -> Int = f(f(1))",
        "show(dos({ x -> x + n }))",
    ] {
        let program = parse(line).unwrap_or_else(|d| panic!("{}: {}", line, d.message));
        session
            .run(&program, true)
            .unwrap_or_else(|d| panic!("{}: {}", line, d.message));
    }
    let out = String::from_utf8(session.out().clone()).unwrap();
    assert_eq!(out, "1\n2\n5\n");
}

#[test]
fn el_ejemplo_de_lambdas_corre() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/lambdas.jb");
    let src = std::fs::read_to_string(path).expect("examples/lambdas.jb");
    let out = ok(&src);
    assert!(out.contains("pares al cuadrado"), "{out}");
}

/// Cada lambda de una llamada genérica se mira al deducir y al comprobar; sin memoria, 40 niveles
/// serían 2^40 verificaciones.
#[test]
fn las_lambdas_anidadas_no_cuestan_exponencial() {
    let depth = 40;
    let mut inner = format!("a{}", depth - 1);
    for i in (0..depth).rev() {
        inner = format!("xs.map {{ a{i} -> {inner} }}.tam()");
    }
    let src = format!(
        "fix xs = Lista<Int>()
xs.agrega(1)
show({inner})
"
    );
    assert_eq!(
        ok(&src),
        "1
"
    );
}

#[test]
fn la_memoria_de_lambdas_distingue_los_tipos_de_las_variables_a_la_vista() {
    // La misma forma de lambda en dos funciones con variables de distinto tipo: cada una se verifica.
    let src = r#"
fn a() -> Int {
    fix k = 1
    fix xs = Lista<Int>()
    xs.agrega(5)
    return xs.map { x -> x + k }.obtiene(0)
}
fn b() -> Str {
    fix k = "!"
    fix xs = Lista<Str>()
    xs.agrega("hola")
    return xs.map { x -> x + k }.obtiene(0)
}
show(a())
show(b())
"#;
    assert_eq!(
        ok(src),
        "6
hola!
"
    );
}
