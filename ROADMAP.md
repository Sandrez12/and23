# Camino a And23 1.0

Plan para llegar a una versión estable con la que se puedan hacer proyectos. Cada paso es pequeño, tiene un criterio de
terminado que se puede comprobar y termina en un commit. Tamaños: **S** = menos de una sesión, **M** = una sesión,
**L** = varias. Se marca `[x]` al terminar cada paso.

## Definición de 1.0

1. Las decisiones de la Fase 1 están tomadas y escritas; cada hueco del lenguaje está resuelto o documentado como límite.
2. El fuzzing no encuentra fallos, la suite pasa en CI en Linux, macOS y Windows, y los binarios de la release se probaron
   en máquinas reales de los tres sistemas.
3. Hay una referencia del lenguaje y de la biblioteca, y al menos un programa real (no un ejemplo) escrito con And23.

Orden de las versiones: `0.4` (fases 0 a 2) → `0.9` (fase 3 y 4) → `1.0.0-rc.1` (solo correcciones) → `1.0.0`.

---

## Fase 0 — Robustez barata (empezar aquí)

Lo que más protege a un usuario y menos cuesta.

- [x] **0.1 Un `Fallo` de tarea que nadie espera ya no se pierde.** (M) Hoy se descarta sin aviso. Decidir el comportamiento
  (propuesta: se imprime al terminar el programa con su traza y el código de salida es distinto de 0) y cubrirlo con tests.
  *Terminado cuando:* un programa con una tarea que falla sin esperarse termina con error visible; test nuevo.
- [x] **0.2 Fuzzing del lexer y el parser.** (M) `cargo-fuzz` o un generador propio con `proptest` sobre `and23-syntax`.
  *Terminado cuando:* 1 millón de entradas aleatorias o mutadas del corpus (`examples`, `mn/casos*`) no producen pánico ni
  cuelgue; los fallos hallados quedan como tests.
- [x] **0.3 Fuzzing del verificador, el compilador y la VM.** (M) Mismo método con programas mutados que sí parsean.
  *Terminado cuando:* sin pánicos del verificador ni del compilador, y la VM solo falla con `Diagnostic`, nunca con pánico.
- [x] **0.4 `--gc-stress` sobre todo el corpus en CI.** (S) Ejecutar los ejemplos y los casos de ejecución con GC en cada punto
  seguro. *Terminado cuando:* un job de CI lo hace y pasa.
- [x] **0.5 Línea de referencia de rendimiento.** (S) Un script que mida compilar `mn/compilador.jb`, un bucle numérico, una
  carga de `Lista`/`Mapa` y una de tareas, y guarde los tiempos en `docs/rendimiento.md`. *Terminado cuando:* existe y se puede
  repetir con un comando.

## Fase 1 — Decisiones (sin código, solo escritas en `DESIGN.md`)

Se deciden **antes** de tocar el lenguaje, porque cada una condiciona la Fase 2.

- [x] **1.1 Política de compatibilidad.** (S) Qué se promete en 1.x (sintaxis, tipos, biblioteca) y qué no (formato del
  bytecode y de los volcados: internos). Cómo se marca algo obsoleto y cuántas versiones vive.
- [x] **1.2 Palabras reservadas definitivas.** (S) Lista final (`kind`, `rescue`, `enum`, `match`, `show`, `thing`...). Decidir
  si alguna se renombra *ahora*; después ya no.
- [x] **1.3 Un veredicto por cada límite conocido.** (M) Tabla con tres columnas: **hacer antes de 1.0**, **documentar como
  límite**, **posponer a 1.x**. Propuesta inicial en la Fase 2.
- [x] **1.4 Qué hacer con `--tree`.** (S) Retirarlo o mantenerlo como referencia, sin promesa de compatibilidad.
- [x] **1.5 Papel de Rust y de Mn.** (S) Decidir si Rust sigue siendo el compilador oficial y Mn la prueba de autocompilación
  (propuesta, menos riesgo), o si Mn pasa a oficial. Cambia lo que se promete en la release.

## Fase 2 — Cerrar huecos del lenguaje (reparto decidido en 1.3, ver `DESIGN.md`)

**Hacer antes de 1.0** (los que un usuario encuentra a la primera semana):

- [x] **2.1 Lambdas que se llaman a sí mismas dentro de una función.** (M)
- [x] **2.2 `Mapa` con orden de inserción.** (M) Hoy el orden al recorrer no es predecible.
- [x] **2.3 Formato de decimales** (`formato(x, 2)` o equivalente). (S)
- [x] **2.4 `ordena` en O(n log n).** (S) Hoy es O(n²).
- [x] **2.5 `!!` y `?:` con `return`/`fail` a la derecha.** (M) El patrón más natural para opcionales.
- [x] **2.6 `use` en el REPL.** (S)
- [x] **2.7 Pasar métodos y constructores como valores.** (M) Hoy hay que envolverlos en una lambda.
- [x] **2.8 Retirar el intérprete de árbol** (`--tree` y el crate `and23-eval`). (M) Decidido en 1.4: los tests de `diferencial.rs` pasan a
  salidas esperadas escritas a mano.
- [x] **2.9 `if`/`else` como último valor de una lambda.** (M) Hoy `{ n -> if n <= 1 { 1 } else { 2 } }` da «la lambda debe devolver un valor de tipo Int»
  y hay que escribir `return` en cada rama. Descubierto al hacer el 2.1; la lambda tiene que aceptar un `if` con `else` (y un `match`) como su valor final.

**Documentar como límite de 1.0, posponer a 1.x** (si no hay razón para cambiarlo):
herencia múltiple de clases, métodos por defecto en interfaces, cotas múltiples, varianza y comodines, guardas y
desestructuración en `match`, reexportar o importar nombres sueltos, `it`, `::nombre`, lambdas genéricas, expulsión por
tiempo de tareas, Unicode avanzado, operaciones de archivos (copiar, renombrar, borrar carpetas).

Cada paso de esta fase: cambio en Rust **y** en `mn/` (el verificador y el compilador de Mn tienen que seguir idénticos), test
nuevo, `DESIGN.md` y `CHANGELOG.md`, y el punto fijo de la etapa 5 sigue pasando.

## Fase 3 — Documentación

- [x] **3.1 Referencia del lenguaje.** (L) `docs/lenguaje.md`: gramática, tipos, reglas del verificador, errores, tareas.
  *Terminado cuando:* cada construcción de `examples/` está explicada y cada mensaje de error tiene su causa.
- [x] **3.2 Referencia de la biblioteca.** (M) `docs/biblioteca.md`: firma y ejemplo de cada función y clase del preludio y de las
  nativas. Un test comprueba que ninguna nativa de `natives.rs` queda sin documentar.
- [x] **3.3 Tutorial al día.** (M) Pasar `docs/tutorial.md` por todas las novedades (`enum`, `match`, `listaDe`, `Dupla`...) y que
  alguien ajeno lo siga desde cero.
- [x] **3.4 Guía de errores comunes y de migración desde 0.x.** (S)

## Fase 4 — Programas reales (la prueba que más descubre)

- [x] **4.1 Primer proyecto: una herramienta de línea de comandos** (por ejemplo, un buscador de texto o un conversor de datos),
  con pruebas propias (`prueba`). (L)
- [x] **4.2 Segundo proyecto distinto** (por ejemplo, un servidor de tareas con canales o un intérprete pequeño). (L)
- [x] **4.3 Registrar cada fricción** en `docs/fricciones.md` y convertir las que importen en pasos de la Fase 2.

## Fase 5 — Distribución

- [x] **5.1 Probar los binarios de Linux y macOS de la release en máquinas reales** (o en una VM/contenedor). (M)
  *Hecho con los ejecutores de GitHub Actions (Linux, macOS y Windows):* el CI corre toda la suite en los tres en cada subida, y la release prueba el binario de cada
  sistema (`--version`, ejemplos, los verificadores, los proyectos y el punto fijo) antes de empaquetarlo. No se probó en máquinas físicas propias.
- [x] **5.2 Sumas de verificación** (SHA-256) junto a cada binario, y script de instalación. (S)
- [x] **5.3 Revisión de metadatos:** `LICENSE`, README, descripciones y versión de cada crate. (S)
- [~] **5.4 Publicar la extensión de VS Code** en el marketplace. (M)
  *Parcial:* la release empaqueta el `.vsix` y lo publica en el Marketplace si el repositorio tiene el secreto `VSCE_PAT` (una cuenta de editor y un token que solo el dueño puede crear).

## Fase 6 — Candidata y lanzamiento

- [x] **6.1 `0.9.0`.** Congelar funciones: desde aquí solo correcciones y documentación. (S)
- [x] **6.2 `1.0.0-rc.1`.** Un periodo de uso real (los proyectos de la Fase 4 y quien quiera probarlo) sin fallos graves
  abiertos. (M)
- [ ] **6.3 `1.0.0`.** Etiqueta, release, CHANGELOG y texto de anuncio. (S)

## Después de 1.0 (no bloquea)

Servidor de lenguaje (autocompletado, ir a la definición), gestor de paquetes y manifiesto de proyecto, errores con línea y
columna también al ejecutar con `--carga`, optimizaciones de la VM (caché en línea de métodos), una segunda herramienta de
Mn (formateador escrito en And23).

---

## Orden recomendado para empezar

1. **0.1** (`Fallo` perdido) y **0.2** (fuzzing del lexer y el parser): poco trabajo, mucho riesgo cubierto.
2. **1.1 a 1.5** (decisiones): una sesión corta y deja todo lo demás claro.
3. **0.3 a 0.5**, y luego la Fase 2 en el orden que salga de 1.3.
