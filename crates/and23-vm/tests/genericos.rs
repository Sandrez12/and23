//! Genéricos con erasure: `Lista<T>`, `Mapa<K, V>` y declaraciones genéricas propias (M7).

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

// ---------------------------------------------------------------- Lista<T>

#[test]
fn lista_de_enteros() {
    let src = r#"
fix l = Lista<Int>()
show(l.vacia())
l.agrega(10)
l.agrega(20)
l.agrega(30)
show(l.tam())
show(l.obtiene(1))
l.pone(1, 99)
show(l)
show(l.texto())
show(l.indiceDe(30))
show(l.indiceDe(7))
show(l.contiene(99))
show(l.quita())
show(l.tam())
show(l.vacia())
"#;
    assert_eq!(
        ok(src),
        "true\n3\n20\nLista(items=[10, 99, 30])\n[10, 99, 30]\n2\n-1\ntrue\n30\n2\nfalse\n"
    );
}

#[test]
fn lista_inserta_quita_y_copia() {
    let src = r#"
fix l = Lista<Str>()
l.agrega("a")
l.agrega("c")
l.inserta(1, "b")
l.inserta(0, "inicio")
l.inserta(l.tam(), "fin")
show(l.texto())
show(l.quitaEn(0))
show(l.quitaEn(l.tam() - 1))
show(l.texto())
fix c = l.copia()
c.agrega("extra")
show(l.tam())
show(c.tam())
show(c.texto())
"#;
    assert_eq!(
        ok(src),
        "[\"inicio\", \"a\", \"b\", \"c\", \"fin\"]\ninicio\nfin\n[\"a\", \"b\", \"c\"]\n3\n4\n[\"a\", \"b\", \"c\", \"extra\"]\n"
    );
}

#[test]
fn lista_de_objetos_listas_y_opcionales() {
    let src = r#"
thing P(fix n: Int)
fix ps = Lista<P>()
fix a = P(1)
fix b = P(2)
ps.agrega(a)
ps.agrega(b)
show(ps.indiceDe(b))
show(ps.indiceDe(P(2)))
show(ps.obtiene(0).n)
fix anidada = Lista<Lista<Int>>()
fix uno = Lista<Int>()
uno.agrega(1)
anidada.agrega(uno)
anidada.agrega(Lista<Int>())
anidada.obtiene(1).agrega(5)
show(anidada.obtiene(0).tam() + anidada.obtiene(1).tam())
fix ops = Lista<Int?>()
ops.agrega(1)
ops.agrega(nil)
show(ops)
show(ops.obtiene(1) ?: 0 - 1)
"#;
    assert_eq!(ok(src), "1\n-1\n1\n2\nLista(items=[1, nil])\n-1\n");
}

#[test]
fn el_tipo_de_la_lista_se_deduce_del_contexto() {
    let src = r#"
fix a: Lista<Int> = Lista()
a.agrega(1)
fn recibe(l: Lista<Str>) -> Int = l.tam()
fn crea() -> Lista<Str> = Lista()
show(recibe(Lista()))
show(crea().tam())
show(a.tam())
"#;
    assert_eq!(ok(src), "0\n0\n1\n");
}

// ---------------------------------------------------------------- Mapa<K, V>

#[test]
fn mapa_basico() {
    let src = r#"
fix m = Mapa<Str, Int>()
show(m.vacio())
m.pone("uno", 1)
m.pone("dos", 2)
m.pone("uno", 11)
show(m.tam())
show(m.obtiene("uno"))
show(m.obtiene("dos"))
show(m.obtiene("tres"))
show(m.contiene("dos"))
show(m.contiene("tres"))
show(m.quita("uno"))
show(m.quita("uno"))
show(m.tam())
show(m.obtiene("uno") ?: 0 - 1)
"#;
    assert_eq!(
        ok(src),
        "true\n2\n11\n2\nnil\ntrue\nfalse\ntrue\nfalse\n1\n-1\n"
    );
}

#[test]
fn mapa_crece_y_conserva_todo() {
    let src = r#"
fix m = Mapa<Int, Str>()
mut i = 0
while i < 1000 {
    m.pone(i, "v${i}")
    i += 1
}
show(m.tam())
mut bien = 0
mut j = 0
while j < 1000 {
    if m.obtiene(j) == "v${j}" { bien += 1 }
    j += 1
}
show(bien)
mut k = 0
while k < 1000 {
    if k % 2 == 0 { m.quita(k) }
    k += 1
}
show(m.tam())
show(m.obtiene(500))
show(m.obtiene(501))
fix claves = m.claves()
fix valores = m.valores()
show(claves.tam() + valores.tam())
"#;
    assert_eq!(ok(src), "1000\n1000\n500\nnil\nv501\n1000\n");
}

#[test]
fn mapa_con_claves_de_distintos_tipos() {
    let src = r#"
thing K(fix id: Int)
fix a = K(1)
fix b = K(1)
fix m = Mapa<K, Str>()
m.pone(a, "primero")
m.pone(b, "segundo")
show(m.tam())
show(m.obtiene(a))
show(m.obtiene(b))
fix f = Mapa<Float, Int>()
f.pone(1.5, 1)
f.pone(2.0, 2)
show(f.obtiene(1.5))
show(f.obtiene(2.0))
fix bo = Mapa<Bool, Str>()
bo.pone(true, "si")
bo.pone(false, "no")
show(bo.obtiene(true))
show(bo.obtiene(false))
fix m2 = Mapa<Str, Lista<Int>>()
m2.pone("xs", Lista<Int>())
m2.obtiene("xs")?.agrega(7)
show(m2.obtiene("xs")?.tam())
show(m2.obtiene("ys")?.tam())
"#;
    assert_eq!(ok(src), "2\nprimero\nsegundo\n1\n2\nsi\nno\n1\nnil\n");
}

#[test]
fn mapa_con_valores_opcionales_no_anida_opcionales() {
    let src = r#"
fix m = Mapa<Str, Int?>()
m.pone("a", 1)
m.pone("b", nil)
show(m.obtiene("a"))
show(m.obtiene("b"))
show(m.obtiene("c"))
show(m.contiene("b"))
"#;
    assert_eq!(ok(src), "1\nnil\nnil\ntrue\n");
}

#[test]
fn las_cadenas_iguales_son_la_misma_clave() {
    let src = r#"
fix m = Mapa<Str, Int>()
m.pone("ho" + "la", 1)
m.pone("hola", 2)
fix clave = "ho"
show(m.tam())
show(m.obtiene(clave + "la"))
"#;
    assert_eq!(ok(src), "1\n2\n");
}

// ---------------------------------------------------------------- clases, interfaces y funciones genéricas

#[test]
fn clase_generica_con_argumentos_deducidos_o_explicitos() {
    let src = r#"
thing Caja<T>(fix valor: T) {
    fn doble() -> Par<T, T> = Par(valor, valor)
}
thing Par<A, B>(fix a: A, fix b: B) {
    fn cambia() -> Par<B, A> = Par(b, a)
}
fix c1 = Caja(5)
fix c2 = Caja("hola")
fix c3 = Caja<Float>(1.5)
fix c4: Caja<Int?> = Caja<Int?>(nil)
show(c1.valor + 1)
show(c2.valor + "!")
show(c3.valor * 2.0)
show(c4.valor)
show(c1.doble())
show(Par(1, "x").cambia())
"#;
    assert_eq!(
        ok(src),
        "6\nhola!\n3.0\nnil\nPar(a=5, b=5)\nPar(a=\"x\", b=1)\n"
    );
}

#[test]
fn funciones_genericas_con_inferencia_y_argumentos_explicitos() {
    let src = r#"
fn primero<T>(xs: [T]) -> T = xs[0]
fn dos<T>(a: T, b: T) -> [T] = [a, b]
fn conv<A, B>(a: A, b: B) -> Par<B, A> = Par(b, a)
fn vacio<T>() -> [T] {
    fix r: [T] = []
    return r
}
thing Par<A, B>(fix a: A, fix b: B)
show(primero([1, 2, 3]))
show(primero(["a"]))
show(dos(1, 2))
show(conv(1, "x"))
show(len(vacio<Int>()))
show(primero<Float>([2.5]))
"#;
    assert_eq!(ok(src), "1\na\n[1, 2]\nPar(a=\"x\", b=1)\n0\n2.5\n");
}

#[test]
fn herencia_entre_clases_genericas() {
    let src = r#"
thing Base<T>(fix x: T) {
    fn dame() -> T = x
}
thing Hijo<U>(y: U) : Base<U>(y) {
    fn otro() -> U = dame()
}
thing Fijo(n: Int) : Base<Int>(n * 2) {
    fn suma() -> Int = dame() + 1
}
fix h = Hijo("s")
show(h.dame() + h.otro())
fix f = Fijo(5)
show(f.suma())
show(f.x)
fix b: Base<Int> = f
show(b.dame())
show(f is Base)
"#;
    assert_eq!(ok(src), "ss\n11\n10\n10\ntrue\n");
}

#[test]
fn pila_hereda_de_lista() {
    let src = r#"
thing Pila<T>() : Lista<T>() {
    fn empuja(x: T) { agrega(x) }
    fn saca() -> T = quita()
}
fix p = Pila<Int>()
p.empuja(1)
p.empuja(2)
show(p.saca())
show(p.tam())
fix l: Lista<Int> = p
show(l.texto())
"#;
    assert_eq!(ok(src), "2\n1\n[1]\n");
}

#[test]
fn interfaz_generica() {
    let src = r#"
kind Fuente<T> { fn siguiente() -> T? }
thing Cuenta(fix n: Int) : Fuente<Int> {
    mut i = 0
    fn siguiente() -> Int? {
        if i >= n { return nil }
        i += 1
        return i * 10
    }
}
thing Letras() : Fuente<Str> {
    mut hecho = false
    fn siguiente() -> Str? {
        if hecho { return nil }
        hecho = true
        return "a"
    }
}
fn todo<T>(f: Fuente<T>) -> Lista<T> {
    fix r = Lista<T>()
    mut x = f.siguiente()
    while x != nil {
        r.agrega(x)
        x = f.siguiente()
    }
    return r
}
show(todo(Cuenta(3)))
show(todo(Letras()))
fix f: Fuente<Int> = Cuenta(1)
show(f.siguiente())
show(f.siguiente())
"#;
    assert_eq!(
        ok(src),
        "Lista(items=[10, 20, 30])\nLista(items=[\"a\"])\n10\nnil\n"
    );
}

#[test]
fn cotas_permiten_llamar_metodos_del_parametro() {
    let src = r#"
kind Ordenable<T> { fn antesDe(o: T) -> Bool }
thing Num(fix v: Int) : Ordenable<Num> {
    fn antesDe(o: Num) -> Bool = v < o.v
}
fn minimo<T: Ordenable<T>>(a: T, b: T) -> T {
    if a.antesDe(b) { return a }
    return b
}
fn menorDe<T: Ordenable<T>>(xs: Lista<T>) -> T {
    mut m = xs.obtiene(0)
    mut i = 1
    while i < xs.tam() {
        m = minimo(m, xs.obtiene(i))
        i += 1
    }
    return m
}
show(minimo(Num(3), Num(2)).v)
fix l = Lista<Num>()
l.agrega(Num(9))
l.agrega(Num(4))
l.agrega(Num(7))
show(menorDe(l).v)
"#;
    assert_eq!(ok(src), "2\n4\n");
}

#[test]
fn una_cota_de_clase_acepta_subclases() {
    let src = r#"
thing Animal(fix n: Str)
thing Perro(n: Str) : Animal(n)
fn nombre<T: Animal>(a: T) -> Str = a.n
thing Jaula<T: Animal>(fix dentro: T) {
    fn quien() -> Str = dentro.n
}
show(nombre(Perro("rex")))
show(Jaula(Perro("toby")).quien())
"#;
    assert_eq!(ok(src), "rex\ntoby\n");
}

// ---------------------------------------------------------------- erasure, igualdad, builtins

#[test]
fn igualdad_de_valores_cadenas_y_referencias() {
    let src = r#"
thing P()
fix a = P()
fix b = P()
fix xs = [1]
fix ys = [1]
show(a == a)
show(a == b)
show(a != b)
show(xs == xs)
show(xs == ys)
show("ab" == "a" + "b")
show(1 == 1.0)
fix n: Int? = nil
fix m: Int? = 3
show(n == m)
show(m == 3)
show(n == nil)
"#;
    assert_eq!(
        ok(src),
        "true\nfalse\ntrue\ntrue\nfalse\ntrue\ntrue\nfalse\ntrue\ntrue\n"
    );
}

#[test]
fn hash_y_pop() {
    let src = r#"
show(hash("hola") == hash("ho" + "la"))
show(hash("a") == hash("b"))
show(hash(1) == hash(1.0))
show(hash(5) >= 0)
show(hash(0 - 5) >= 0)
show(hash("x") >= 0)
fix xs = [1, 2, 3]
show(pop(xs))
show(xs)
fix vacio: [Int] = []
pop(vacio)
"#;
    let err = run(src).unwrap_err();
    assert_eq!(err, "`pop` sobre un arreglo vacío");
    let program = parse(src).unwrap();
    let mut session = Session::new(Vec::new());
    let _ = session.run(&program, false);
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        "true\nfalse\ntrue\ntrue\ntrue\ntrue\n3\n[1, 2]\n"
    );
}

#[test]
fn los_genericos_no_encajonan_valores() {
    // Un Mapa<Int, Int> guarda enteros directamente; el recolector solo ve objetos reales.
    let src = r#"
fix m = Mapa<Int, Int>()
mut i = 0
while i < 200 { m.pone(i, i * 2)
 i += 1 }
mut total = 0
mut j = 0
while j < 200 { total += m.obtiene(j) ?: 0
 j += 1 }
show(total)
"#;
    assert_eq!(ok(src), "39800\n");
}

#[test]
fn muchas_estructuras_genericas_sobreviven_al_gc() {
    let src = r#"
fix m = Mapa<Str, Lista<Int>>()
mut i = 0
while i < 120 {
    fix clave = "k${i % 10}"
    if !m.contiene(clave) { m.pone(clave, Lista<Int>()) }
    m.obtiene(clave)?.agrega(i)
    i += 1
}
mut total = 0
fix ks = m.claves()
mut j = 0
while j < ks.tam() {
    total += (m.obtiene(ks.obtiene(j))?.tam() ?: 0)
    j += 1
}
show(m.tam())
show(total)
"#;
    assert_eq!(ok(src), "10\n120\n");
}

#[test]
fn los_genericos_persisten_entre_entradas_del_repl() {
    let mut session = Session::new(Vec::new()).with_gc_stress(true);
    for input in [
        "thing Caja<T>(fix v: T) { fn dame() -> T = v }",
        "fix c = Caja(41)",
        "c.dame() + 1",
        "fn id<T>(x: T) -> T = x",
        "id(\"hola\")",
        "fix l = Lista<Int>()",
        "l.agrega(1)",
        "l.texto()",
    ] {
        let program = parse(input).unwrap();
        session.run(&program, true).unwrap();
    }
    assert_eq!(
        String::from_utf8(session.out().clone()).unwrap(),
        "42\nhola\n[1]\n"
    );
}

#[test]
fn el_ejemplo_de_genericos_corre() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/genericos.jb"
    ))
    .unwrap();
    let out = ok(&src);
    assert!(out.starts_with("[1, 4, 9, 16, 25]\n"), "{out}");
    assert!(out.contains("a=3 b=2 c=1 d=nil\n"), "{out}");
    assert!(
        out.contains("500 entradas: 500, 123 -> 15129, 999 -> nil\n"),
        "{out}"
    );
    assert!(out.contains("la menor es 1.9\n"), "{out}");
    assert!(out.ends_with("[\"ja\", \"ja\", \"ja\"]\n[0, 0]\n"), "{out}");
}
