# And23: decisiones de diseño

Lenguaje estático orientado a objetos de nivel similar a Java, con compilador y VM de bytecode escritos en Rust.
Meta: aprendizaje y proyecto serio a largo plazo; crecer por hitos hasta nivel Java y, al final, auto-compilarse.

## Decisiones tomadas

| Tema | Decisión |
|---|---|
| Nombre | **And23** |
| Extensión | **`.jb`** |
| Implementación | Rust |
| Ejecución | VM de pila con bytecode propio (estilo JVM) |
| Tipado | Estático, orientado a objetos |
| Identificadores | Unicode (`dueño`, `ñandú`) |
| Genéricos | Con erasure (un solo bytecode, casts insertados por el compilador) |
| GC | Mark-sweep propio detrás de una interfaz de heap (migrable a `gc-arena`) |
| Parser | A mano: descenso recursivo + Pratt |
| Meta | Aprendizaje + serio a largo plazo; self-hosting al final |

## Sintaxis (borrador aprobado)

```
pack demo.zoo                      // paquete

kind Habla {                       // interfaz
    fn sonido() -> Str
}

thing Animal(fix nombre: Str) {    // clase; el constructor va en la cabecera
    mut edad = 0                   // campo mutable
    fn cumple() { edad += 1 }
}

thing Perro(nombre: Str) : Animal(nombre), Habla {
    fn sonido() -> Str = "guau"    // cuerpo de una expresión con "="
}

fn main() {
    fix p = Perro("Rex")           // fix = inmutable, mut = mutable
    p.cumple()
    show("${p.nombre} dice ${p.sonido()}")

    fix dueño: Perro? = nil        // nil y T? en vez de null
    dueño?.cumple()                // acceso seguro

    attempt { riesgo() }           // try/catch
    rescue e: Fallo { show(e.msg) }
}
```

Palabras propias: `thing` (clase), `kind` (interfaz), `fix`/`mut` (inmutable/mutable), `show` (imprimir),
`attempt`/`rescue` (excepciones), `nil`. Sin `;` obligatorios.

Principios heredados de Kotlin y de lo que Java lamenta: null-safety (`T?`), inmutable por defecto,
igualdad estructural, strings UTF-8, sin boxing visible al usuario.

## Arquitectura

Workspace de Rust en `crates/`:

- `and23-syntax`: lexer, AST, parser y diagnósticos (spans con línea y columna).
- `and23-types`: verificador de tipos (`Checker`); corre entre el parser y el compilador.
- `and23-bytecode`: `Op`, `Module`/`Function`, compilador AST → bytecode y desensamblador.
- `and23-vm`: VM de pila (`Vm`) y `Session`, que une compilador y VM conservando el estado del REPL.
- `and23-eval`: intérprete de árbol (M0/M1a). Ya no es el camino de ejecución; se conserva como oráculo en los tests diferenciales y para `--tree`.
- `and23-cli`: binario `and23` (ejecuta archivos `.jb` y ofrece un REPL).


## Hitos

| Hito | Contenido | Criterio de éxito |
|---|---|---|
| M0 | Lexer, parser y expresiones aritméticas | `1 + 2 * 3` da 7, con tests |
| M1 | Variables, `if`/`while`, funciones, bytecode y VM de pila | fib(30) y factorial |
| M2 | Tipos estáticos y errores con línea y columna | Un programa mal tipado se rechaza con mensaje claro |
| M3 | Strings, arrays y GC mark-sweep | 10M de asignaciones sin agotar memoria; pasa en modo stress |
| M4 | Clases, campos, métodos, constructores | Programa OO de ~200 líneas (`examples/clases.jb`, 222 líneas) |
| M5 | Herencia, interfaces, vtables, `instanceof` | Ejemplo polimórfico (`examples/polimorfismo.jb`) |
| M6 | Excepciones y null-safety | `attempt`/`rescue`; el compilador rechaza deref de `T?` |
| M7 | Genéricos con erasure | `List<T>` y `Map<K,V>` propios |
| M8 | Closures y lambdas | `lista.map { ... }` |
| M9 | Módulos y paquetes | Proyecto de 3+ archivos |
| M10 | Stdlib inicial (IO, strings, math) | Leer y procesar un archivo |
| M11 | Concurrencia | Productor/consumidor |
| M12 | Herramientas (REPL, desensamblador, benchmarks) | `and23 --disasm` |
| Mn | Auto-compilación | El compilador escrito en And23 se compila a sí mismo |

## Estado

- **M0** (hecho): lexer, parser Pratt y expresiones aritméticas. `examples/aritmetica.jb`.
- **M1a** (hecho): variables, booleanos, `if`/`while`, funciones y recursión sobre el intérprete de árbol.
  `examples/funciones.jb` y `examples/control.jb`. Referencia para M1b: `fib(30)` = 832040 en ~1,7 s (release).
- **M1b** (hecho): compilador a bytecode y VM de pila. `fib(30)` = 832040 en ~0,17 s (release) contra ~1,6 s del
  árbol (≈9×). Tests diferenciales: la VM da la misma salida y los mismos errores que el intérprete de árbol.

- **M2** (hecho): tipos estáticos (`Int`, `Float`, `Bool`, `Unit`) con errores de línea y columna; ver "Tipos (M2)".
- **M3** (hecho): cadenas UTF-8, interpolación, arreglos y GC mark-sweep propio; ver "Cadenas, arreglos y GC (M3)".
- **M4** (hecho): clases, campos, métodos y constructores; ver "Clases (M4)".
- **M5** (hecho): herencia, interfaces, despacho virtual, `is`/`as`/`super`; ver "Herencia e interfaces (M5)".

### Diseño de la VM (M1b)

- Instrucciones `Op` (enum `Copy`, destinos de salto absolutos) y una tabla de `Span` por instrucción para los errores.
- Los locales son las primeras posiciones del marco en la pila; cada bloque saca los suyos al salir (`Pop`/`PopN`),
  y `break`/`continue` sacan los que declaró el cuerpo del bucle antes de saltar.
- Las `fix`/`mut` del nivel superior son globales (ranuras); las funciones y globales del archivo se registran antes de
  compilar, así que una función puede usar una global declarada más abajo (falla al ejecutar si aún no tiene valor).
- Las llamadas no usan la pila nativa de Rust: un marco por llamada en un `Vec`, con límite de 10 000.
- Variable no definida, asignación a `fix`, aridad incorrecta y función desconocida se detectan **al compilar**
  (antes de ejecutar nada); los errores de tipo, desbordamiento y división entre cero, al ejecutar.
- Si la compilación falla, el compilador queda intacto (REPL). `and23 --disasm archivo.jb` imprime el bytecode.

### Semántica fijada en M1a

- Valores: `Int` (i64 con desbordamiento verificado), `Float`, `Bool` y `Unit`. Los strings llegan en M3.
- `fix` es inmutable y `mut` mutable. Los parámetros de función son inmutables.
- `Int / Int` es división entera; si cualquier lado es `Float`, la operación es decimal. `1 == 1.0` es verdadero.
- `&&` y `||` hacen cortocircuito. Las condiciones de `if` y `while` deben ser `Bool`.
- `if` y `while` son sentencias (no expresiones). `else` va en la misma línea que la `}` anterior.
- Las funciones se declaran solo al nivel superior, pueden usarse antes de su definición y ven las variables
  globales, no las locales del llamador. Si el archivo define `main()`, se llama tras el código del nivel superior.
- Las anotaciones de tipo (`n: Int`, `-> Int`) son obligatorias en los parámetros y desde M2 se verifican.
- Recursión limitada a 10 000 llamadas anidadas: más allá es un error con posición, no un cierre del proceso.
- Desde M1b esos errores de nombres y aridad se informan al compilar, no al ejecutar.
- Palabras clave: `fix mut fn if else while return break continue true false show` (desde M4 `thing this` y desde M5 `kind super is as`).

### Tipos (M2)

- Tipos: `Int`, `Float`, `Bool`, `Unit`. Un tipo desconocido es un error. Un `fix`/`mut` no puede guardar `Unit`.
- Variables: el tipo sale de la anotación o, si falta, del valor inicial; una anotación distinta del valor es un error.
- **Sin conversión implícita** `Int` → `Float` al asignar, pasar argumentos o devolver. En aritmética y comparaciones,
  mezclar `Int` y `Float` es válido y da `Float`/`Bool`. Con `==` solo se pueden comparar tipos iguales (o dos numéricos).
- Funciones: sin `->` y con cuerpo `{ }` devuelven `Unit`; con cuerpo `= expr` el tipo se infiere. Una función
  con `= expr` que se llama a sí misma antes de conocer su tipo debe anotar `-> Tipo`. Una función con tipo de retorno
  no `Unit` debe devolver en todos los caminos (`return` final o `if`/`else` que devuelven los dos).
- Las funciones se registran antes (pueden usarse antes de definirse); el nivel superior se verifica en orden,
  así que una global debe declararse antes de usarse desde el nivel superior. Los cuerpos de función se verifican
  después y ven todas las globales del archivo.
- El verificador devuelve un solo error, el primero que encuentra: primero el nivel superior y después los cuerpos
  de función, no necesariamente en orden de líneas.
- Si falla, no cambia su estado (REPL).

### Cadenas, arreglos y GC (M3)

- **Tipos nuevos:** `Str` y `[T]` (arreglo homogéneo; `TypeRef` lleva `args`, así `[Int]` es `Array<Int>` y deja el camino a los genéricos).
  `Unit` no puede ser elemento ni interpolarse.
- **Cadenas:** inmutables y UTF-8. `len` cuenta caracteres Unicode. `+` solo une `Str` con `Str`; para mezclar con otros valores se usa
  `"${...}"`. Comparan por contenido (`==`) y lexicográficamente (`<`). `==` sobre arreglos no existe todavía.
- **Interpolación:** el lexer lee `${expr}` recursivamente (admite cadenas y llaves anidadas) y el parser analiza esos tokens con las
  posiciones del fuente original, así los errores dentro de una interpolación señalan el sitio exacto.
- **Arreglos:** son referencias (asignar o pasar uno no lo copia); `fix` impide reasignar la variable, no cambiar los elementos.
  `[]` no tiene tipo propio: toma el esperado (`fix a: [Int] = []`, argumentos, retornos) o es un error. Un índice fuera de rango es un
  error de ejecución con la posición del acceso.
- **Funciones integradas:** `len` y `push` (no se pueden redefinir). Se compilan a instrucciones (`Len`, `Push`), no a llamadas.
- **Instrucciones nuevas:** `Str`, `Interp(n)`, `Array(n)`, `Index`, `SetIndex`, `Dup2` (para `a[i] += v`), `Len`, `Push`.
- **Heap y GC:** objetos (`Str`, `Array`) en un `Vec` de ranuras referenciadas por índice (`ObjRef`), con lista de ranuras libres.
  Mark-sweep con pila de grises explícita (los arreglos anidados no recursan en Rust). Raíces: pila de la VM, globales y las cadenas
  constantes. **Solo se recolecta en puntos seguros**: al inicio de las instrucciones que reservan memoria, cuando los operandos aún
  están en la pila; ningún valor "en la mano" de Rust queda sin ver. El umbral es el doble de lo vivo tras cada recolección
  (mínimo 1 MB). `Heap` expone una interfaz pequeña (`alloc_*`, `collect`, `get`), lo que permite cambiarlo por otro recolector
  (p. ej. `gc-arena`) sin tocar el compilador.
- **Modo stress** (`--gc-stress`, `Session::with_gc_stress`): recolecta en cada punto seguro. Los tests ejecutan cada programa en modo
  normal y en modo stress y exigen el mismo resultado.
- **Criterio de éxito:** 10 millones de asignaciones de cadenas con el heap acotado (el test exige menos de 200 000 ranuras) y los tests
  en modo stress. Medido: 10 millones de iteraciones con una cadena nueva cada una tardan ~1,9 s (release).
- **Intérprete de árbol:** queda congelado en M1a; rechaza cadenas, arreglos y clases con un error claro.

### Clases (M4)

```
thing Cuenta(fix titular: Str, saldoInicial: Int) {   // cabecera = constructor
    mut saldo = saldoInicial                           // campo del cuerpo
    fn deposita(n: Int) { saldo += n }                 // método
}
fix c = Cuenta("Ana", 10)
c.deposita(5)
```

- **Cabecera:** `fix`/`mut nombre: T` crea un campo con ese nombre; sin `fix`/`mut` el parámetro solo existe para los
  inicializadores del cuerpo. Orden de los campos en el objeto: los de la cabecera y luego los del cuerpo.
- **Cuerpo:** campos (`fix`/`mut nombre [: T] = expr`) y métodos (`fn`), en cualquier orden. Cada inicializador ve los
  parámetros del constructor y los campos anteriores (también llamar funciones o crear otros objetos), pero no `this`.
  El cuerpo puede omitirse (`thing P(fix x: Int)`).
- **Nombres:** dentro de un método se busca primero el local o parámetro, luego el campo de `this` y al final la global.
  `this.campo` siempre es el campo. `metodo()` sin receptor llama al método de la propia clase (antes que a una función
  global del mismo nombre). Los campos y los métodos comparten espacio: no pueden llamarse igual.
- **Clases y tipos:** una clase es un tipo (`fix p: Punto`, `[Punto]`, parámetros, retornos). Se pueden usar antes de su
  declaración y referirse entre sí; el nombre no puede coincidir con una función, un tipo integrado (`Int`...) ni otra clase.
- **Objetos:** viven en el heap como `Instance { class, fields }`, son referencias (compartirlos no los copia) y el GC
  recorre sus campos; los ciclos (`a.hijos` que contiene a `a`) se recolectan y `show` los corta con `...`. `fix` en un campo
  prohíbe reasignarlo (incluso desde los métodos); `fix` en una variable solo prohíbe cambiar la variable.
  `==` entre objetos no existe todavía (la igualdad estructural llegará con los tipos de datos).
  `show` imprime `Clase(campo=valor, ...)`; las cadenas dentro van entre comillas.
- **Compilación:** un método es una función normal llamada `Clase.metodo` con `this` como local 0 y los parámetros después;
  `obj.m(x)` lleva el receptor como primer argumento (en M4 era una llamada estática; desde M5 es virtual, ver abajo).
  Desde M5 el constructor es una función `Clase` que reserva el objeto (`Alloc`, campos en `()`) y llama a `Clase.<init>`,
  que evalúa los inicializadores como locales y guarda los campos con `SetField`. Instrucciones: `GetField(i)`, `SetField(i)`, `Dup`, `Alloc`.
- **Qué decide el verificador:** el compilador no tiene tipos, así que `Checker::check` devuelve `Resolutions`: para cada
  acceso (por su posición en el fuente) el índice de campo, y para cada llamada la clave `"Clase.metodo"`. Es la única
  fuente de verdad de la resolución de nombres: el compilador no la repite.
- **Límites conocidos de M4:** sin herencia ni interfaces (hechas en M5), sin `nil` (los campos siempre se inicializan: M6),
  sin visibilidad (todo es público) y sin sobrecarga. El tipo de un campo sin anotar se infiere la primera vez que se
  necesita (como el retorno de una función `= expr`); si depende de sí mismo hay que anotarlo.

### Herencia e interfaces (M5)

```
kind Habla { fn sonido() -> Str }                       // interfaz: solo firmas

thing Animal(fix nombre: Str) {
    fn presenta() -> Str = "${nombre} dice ${this.sonido()}"
    fn sonido() -> Str = "..."
}
thing Perro(nombre: Str) : Animal(nombre), Habla {      // : Padre(args), Interfaz, ...
    fn sonido() -> Str = "guau"                         // sobrescribe: misma firma
}
fix a: Animal = Perro("Rex")
show(a.presenta())                                      // Rex dice guau
show(a is Perro)                                        // true
fix p = a as Perro                                      // comprobado al ejecutar
```

- **Sintaxis:** tras `:` va a lo sumo un padre con paréntesis (`Animal(nombre)`) y cualquier cantidad de interfaces sin
  ellos. `kind` solo declara firmas (sin cuerpo); no hereda de otras interfaces ni trae métodos por defecto. No hay
  `override`: sobrescribir es definir un método con el mismo nombre, y el verificador exige la misma firma exacta
  (parámetros y retorno; sin covarianza). Las clases son heredables siempre (no hay `final`/`open`).
- **Campos:** los del padre van primero (prefijo común), después los de la cabecera y del cuerpo del hijo. Un campo del hijo no
  puede repetir uno heredado, ni un método llamarse como un campo. Se ven por su nombre en los métodos del hijo. Los
  inicializadores no tienen `this`, así que no ven los campos heredados, solo los parámetros del constructor.
- **Construcción:** `Hijo.<init>` llama primero a `Padre.<init>` con los argumentos de `: Padre(...)` (expresiones sobre los
  parámetros del hijo) y después rellena sus propios campos: el orden es padre, cabecera, cuerpo; probado con trazas.
- **Subtipos:** un tipo clase o interfaz acepta cualquier subclase o implementación donde se espera (asignación, argumentos,
  retorno, `push`, y los elementos de un literal `[...]` si el arreglo anotado es de objetos). Los arreglos son invariantes:
  un `[Perro]` no es un `[Animal]`. `==` entre objetos sigue sin existir.
- **Despacho virtual:** `obj.m()`, `this.m()` y `m()` sin receptor compilan a `CallMethod { método, argc }`. Cada clase del módulo
  (`ClassDef`) lleva una tabla `vtable: id de método -> función` ya aplanada (heredados incluidos, los sobrescritos reemplazan),
  que el compilador arma con el padre antes que el hijo. Los ids de método son por nombre (`Module::methods`); no hay
  sobrecarga, así que el nombre basta. `super.m()` es una `Call` estática a la implementación del padre, que el verificador
  resuelve por su posición como `Resolutions::methods`; las llamadas virtuales van en `Resolutions::virtuals`.
- **Interfaces en la VM:** son entradas de `Module::classes` sin campos ni tabla. `ClassDef::supers` lista el propio índice, el de
  los ancestros y el de las interfaces que cumple, así `IsInstance(t)` y `Cast(t)` son una búsqueda en ese vector. Un valor
  tipado como interfaz se llama con el mismo `CallMethod`: la tabla de la clase real decide.
- **Comprobaciones al declarar:** herencia circular, padre inexistente, heredar de una interfaz o implementar una clase (con el
  mensaje que dice cómo escribirlo), dos padres, interfaz repetida, argumentos del padre (cantidad y tipos), firma de cada
  sobrescritura y que la clase implemente todos los métodos de sus interfaces con la firma exacta (propios o heredados).
  `x is T` / `x as T` exigen objetos y rechazan dos clases sin parentesco (con interfaces de por medio sí pueden coincidir).
- **Costo:** la búsqueda en la tabla es un `HashMap` por llamada; basta por ahora (un caché en línea quedaría para un hito de
  rendimiento). `super` no existe fuera de métodos de una clase con padre.
- **Límites conocidos de M5:** una sola clase padre (sin herencia múltiple de clases), interfaces sin herencia ni métodos por
  defecto, sin clases abstractas, sin visibilidad (todo público), sin `==` entre objetos, sin covarianza en sobrescrituras.

## Fuentes de referencia

- Crafting Interpreters (VM de bytecode, GC): https://craftinginterpreters.com
- Writing a Compiler in Go (AST, compilador, VM): https://compilerbook.com
- Especificación de la JVM (class file, despacho): https://docs.oracle.com/javase/specs/jvms/se21/html/
- Dora VM (lenguaje con VM en Rust, MIT): https://github.com/dinfuehr/dora
- Wren (VM pequeña con clases): https://wren.io

Los datos de crates (versiones, licencias) se verificaron en crates.io el 2026-11-05:
`gc-arena` 0.7.0 (MIT), `logos` 0.16.1, `ariadne` 0.6.0. Los libros y la especificación de la JVM no se verificaron.
