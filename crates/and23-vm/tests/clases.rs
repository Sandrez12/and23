//! Clases, campos, métodos y constructores (M4).

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
fn campos_de_la_cabecera_y_acceso() {
    let src = "thing P(fix x: Int, mut y: Int)\nfix p = P(1, 2)\nshow(p.x)\nshow(p.y)\np.y = 7\np.y += 1\nshow(p.y)\nshow(p)";
    assert_eq!(ok(src), "1\n2\n8\nP(x=1, y=8)\n");
}

#[test]
fn los_parametros_sin_fix_ni_mut_no_son_campos() {
    let src = "thing C(n: Int) {\n  fix doble = n * 2\n  mut copia = n\n}\nfix c = C(5)\nshow(c)\nshow(c.doble)";
    assert_eq!(ok(src), "C(doble=10, copia=5)\n10\n");
    let err = run("thing C(n: Int)\nshow(C(1).n)").unwrap_err();
    assert_eq!(err, "la clase `C` no tiene el campo `n`");
}

#[test]
fn campos_del_cuerpo_ven_los_anteriores_y_las_funciones() {
    let src = r#"
fn base() -> Int = 10
thing T(fix a: Int) {
    fix b = a + base()
    fix c = b * 2
    mut texto = "${a}-${b}-${c}"
}
show(T(1))
"#;
    assert_eq!(ok(src), "T(a=1, b=11, c=22, texto=\"1-11-22\")\n");
}

#[test]
fn metodos_leen_y_escriben_campos_por_su_nombre() {
    let src = r#"
thing Cuenta(mut saldo: Int) {
    mut ops = 0
    fn deposita(n: Int) {
        saldo += n
        ops += 1
    }
    fn total() -> Int = saldo
}
fix c = Cuenta(10)
c.deposita(5)
c.deposita(7)
show(c.total())
show(c.ops)
"#;
    assert_eq!(ok(src), "22\n2\n");
}

#[test]
fn this_desambigua_y_un_parametro_tapa_al_campo() {
    let src = r#"
thing A(mut x: Int) {
    fn suma(x: Int) -> Int = x + this.x
    fn fijaX(nuevo: Int) { this.x = nuevo }
    fn mismo() -> A = this
}
fix a = A(10)
show(a.suma(5))
a.fijaX(3)
show(a.x)
a.mismo().fijaX(99)
show(a.x)
"#;
    assert_eq!(ok(src), "15\n3\n99\n");
}

#[test]
fn un_metodo_llama_a_otros_con_y_sin_this() {
    let src = r#"
thing M(mut n: Int) {
    fn inc() { n += 1 }
    fn dos() {
        inc()
        this.inc()
    }
    fn valor() -> Int = n
}
fix m = M(0)
m.dos()
m.dos()
show(m.valor())
"#;
    assert_eq!(ok(src), "4\n");
}

#[test]
fn un_metodo_puede_llamar_a_una_funcion_global() {
    let src = "fn triple(n: Int) = n * 3\nthing T(fix v: Int) { fn r() -> Int = triple(v) }\nshow(T(4).r())";
    assert_eq!(ok(src), "12\n");
}

#[test]
fn recursion_en_metodos() {
    let src = r#"
thing F() {
    fn fib(n: Int) -> Int {
        if n < 2 { return n }
        return fib(n - 1) + this.fib(n - 2)
    }
}
show(F().fib(20))
"#;
    assert_eq!(ok(src), "6765\n");
}

#[test]
fn las_clases_pueden_usarse_antes_de_declararse_y_referirse_entre_si() {
    let src = r#"
show(Par(1, 2).suma())
show(dobla(Par(3, 4)))
fn dobla(p: Par) -> Par = Par(p.a * 2, p.b * 2)
thing Par(fix a: Int, fix b: Int) {
    fn suma() -> Int = a + b
    fn otro(c: Caja) -> Int = c.valor + a
}
thing Caja(fix valor: Int) {
    fn con(p: Par) -> Int = p.otro(this)
}
show(Caja(10).con(Par(1, 1)))
"#;
    assert_eq!(ok(src), "3\nPar(a=6, b=8)\n11\n");
}

#[test]
fn objetos_en_arreglos_y_encadenados() {
    let src = r#"
thing Caja(mut v: Int) {
    fn mas(n: Int) -> Caja {
        v += n
        return this
    }
}
fix cajas = [Caja(1), Caja(2)]
cajas[0].mas(5).mas(5)
cajas[1].v *= 10
show(cajas)
fix vacio: [Caja] = []
push(vacio, Caja(0))
show(len(vacio))
"#;
    assert_eq!(ok(src), "[Caja(v=11), Caja(v=20)]\n1\n");
}

#[test]
fn los_objetos_son_referencias() {
    let src = "thing B(mut v: Int)\nfix a = B(1)\nfix b = a\nb.v = 9\nshow(a.v)\nfn toca(x: B) { x.v += 1 }\ntoca(a)\nshow(b.v)";
    assert_eq!(ok(src), "9\n10\n");
}

#[test]
fn imprimir_objetos_con_cadenas_y_anidados() {
    let src = r#"
thing In(fix s: Str)
thing Out(fix i: In, fix xs: [In])
fix o = Out(In("a\"b"), [In("c")])
show(o)
show("${o}")
"#;
    let line = "Out(i=In(s=\"a\\\"b\"), xs=[In(s=\"c\")])\n";
    assert_eq!(ok(src), format!("{line}{line}"));
}

#[test]
fn ciclos_se_imprimen_y_se_recolectan() {
    let src = r#"
thing N() { mut hijos: [N] = [] }
fix n = N()
push(n.hijos, n)
show(n)
"#;
    assert_eq!(ok(src), "N(hijos=[...])\n");
    // Miles de ciclos descartados: el GC los libera aunque se apunten entre sí.
    let src = r#"
thing N() { mut hijos: [N] = [] }
mut i = 0
while i < 200000 {
    fix n = N()
    push(n.hijos, n)
    i += 1
}
show(i)
"#;
    let program = parse(src).unwrap();
    let mut session = Session::new(Vec::new());
    session.run(&program, false).unwrap();
    let stats = session.heap_stats();
    assert!(stats.collections > 0 && stats.freed > 100_000, "{stats:?}");
    assert!(stats.slots < 100_000, "{stats:?}");
}

#[test]
fn muchos_objetos_vivos_sobreviven_en_modo_stress() {
    let src = r#"
thing Item(fix id: Int, fix nombre: Str)
fix todos: [Item] = []
mut i = 0
while i < 400 {
    push(todos, Item(i, "item ${i}"))
    i += 1
}
mut suma = 0
mut j = 0
while j < len(todos) {
    suma += todos[j].id + len(todos[j].nombre)
    j += 1
}
show(suma)
show(todos[399])
"#;
    // sum(0..399) = 79800; largos de "item N": 10*6 + 90*7 + 300*8 = 3090
    assert_eq!(ok(src), "82890\nItem(id=399, nombre=\"item 399\")\n");
}

#[test]
fn el_estado_de_las_clases_persiste_en_el_repl() {
    let mut session = Session::new(Vec::new()).with_gc_stress(true);
    for input in [
        "thing P(fix x: Int, mut y: Int) { fn suma() -> Int = x + y }",
        "fix p = P(1, 2)",
        "p.y = 10",
        "p.suma()",
        "p",
        "thing Q(fix p: P)",
        "Q(p).p.y",
    ] {
        let program = parse(input).unwrap();
        session.run(&program, true).unwrap();
    }
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        "11\nP(x=1, y=10)\n10\n"
    );
}

#[test]
fn un_error_en_el_repl_no_deja_la_clase_a_medias() {
    let mut session = Session::new(Vec::new());
    let bad = parse("thing R(fix x: Int)\nshow(R(1).zz)").unwrap();
    assert!(session.run(&bad, true).is_err());
    // La clase de la entrada fallida no existe: puede declararse de nuevo.
    let good = parse("thing R(fix x: Int)\nR(5)").unwrap();
    session.run(&good, true).unwrap();
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        "R(x=5)\n"
    );
}

#[test]
fn errores_de_ejecucion_dentro_de_metodos_apuntan_al_sitio() {
    let src = "thing T(fix v: [Int]) { fn primero() -> Int = v[5] }\nshow(T([1]).primero())";
    let err = run(src).unwrap_err();
    assert!(err.contains("índice fuera de rango: 5"), "{err}");
}

#[test]
fn profundidad_de_llamadas_con_metodos() {
    let src = "thing R() { fn f(n: Int) -> Int = f(n + 1) }\nshow(R().f(0))";
    let err = run(src).unwrap_err();
    assert!(err.contains("desbordamiento de pila"), "{err}");
}

#[test]
fn el_ejemplo_de_clases_corre() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/clases.jb"
    ))
    .unwrap();
    let out = ok(&src);
    assert!(out.contains("a + b, por 2 = (8.0, 12.0)"), "{out}");
    assert!(out.contains("k = 15, otro = 15"), "{out}");
    assert!(out.contains("valor final: 13500"), "{out}");
}
