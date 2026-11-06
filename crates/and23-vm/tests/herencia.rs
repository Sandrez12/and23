//! Herencia, interfaces, despacho virtual, `is`, `as` y `super` (M5).

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

const ANIMALES: &str = r#"
thing Animal(fix nombre: Str) {
    mut edad = 0
    fn cumple() { edad += 1 }
    fn habla() -> Str = "..."
    fn dice() -> Str = "${nombre} dice ${this.habla()}"
}
thing Perro(nombre: Str) : Animal(nombre) {
    fn habla() -> Str = "guau"
}
thing Gato(nombre: Str, fix vidas: Int) : Animal(nombre) {
    fn habla() -> Str = "miau"
}
"#;

#[test]
fn los_campos_heredados_van_primero_y_se_imprimen() {
    let src = format!("{ANIMALES}\nshow(Perro(\"Rex\"))\nshow(Gato(\"Mishi\", 9))");
    assert_eq!(
        ok(&src),
        "Perro(nombre=\"Rex\", edad=0)\nGato(nombre=\"Mishi\", edad=0, vidas=9)\n"
    );
}

#[test]
fn los_hijos_usan_campos_y_metodos_heredados() {
    let src = format!(
        "{ANIMALES}\nfix p = Perro(\"Rex\")\np.cumple()\np.cumple()\nshow(p.edad)\nshow(p.nombre)\np.edad = 10\nshow(p)"
    );
    assert_eq!(ok(&src), "2\nRex\nPerro(nombre=\"Rex\", edad=10)\n");
}

#[test]
fn el_metodo_sobrescrito_se_elige_por_la_clase_real() {
    let src = format!(
        "{ANIMALES}\nfix zoo: [Animal] = [Animal(\"x\"), Perro(\"Rex\"), Gato(\"Mishi\", 7)]\nmut i = 0\nwhile i < len(zoo) {{ show(zoo[i].dice())\n i += 1 }}"
    );
    assert_eq!(ok(&src), "x dice ...\nRex dice guau\nMishi dice miau\n");
}

#[test]
fn this_y_las_llamadas_sin_receptor_son_virtuales() {
    // `dice` está en Animal pero llama a `habla` del hijo, tanto con `this.` como sin él.
    let src = r#"
thing A() {
    fn quien() -> Str = "A"
    fn conThis() -> Str = this.quien()
    fn sinThis() -> Str = quien()
}
thing B() : A() {
    fn quien() -> Str = "B"
}
fix b = B()
show(b.conThis())
show(b.sinThis())
show(A().conThis())
"#;
    assert_eq!(ok(src), "B\nB\nA\n");
}

#[test]
fn super_llama_al_padre_sin_despacho_virtual() {
    let src = r#"
thing A() { fn f() -> Str = "A.f" }
thing B() : A() { fn f() -> Str = "B>" + super.f() }
thing C() : B() { fn f() -> Str = "C>" + super.f() }
show(C().f())
show(B().f())
"#;
    assert_eq!(ok(src), "C>B>A.f\nB>A.f\n");
}

#[test]
fn super_alcanza_un_metodo_heredado_del_abuelo() {
    let src = r#"
thing A() { fn f() -> Int = 1 }
thing B() : A()
thing C() : B() { fn f() -> Int = super.f() + 10 }
show(C().f())
"#;
    assert_eq!(ok(src), "11\n");
}

#[test]
fn los_argumentos_del_constructor_padre_pueden_ser_expresiones() {
    let src = r#"
thing Base(fix a: Int, fix b: Str) {
    fix doble = a * 2
}
thing Hijo(n: Int) : Base(n + 1, "x${n}") {
    fix triple = n * 3
}
show(Hijo(4))
"#;
    assert_eq!(ok(src), "Hijo(a=5, b=\"x4\", doble=10, triple=12)\n");
}

#[test]
fn el_orden_de_inicializacion_es_padre_cabecera_y_cuerpo() {
    let src = r#"
fn marca(s: Str) -> Str {
    show("init ${s}")
    return s
}
thing A(fix x: Str) { fix a = marca("A.cuerpo") }
thing B(y: Str) : A(marca("B->A")) { fix b = marca("B.cuerpo") }
thing C() : B(marca("C->B")) { fix c = marca("C.cuerpo") }
show(C())
"#;
    assert_eq!(
        ok(src),
        "init C->B\ninit B->A\ninit A.cuerpo\ninit B.cuerpo\ninit C.cuerpo\nC(x=\"B->A\", a=\"A.cuerpo\", b=\"B.cuerpo\", c=\"C.cuerpo\")\n"
    );
}

#[test]
fn interfaces_como_tipo_de_parametro_variable_y_arreglo() {
    let src = r#"
kind Figura { fn area() -> Int }
thing Cuadrado(fix l: Int) : Figura { fn area() -> Int = l * l }
thing Rect(fix a: Int, fix b: Int) : Figura { fn area() -> Int = a * b }
fn doble(f: Figura) -> Int = f.area() * 2
fix f: Figura = Cuadrado(3)
show(f.area())
show(doble(Rect(2, 5)))
fix todas: [Figura] = [Cuadrado(2), Rect(2, 3)]
mut total = 0
mut i = 0
while i < len(todas) { total += todas[i].area()
 i += 1 }
show(total)
"#;
    assert_eq!(ok(src), "9\n20\n10\n");
}

#[test]
fn una_clase_implementa_varias_interfaces_y_hereda_la_implementacion() {
    let src = r#"
kind A { fn a() -> Str }
kind B { fn b() -> Str }
thing Base() { fn a() -> Str = "a de Base" }
thing X() : Base(), A, B { fn b() -> Str = "b de X" }
fix x = X()
fix pa: A = x
fix pb: B = x
show(pa.a())
show(pb.b())
show(x is A && x is B && x is Base)
"#;
    assert_eq!(ok(src), "a de Base\nb de X\ntrue\n");
}

#[test]
fn is_distingue_clase_real_ancestros_e_interfaces() {
    let src = r#"
kind Habla { fn h() -> Int }
thing A() : Habla { fn h() -> Int = 1 }
thing B() : A()
thing C() : A()
fix x: A = B()
show(x is B)
show(x is A)
show(x is C)
show(x is Habla)
fix h: Habla = C()
show(h is C)
show(h is B)
"#;
    assert_eq!(ok(src), "true\ntrue\nfalse\ntrue\ntrue\nfalse\n");
}

#[test]
fn as_convierte_y_falla_si_no_corresponde() {
    let src = r#"
thing A() { }
thing B() : A() { fn solo() -> Str = "solo de B" }
thing C() : A()
fix x: A = B()
show((x as B).solo())
fix y: A = C()
show((y as B).solo())
"#;
    let err = run(src).unwrap_err();
    assert_eq!(err, "no se puede convertir un `C` a `B`");
}

#[test]
fn as_hacia_arriba_y_hacia_interfaz() {
    let src = r#"
kind K { fn k() -> Int }
thing A() : K { fn k() -> Int = 7 }
fix a = A()
show((a as K).k())
show((a as A).k())
"#;
    assert_eq!(ok(src), "7\n7\n");
}

#[test]
fn un_metodo_heredado_por_dos_ramas_conserva_su_implementacion() {
    let src = r#"
thing A() { fn f() -> Int = 1
 fn g() -> Int = f() * 10 }
thing B() : A() { fn f() -> Int = 2 }
thing C() : A()
show(A().g())
show(B().g())
show(C().g())
"#;
    assert_eq!(ok(src), "10\n20\n10\n");
}

#[test]
fn un_hijo_puede_pasarse_como_el_padre_y_devolverse() {
    let src = r#"
thing A(fix n: Int)
thing B(m: Int) : A(m * 2)
fn crea(x: Int) -> A = B(x)
fn lee(a: A) -> Int = a.n
show(lee(crea(5)))
show(lee(B(1)))
"#;
    assert_eq!(ok(src), "10\n2\n");
}

#[test]
fn herencia_entre_entradas_del_repl() {
    let mut session = Session::new(Vec::new()).with_gc_stress(true);
    for input in [
        "kind K { fn k() -> Int }",
        "thing A(fix x: Int) : K { fn k() -> Int = x }",
        "thing B(y: Int) : A(y + 1) { fn k() -> Int = super.k() * 100 }",
        "fix b = B(1)",
        "b.k()",
        "b is A",
        "(b as K).k()",
        "b",
    ] {
        let program = parse(input).unwrap();
        session.run(&program, true).unwrap();
    }
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        "200\ntrue\n200\nB(x=2)\n"
    );
}

#[test]
fn un_error_en_el_repl_no_deja_tipos_a_medias() {
    let mut session = Session::new(Vec::new());
    let bad = parse("thing P(fix x: Int)\nthing Q() : P(1) { fn f() -> Int = zzz }").unwrap();
    assert!(session.run(&bad, true).is_err());
    let good = parse("thing P(fix x: Int)\nthing Q() : P(1)\nQ()").unwrap();
    session.run(&good, true).unwrap();
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        "Q(x=1)\n"
    );
}

#[test]
fn objetos_de_subclases_sobreviven_al_gc() {
    let src = r#"
thing Item(fix id: Int)
thing Especial(i: Int, fix etiqueta: Str) : Item(i)
fix todos: [Item] = []
mut i = 0
while i < 300 {
    push(todos, Especial(i, "e${i}"))
    i += 1
}
mut suma = 0
mut j = 0
while j < len(todos) {
    suma += todos[j].id
    if todos[j] is Especial { suma += 1000 }
    j += 1
}
show(suma)
show(todos[299])
"#;
    assert_eq!(ok(src), "344850\nEspecial(id=299, etiqueta=\"e299\")\n");
}

#[test]
fn el_ejemplo_polimorfico_corre() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/polimorfismo.jb"
    ))
    .unwrap();
    let out = ok(&src);
    assert!(out.contains("patas en total: 14"), "{out}");
    assert!(
        out.contains("[gato] Mishi (4 patas, 1 años): gato de 9 vidas"),
        "{out}"
    );
    assert!(out.contains("guau (agudito)!"), "{out}");
    assert!(
        out.contains("la figura mayor es un círculo de área 12.56"),
        "{out}"
    );
}
