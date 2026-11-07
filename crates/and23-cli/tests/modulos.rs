//! M9: proyectos de varios archivos (`use`, `open`, alias).
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn and23() -> Command {
    Command::new(env!("CARGO_BIN_EXE_and23"))
}

/// Carpeta temporal única con los archivos dados (`ruta`, contenido); se borra al terminar.
struct Proyecto {
    dir: PathBuf,
}

impl Proyecto {
    fn nuevo(tag: &str, archivos: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!("and23_mod_{tag}_{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        for (ruta, texto) in archivos {
            let path = dir.join(ruta);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, texto).unwrap();
        }
        Proyecto { dir }
    }

    fn ejecuta(&self, principal: &str, extra: &[&str]) -> Output {
        and23()
            .current_dir(&self.dir)
            .args(extra)
            .arg(principal)
            .output()
            .unwrap()
    }
}

impl Drop for Proyecto {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn out(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn err(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

#[test]
fn ejemplo_de_proyecto_de_varios_archivos() {
    let ruta = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/proyecto/main.jb");
    let normal = and23().arg(&ruta).output().unwrap();
    assert!(normal.status.success(), "{}", err(&normal));
    assert_eq!(
        out(&normal),
        "círculo #1: 12.56636\nrectángulo #2: 13.5\nmayor: 9\npi al cuadrado: 9.869587728099999\n"
    );
    let estres = and23().arg("--gc-stress").arg(&ruta).output().unwrap();
    assert_eq!(out(&estres), out(&normal));
}

#[test]
fn los_archivos_importados_se_ejecutan_una_vez_y_antes() {
    let p = Proyecto::nuevo(
        "orden",
        &[
            (
                "main.jb",
                "use \"a\"\nuse \"b\"\nshow(\"main\")\nshow(sumaA() + sumaB())",
            ),
            (
                "a.jb",
                "use \"base\"\nshow(\"a\")\nopen fn sumaA() -> Int = valor()",
            ),
            (
                "b.jb",
                "use \"base\"\nshow(\"b\")\nopen fn sumaB() -> Int = valor()",
            ),
            ("base.jb", "show(\"base\")\nopen fn valor() -> Int = 21"),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(out(&o), "base\na\nb\nmain\n42\n");
}

#[test]
fn lo_privado_no_se_ve_desde_otro_archivo() {
    let p = Proyecto::nuevo(
        "privado",
        &[
            ("main.jb", "use \"util\"\nshow(secreto())"),
            (
                "util.jb",
                "fn secreto() -> Int = 7\nopen fn publico() -> Int = secreto()",
            ),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    let e = err(&o);
    assert!(e.contains("`secreto`"), "{e}");
    assert!(e.contains("es privado en `util`"), "{e}");
    assert!(e.contains("main.jb:2:6"), "{e}");
}

#[test]
fn dos_archivos_pueden_tener_el_mismo_nombre_privado() {
    let p = Proyecto::nuevo(
        "mismonombre",
        &[
            (
                "main.jb",
                "use \"a\"\nuse \"b\"\nshow(fa() * 10 + fb())\nfn ayuda() -> Int = 3\nshow(ayuda())",
            ),
            ("a.jb", "fn ayuda() -> Int = 1\nopen fn fa() -> Int = ayuda()"),
            ("b.jb", "fn ayuda() -> Int = 2\nopen fn fb() -> Int = ayuda()"),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(out(&o), "12\n3\n");
}

#[test]
fn un_error_de_tipos_en_un_modulo_senala_ese_archivo() {
    let p = Proyecto::nuevo(
        "tipos",
        &[
            ("main.jb", "use \"lib/m\"\nshow(f())"),
            ("lib/m.jb", "// nada\nopen fn f() -> Int = \"hola\""),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    let e = err(&o);
    assert!(e.contains("lib/m.jb:2:"), "{e}");
    assert!(e.contains("open fn f() -> Int = \"hola\""), "{e}");
}

#[test]
fn un_error_de_ejecucion_en_un_modulo_senala_ese_archivo() {
    let p = Proyecto::nuevo(
        "ejecucion",
        &[
            ("main.jb", "use \"m\"\nshow(divide(1, 0))"),
            ("m.jb", "open fn divide(a: Int, b: Int) -> Int = a / b"),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    assert!(err(&o).contains("m.jb:1:"), "{}", err(&o));
}

#[test]
fn las_dependencias_circulares_se_rechazan() {
    let p = Proyecto::nuevo(
        "ciclo",
        &[
            ("main.jb", "use \"a\""),
            ("a.jb", "use \"b\""),
            ("b.jb", "use \"a\""),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    let e = err(&o);
    assert!(
        e.contains("dependencia circular entre archivos: a -> b -> a"),
        "{e}"
    );
    assert!(e.contains("b.jb:1:1"), "{e}");
}

#[test]
fn un_use_de_un_archivo_inexistente_se_explica() {
    let p = Proyecto::nuevo("falta", &[("main.jb", "use \"nada\"")]);
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    assert!(
        err(&o).contains("no se encontró el archivo `nada.jb`"),
        "{}",
        err(&o)
    );
}

#[test]
fn el_alias_cualifica_funciones_clases_tipos_y_variables() {
    let p = Proyecto::nuevo(
        "alias",
        &[
            (
                "main.jb",
                "use \"geo\" como g\n\
                 fix p: g.Punto = g.Punto(1, 2)\n\
                 show(g.suma(p))\n\
                 show(g.ORIGEN.x)\n\
                 fn tamano(q: g.Punto) -> Int = q.x + q.y\n\
                 show(tamano(p))\n\
                 show(p is g.Punto)",
            ),
            (
                "geo.jb",
                "open thing Punto(fix x: Int, fix y: Int)\n\
                 open fix ORIGEN = Punto(0, 0)\n\
                 open fn suma(p: Punto) -> Int = p.x + p.y + ORIGEN.x",
            ),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(out(&o), "3\n0\n3\ntrue\n");
}

#[test]
fn un_alias_pide_que_el_nombre_este_abierto() {
    let p = Proyecto::nuevo(
        "alias_cerrado",
        &[
            ("main.jb", "use \"m\" como m\nshow(m.oculta())"),
            ("m.jb", "fn oculta() -> Int = 1"),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    assert!(
        err(&o).contains("`m` no tiene `oculta` abierto"),
        "{}",
        err(&o)
    );
}

#[test]
fn los_conflictos_de_nombres_entre_imports_se_detectan() {
    let p = Proyecto::nuevo(
        "choque",
        &[
            ("main.jb", "use \"a\"\nuse \"b\"\nshow(f())"),
            ("a.jb", "open fn f() -> Int = 1"),
            ("b.jb", "open fn f() -> Int = 2"),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    assert!(
        err(&o).contains("`f` viene de dos `use` distintos"),
        "{}",
        err(&o)
    );

    let p = Proyecto::nuevo(
        "choque_propio",
        &[
            ("main.jb", "use \"a\"\nfn f() -> Int = 0"),
            ("a.jb", "open fn f() -> Int = 1"),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    assert!(
        err(&o).contains("se declara en este archivo y también viene de"),
        "{}",
        err(&o)
    );
}

#[test]
fn una_variable_mut_no_se_puede_abrir() {
    let p = Proyecto::nuevo(
        "open_mut",
        &[("main.jb", "use \"a\""), ("a.jb", "open mut n = 0")],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    assert!(err(&o).contains("no se puede abrir"), "{}", err(&o));
}

#[test]
fn las_clases_pueden_heredar_de_clases_de_otro_archivo() {
    let p = Proyecto::nuevo(
        "herencia",
        &[
            (
                "main.jb",
                "use \"base\"\n\
                 thing Perro(n: Str) : Animal(n) { fn habla() -> Str = \"guau\" }\n\
                 fix a: Animal = Perro(\"rex\")\n\
                 show(a.habla())\n\
                 attempt { fail Roto(\"x\") } rescue e: Roto { show(\"roto ${e.msg}\") }",
            ),
            (
                "base.jb",
                "open thing Animal(fix nombre: Str) { fn habla() -> Str = \"...\" }\n\
                 open thing Roto(fix m: Str) : Fallo(m)",
            ),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(out(&o), "guau\nroto x\n");
}

#[test]
fn el_repl_no_admite_use() {
    let mut hijo = and23()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    hijo.stdin
        .take()
        .unwrap()
        .write_all(b"use \"x\"\n1 + 1\n")
        .unwrap();
    let o = hijo.wait_with_output().unwrap();
    assert!(
        err(&o).contains("`use` solo funciona en archivos"),
        "{}",
        err(&o)
    );
    assert!(out(&o).contains('2'));
}

#[test]
fn disasm_de_un_proyecto_muestra_las_funciones_de_todos_los_archivos() {
    let p = Proyecto::nuevo(
        "disasm",
        &[
            ("main.jb", "use \"m\"\nfn main() { show(f()) }"),
            ("m.jb", "open fn f() -> Int = 1"),
        ],
    );
    let o = p.ejecuta("main.jb", &["--disasm"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("m.f"), "{}", out(&o));
}

#[test]
fn los_errores_muestran_la_ruta_del_archivo_normalizada() {
    let p = Proyecto::nuevo(
        "ruta",
        &[
            (
                "main.jb",
                "use \"./lib/../lib/m\"
show(f())",
            ),
            ("lib/m.jb", "open fn f() -> Int = \"x\""),
        ],
    );
    let o = p.ejecuta("main.jb", &[]);
    assert!(!o.status.success());
    let e = err(&o);
    assert!(e.contains("--> lib/m.jb:1:"), "{e}");
    assert!(!e.contains("./"), "{e}");
}
