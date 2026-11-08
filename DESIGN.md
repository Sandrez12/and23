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
| M6 | Excepciones y null-safety | `attempt`/`rescue`; el compilador rechaza deref de `T?` (`examples/excepciones.jb`) |
| M7 | Genéricos con erasure | `List<T>` y `Map<K,V>` propios (`Lista<T>` y `Mapa<K, V>` en `prelude.jb`, escritas en And23) |
| M8 | Closures y lambdas | `lista.map { ... }` (`examples/lambdas.jb`; `Lista` con `map`/`filter`/`reduce`/`ordena`) |
| M9 | Módulos y paquetes | Proyecto de 3+ archivos (`examples/proyecto/`; `use`, `open`, alias) |
| M10 | Stdlib inicial (IO, strings, math) | Leer y procesar un archivo (`examples/palabras.jb`, `examples/notas.jb`) |
| M11 | Concurrencia | Productor/consumidor (`examples/tareas.jb`) |
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
- **M6** (hecho): excepciones (`attempt`/`rescue`/`fail`) y opcionales (`T?`, `nil`, `?.`, `?:`, estrechamiento); ver "Excepciones y opcionales (M6)".
- **M7** (hecho): genéricos con erasure (clases, interfaces y funciones), cotas, `Lista<T>` y `Mapa<K, V>` en el preludio; ver "Genéricos (M7)".
- **M8** (hecho): lambdas, closures (captura por valor y por referencia), tipos de función, métodos genéricos y `Lista` con `map`/`filter`/...; ver "Closures y lambdas (M8)".
- **M9** (hecho): proyectos de varios archivos con `use`/`open` y alias; ver "Módulos (M9)".
- **M10** (hecho): biblioteca estándar inicial (texto, números, archivos, entrada estándar); ver "Biblioteca estándar (M10)".
- **M11** (hecho): tareas cooperativas y canales; ver "Concurrencia (M11)".

### Diseño de la VM (M1b)

- Instrucciones `Op` (enum `Copy`, destinos de salto absolutos) y una tabla de `Span` por instrucción para los errores.
- Los locales son las primeras posiciones del marco en la pila; cada bloque saca los suyos al salir (`Pop`/`PopN`),
  y `break`/`continue` sacan los que declaró el cuerpo del bucle antes de saltar.
- Las `fix`/`mut` del nivel superior son globales (ranuras); las funciones y globales del archivo se registran antes de
  compilar, así que una función puede usar una global declarada más abajo (falla al ejecutar si aún no tiene valor).
- Las llamadas no usan la pila nativa de Rust: un marco por llamada en un `Vec`, con límite de 100 000 (`--max-depth N` lo cambia; el intérprete de árbol sigue en 10 000).
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
- Recursión limitada a 100 000 llamadas anidadas (configurable con `--max-depth`): más allá es un error con posición, no un cierre del proceso.
- Desde M1b esos errores de nombres y aridad se informan al compilar, no al ejecutar.
- Palabras clave: `fix mut fn if else while return break continue true false show` (desde M4 `thing this`, desde M5 `kind super is as` y desde M6 `nil attempt rescue fail`).

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
  `"${...}"`. Comparan por contenido (`==`) y lexicográficamente (`<`). `==` sobre arreglos compara por referencia (desde M7).
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
  `==` entre objetos compara por referencia (desde M7; antes no existía).
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
  un `[Perro]` no es un `[Animal]`. `==` entre objetos compara por referencia (desde M7).
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
  defecto, sin clases abstractas, sin visibilidad (todo público) y sin covarianza en sobrescrituras.

### Excepciones y opcionales (M6)

```
thing MiError(fix codigo: Int) : Fallo("falló con ${codigo}")   // hereda de Fallo (trae `msg`)
attempt { fail MiError(3) }
rescue e: MiError { show(e.msg) }

fix p: Perro? = nil
show(p?.nombre ?: "nadie")
if p != nil { show(p.nombre) }
```

**Excepciones**

- **`Fallo`:** el preludio (`and23_syntax::PRELUDE`, `thing Fallo(fix msg: Str)`) lo ejecuta toda `Session` antes del programa; es una clase
  normal, así que `msg` es el primer campo de todo error (la VM lo usa para el mensaje de una excepción sin capturar) y los errores
  propios son subclases con sus campos. `fail` solo acepta objetos que hereden de `Fallo`; `rescue` solo clases que lo hagan.
- **Sintaxis:** `attempt { ... }` seguido de uno o más `rescue nombre: Tipo { ... }`, que pueden ir en la línea siguiente. Sin
  `finally`. `fail` y `attempt` valen en cualquier sitio; `fail` cuenta como salida para el chequeo de "devuelve en todos los
  caminos", y un `attempt` cuenta si su cuerpo y todos sus `rescue` devuelven.
- **VM:** `Try(destino)` apila un manejador con el número de marcos, la altura de la pila, la función y la base; `EndTry` lo quita;
  `Fail` saca el objeto y desenrolla: descarta los marcos nuevos, recorta la pila a su altura y salta al destino con el objeto en la
  cima. Ahí el compilador emite, por cada `rescue`: `Dup`, `IsInstance(tipo)`, salto si no coincide, el objeto pasa a ser el local
  de la cláusula y el cuerpo corre; si ninguna coincide, un `Fail` más la relanza hacia fuera. Sin manejador es un error de ejecución
  (`excepción sin capturar `Tipo`: mensaje`) con la posición del `fail`.
- **Manejadores que no deben quedar colgados:** `Return` descarta los manejadores de su función; `break`/`continue` emiten un
  `EndTry` por cada `attempt` que cierran (el compilador lleva la cuenta por bucle); al relanzar el manejador ya se quitó.
- **GC:** los manejadores no guardan valores; el objeto lanzado viaja en la pila, que ya es raíz.
- **Límites:** los errores propios de la VM (división entre cero, índice fuera de rango, desbordamiento de entero, `as` inválido, pila
  agotada) no son excepciones y no se capturan; sin `finally`; en el REPL el `rescue` debe ir en la misma línea que el `attempt`.

**Opcionales**

- **Tipos:** `T?` es `Type::Nullable`; `nil` tiene el tipo interno `Nil`, que solo existe mientras se verifica (no se puede inferir
  una variable, arreglo o campo de `nil`: hay que anotar). `T??` y `Unit?` no existen. En ejecución `nil` es un valor más
  (`Value::Nil`), así que también hay `Int?`, `[Int?]` y `Str?`. Un `T` se acepta donde se pide `T?`, no al revés.
- **Operadores:** `a?.campo` y `a?.metodo(args)` (`Op::JumpIfNil`: si `a` es nil el resultado es nil y no se evalúan los argumentos;
  el resultado se vuelve opcional, y una llamada que devuelve `Unit` sigue siendo `Unit`), `a ?: b` (`Op::JumpIfNotNil`, solo
  evalúa `b` si hace falta; es el operador de menor precedencia y asocia a la izquierda). `x == nil` y `x != nil` solo se aceptan
  sobre un opcional; comparar un valor que nunca es nil con `nil` es un error. `==` entre dos opcionales del mismo tipo vale desde M7 (`nil` es igual a `nil` y distinto de cualquier valor).
- **Estrechamiento** (solo en el verificador; no genera código): `facts(cond)` extrae qué variables prueba la condición cuando es
  verdadera y cuando es falsa (`!=`/`==` con `nil`, `&&`, `||`, `!`). Se aplica al bloque `then`/`else`, al cuerpo de un `while`,
  al lado derecho de `&&`/`||` y, si un bloque sale (`return`, `fail`, `break`, `continue`), al resto del bloque que lo contiene.
  Se implementa con una entrada que tapa a la variable en un ámbito propio y recuerda su tipo declarado.
- **Qué se estrecha:** locales y parámetros, y globales `fix` (en el nivel superior se guardan aparte, para no filtrar el estrechamiento a
  las funciones, que se verifican después). No se estrechan los campos. Una global `mut` se estrecha solo en el código del nivel superior y solo
  si ninguna función, método ni lambda del programa la asigna (ver "Estrechamiento de globales `mut`"); si no, otro código podría cambiarla
  entre la comprobación y el uso. Un `mut` local se estrecha, pero una asignación que pueda dar `nil` lo vuelve a ensanchar, y si se asigna
  dentro de un bucle anidado en el bloque no se estrecha (el bucle repetiría un uso anterior a la asignación).
- **Límites:** sin `!!` (desreferencia forzada), sin asignación segura (`a?.x = 1`), sin `?:` con `fail`/`return` a la derecha.

### Genéricos (M7)

```
thing Caja<T>(fix valor: T) { fn dame() -> T = valor }
thing Pila<T>() : Lista<T>() { fn empuja(x: T) { agrega(x) } }          // herencia entre genéricos
kind Ordenable<T> { fn antesDe(otro: T) -> Bool }
fn minimo<T: Ordenable<T>>(a: T, b: T) -> T { ... }                   // con cota
fix c = Caja(5)                                                         // Caja<Int>, deducido
fix l: Lista<Str> = Lista()                                             // deducido del tipo esperado
fix m = Mapa<Str, Int>()                                                // explícito
```

- **Erasure, de verdad:** el bytecode, la VM y el recolector no cambian: `Value` ya es uniforme (un `Int`, un `Float` o una referencia ocupan
  lo mismo), así que no hay encajonado ni casts insertados. Los genéricos existen solo en el verificador: un `Mapa<Int, Int>` guarda
  enteros directamente y `Caja<T>` ocupa lo mismo que `Caja<Str>`. El compilador solo ve nombres de clase (los argumentos de tipo se
  ignoran, salvo para resolver llamadas) y `is`/`as` comparan solo la clase.
- **Dónde hay parámetros de tipo:** clases (`thing`), interfaces (`kind`) y funciones del nivel superior. Los métodos usan los de su clase y no
  pueden declarar los suyos (todavía no hay lambdas que lo justifiquen). Cota opcional por parámetro (`T: Ordenable<T>`, una clase o
  interfaz, admite referirse a sí mismo).
- **Tipos nuevos en el verificador:** `Type::Obj(nombre, argumentos)` y `Type::Param(nombre)`. Se verifica **una sola vez** el cuerpo genérico con
  `T` opaco, no una copia por instanciación: dentro de él un `T` solo se puede asignar, comparar con `==`/`!=`, guardar, pasar, imprimir,
  interpolar y hashear; no admite aritmética, `<`, campos ni métodos, salvo los de su cota (un `T: Animal` ofrece sus campos y métodos
  y vale como `Animal`).
- **Instanciar:** `subst` sustituye parámetros por argumentos (y aplana `T?` cuando `T` ya es opcional: un `Mapa<Str, Int?>` da `Int?` y no
  `Int??`). Los campos, métodos y constructores se leen con los argumentos del receptor aplicados; la herencia los encadena
  (`Pila<Str>` → `Lista<Str>`), igual que las interfaces (`as_super` ve un tipo como su ancestro o interfaz con sus argumentos).
- **Inferencia de argumentos de tipo** (`infer`, en este orden): los explícitos; luego los que salen de unificar el tipo de cada
  parámetro con el del argumento (`nil` y `[]` no aportan); y, solo para constructores, el tipo que pide el contexto
  (`fix l: Lista<Int> = Lista()`, argumentos, retornos y campos). Si queda alguno sin deducir es un error que sugiere escribirlos
  (`Lista<Int>()`). Después se comprueban las cotas.
- **Invarianza:** `Caja<Perro>` no es `Caja<Animal>` ni `[Perro]` es `[Animal]`; una subclase sí es su tipo instanciado
  (`CajaP : Caja<Perro>` es `Caja<Perro>`). Las sobrescrituras y las implementaciones de interfaces genéricas se comparan con la firma
  ya instanciada.
- **Sintaxis:** los argumentos de tipo van entre `<>` tras un nombre de tipo (`Mapa<Str, [Int]>`, `Lista<Lista<Int>>`) y también al llamar
  (`Lista<Int>()`, `primero<Int>(xs)`): el parser lo intenta y, si no encaja con un `(` al final, vuelve atrás y lo lee como
  comparación (`a < b`). No hay token `>>`, así que los cierres anidados no necesitan espacios; sí hace falta un espacio en
  `Caja<Int> = x` (sin él se lee `>=`).
- **Igualdad:** para poder escribir un `Mapa` hecho en el lenguaje hizo falta `==` sobre un `T` cualquiera, así que desde M7 `==`/`!=` valen
  entre valores del mismo tipo (o subtipos): números por valor (`1 == 1.0`), cadenas por contenido, objetos y arreglos por referencia, `nil`
  contra opcionales. Es una decisión de M7 que sustituye a "no existe" de M3–M6.
- **Funciones integradas nuevas:** `pop(arreglo)` (quita y devuelve el último; error de ejecución si está vacío) y `hash(valor)` (un `Int` no negativo;
  cadenas por contenido, `1` y `1.0` coinciden, objetos por identidad). Son las dos piezas que faltaban para escribir `Lista` y `Mapa` en And23.
- **Preludio:** `crates/and23-syntax/prelude.jb` se incluye en `and23_syntax::PRELUDE` junto con `Fallo` y cada `Session` lo verifica y compila
  antes del programa (unos 30-40 ms de arranque). `Lista<T>` es un arreglo con operaciones; `Mapa<K, V>` es una tabla hash de cubetas
  (arreglos de `Entrada<K, V>`) que duplica sus cubetas al pasar de 2 entradas por cubeta. Nombres reservados por el preludio: `Fallo`,
  `Lista`, `Entrada`, `Mapa` y `_cubetasVacias`.
- **Límites conocidos de M7:** (los métodos genéricos propios llegaron en M8) sin cotas múltiples, sin genéricos en `rescue`, sin comodines ni varianza
  (`Lista<out T>`), sin tipos crudos (`Lista` a secas es un error, salvo en `is`), sin deducir los argumentos de tipo de un método a partir del
  tipo esperado, y `Mapa` es una tabla sin orden (los `claves()` salen en el orden de las cubetas).

### Closures y lambdas (M8)

- **Sintaxis:** `{ a, b -> cuerpo }`; los parámetros se anotan (`a: Int`) o se deducen del tipo esperado; `{ cuerpo }` no tiene parámetros. El
  cuerpo es una lista de sentencias cuya última expresión es el valor (con `Unit` esperado se descarta). `return` sale de la lambda. Una
  lambda final va fuera del paréntesis (`xs.map { ... }`, `f(a) { ... }`, `f { ... }`); en la condición de un `if`/`while` la `{` abre el
  cuerpo, así que ahí una lambda necesita paréntesis. Tipos de función: `(A, B) -> R` (el verificador los guarda como `Type::Fn`; en el AST es
  un `TypeRef` de nombre `Fn` con los parámetros y el resultado como argumentos, por eso `Fn` es un nombre reservado). `f(1)(2)` es
  `Expr::CallValue`, y `f(x)` donde `f` es una variable, parámetro o campo de tipo función se anota en `Resolutions.valuecalls`.
- **Inferencia:** una lambda se verifica contra el tipo esperado (`expr_as`). En una llamada genérica, primero se deducen los argumentos que no
  son lambdas, luego cada lambda con los parámetros ya conocidos (de ahí sale `R` en `map<R>`), y el cuerpo se verifica otra vez con los tipos
  completos. Sin tipo esperado (`fix f = { ... }`) los parámetros deben llevar anotación. Una función genérica no se puede usar como valor.
  Asignabilidad: los parámetros son contravariantes, el resultado covariante, y un `-> Unit` solo admite funciones que devuelven `Unit`.
- **Estrechamiento y lambdas:** una lambda puede ejecutarse más tarde, así que dentro de ella una variable `mut` de fuera vuelve a su tipo
  declarado (la `fix` conserva el estrechamiento), y las `mut` que la lambda asigna dejan de estar estrechadas al crearla.
- **Métodos genéricos:** `fn mapea<R>(f: (T) -> R) -> Caja<R>`. Los parámetros de tipo del método se suman a los de la clase (no pueden repetirse)
  y se deducen en cada llamada como en una función; las interfaces (`kind`) siguen sin métodos genéricos.
- **Ejecución:** una lambda es un objeto del heap (`Object::Closure{function, captures}`) y su función es una `Function` del módulo cuyo local 0
  es la propia lambda y cuyos parámetros siguen. `Closure{func, n}` saca `n` valores capturados y crea el objeto; `CallValue(argc)` llama al
  objeto que está bajo los argumentos; `GetCapture(i)` lee un valor capturado. No hay marcos nuevos ni cambios en la tabla de métodos.
- **Qué se captura:** el compilador resuelve cada nombre buscando en la función actual, luego en las que la envuelven (cada lambda del camino
  captura la variable) y al final en las globales, que se usan directamente sin capturar. `fix` y parámetros se copian al crear la lambda. Una
  variable `mut` capturada vive en una celda (`Object::Cell`; `NewCell`, `CellGet`, `CellSet`) para que la lambda y la función de fuera
  compartan cambios y la variable sobreviva a su función. Saber qué `mut` van en celda hace falta al declararlas, así que `build` compila cada
  función una vez, anota las capturadas (por número de orden de declaración) y, si hubo alguna, la compila de nuevo con ellas en celda.
  Las demás `mut` no pagan nada; el coste es compilar dos veces algunas funciones. `this` se captura como una variable más.
- **Funciones como valores:** `aplica(triple, 4)` crea una lambda sin capturas que llama a la función (un envoltorio `triple.<ref>`).
- **Recolector:** el marcado recorre los valores capturados y el contenido de las celdas; `--gc-stress` da la misma salida en todos los tests.
- **Verificación de lambdas:** una llamada genérica mira cada lambda dos veces (al deducir y al comprobar), y el cuerpo contiene a su vez
  llamadas con lambdas. Para que no costara 2^profundidad, el verificador recuerda el tipo de cada lambda ya verificada, con clave: su
  posición, los tipos esperados y los tipos de todas las variables a la vista. 40 niveles anidados verifican al instante.
- **Límites conocidos de M8:** una lambda no puede referirse a sí misma por su nombre dentro de una función (solo vale en el nivel superior y con el tipo anotado,
  `fix f: (Int) -> Int = { ... f(x) ... }`, donde `f` es global); no hay `it` implícito ni sintaxis `::nombre`; los métodos y constructores no se pasan como
  valores (envuélvelos en una lambda); no hay lambdas genéricas ni cotas en las interfaces; `ordena` es una inserción estable O(n²).

### Módulos (M9)

- **Sintaxis:** `use "ruta"` y `use "ruta" como alias` al nivel superior de un archivo (`use` y `como` son palabras contextuales: siguen siendo
  nombres válidos en otros sitios); `open` antes de `fn`, `thing`, `kind` o `fix` hace pública esa declaración. `open mut` es un error (otro
  archivo no podría cambiarla). Los métodos y campos no tienen visibilidad propia: la visibilidad es del nivel superior. Los nombres
  cualificados `alias.nombre` valen como valor, llamada (`a.f(x)`, `a.Clase(x)`) y tipo (`x: a.Clase`, `is a.Clase`, `: a.Padre`).
- **Resolución:** la ruta se resuelve respecto a la carpeta del archivo que escribe el `use`, se le añade `.jb` y se normalizan `.` y `..` (no
  hay carpeta raíz de proyecto ni manifiesto). Importar es transitivo hacia dentro pero no hacia fuera: `main` ve lo abierto de `formas`, no lo
  que `formas` importó. Sin alias, los nombres abiertos entran directamente; chocar con una declaración propia o con otro `use` es un error
  (se resuelve con `como`). Un alias no puede repetir un nombre ya usado.
- **Cómo se une todo:** `modules::load` lee el archivo principal, carga recursivamente cada `use` (cada archivo una sola vez, dependencias
  primero; un ciclo es un error que muestra la cadena `a -> b -> a`) y devuelve un único `Program` con las sentencias de todos en orden de
  dependencia. Por eso los efectos del nivel superior de un módulo (`show`, variables globales) ocurren una vez, antes de los de quien lo
  importa. El verificador, el compilador y la VM no saben que existen módulos.
- **Renombrado:** las declaraciones del nivel superior de los archivos distintos del principal se renombran a `ruta.nombre` (`util/mate.cuadrado`);
  el principal conserva sus nombres. Un recorrido por el AST de cada archivo cambia cada referencia (variables, llamadas, tipos, clases
  base, `is`/`as`/`rescue`, asignaciones) respetando los nombres locales que la tapan (parámetros, variables, parámetros de tipo, campos de la
  clase). Así dos archivos pueden tener un `ayuda` privado, y los mensajes de error nombran el módulo (`util/mate.Punto`), al igual que
  `show` de un objeto.
- **Errores con archivo:** todos los archivos comparten un único espacio de posiciones (cada uno empieza donde acabó el anterior), así que un
  `Diagnostic` del verificador, del compilador o de la VM encuentra su archivo y su línea (`SourceMap`). Si un nombre no existe pero es
  privado en un módulo importado, el error lo dice: «`ayuda` es privado en `util`: declárala con `open` allí».
- **Límites conocidos de M9:** no hay `use` en el REPL; no hay reexportaciones ni importar solo algunos nombres; no hay manifiesto ni carpeta
  de paquetes (un «paquete» es una carpeta de archivos con `use`); dentro de un método, un campo heredado de una clase de otro archivo no tapa a
  una global del mismo nombre del archivo del método (el renombrado no ve los campos del padre).

### Biblioteca estándar (M10)

- **Dos capas.** Lo que no se puede escribir en And23 son funciones *nativas* (Rust): una tabla única, `and23_types::NATIVES` (nombre, tipos de
  los parámetros y del resultado), que usan los tres pasos: el verificador las tipa como las funciones integradas (`len`...), el compilador
  emite `Op::Native(id)` y la VM las ejecuta (`and23-vm/src/natives.rs`). El orden de la tabla fija el id. Todo lo demás está en el preludio,
  escrito en el propio lenguaje, encima de las nativas. Las nativas con prefijo `_` (`_trozo`, `_busca`, `_leeArchivo`...) son detalles del
  preludio; las demás (`raiz`, `existe`, `leeLinea`...) las usa el programa directamente.
- **Métodos de `Str`.** `Str` no es un objeto, así que `s.metodo(args)` no pasa por la tabla de métodos: el verificador busca la función
  `_str_metodo` del preludio (la cadena es su primer argumento), la tipa como una llamada normal y anota la redirección
  (`Resolutions.redirects`) para que el compilador emita `Call`. `Int.parse` y `Float.parse` igual (`_int_parse`, `_float_parse`, sin receptor).
  Añadir un método a `Str` es escribir una función del preludio. `s[i]` es una operación de la VM (`Index` acepta cadenas); asignar a `s[i]` es
  un error de tipos.
- **Posiciones de texto.** Cuentan caracteres (no bytes), como `len`. Con texto ASCII la conversión a bytes es directa; con otro texto es O(n) por
  llamada.
- **Números.** `parse` devuelve un opcional (nil si no es un número, también `nan` e `inf`) en vez de lanzar: `Int.parse(s) ?: 0`. No hay
  conversión implícita entre Int y Float: `aFloat`, `piso`, `techo` y `redondea` convierten, y un Float no finito o fuera del rango de Int es un
  error de ejecución. Los nombres de las matemáticas son españoles (`absoluto`, `minimo`, `maximo`) con variante `F` para Float, porque no hay
  sobrecarga.
- **Archivos.** Las nativas de archivos devuelven `nil`/`false` y guardan el mensaje en la VM (`_errorES`); el preludio lo convierte en un
  `ErrorES` (que hereda de `Fallo`), así que se atrapa con `rescue e: ErrorES` o `rescue e: Fallo`. Sin atrapar, es un error con la posición de
  la llamada. `leeArchivo` quita el BOM de UTF-8; `lineas()` acepta `
` y `
` y no crea una línea vacía por el salto final. `leeLinea()` lee
  la entrada estándar de la VM (nil al terminar); como el REPL también la lee, no se mezclan.
- **El preludio es redefinible.** Al añadir funciones al preludio, un programa que ya tenía una con ese nombre dejaría de compilar. Para evitarlo,
  tras cargar el preludio el verificador y el compilador marcan sus funciones como redefinibles: la del programa gana y lo ya compilado del
  preludio sigue usando la suya (las llamadas ya van por índice). Las clases del preludio y las funciones integradas no se pueden redefinir.
- **Bucle de lectura.** `mut l = leeLinea()` seguido de `while l != nil { ...; l = leeLinea() }` funciona, también en el nivel superior (ver
  "Estrechamiento de globales `mut`"); `while true { fix l = leeLinea(); if l == nil { break } ... }` es la alternativa.
- **Límites conocidos de M10:** sin formato de números (decimales), `Mapa` ni ordenar por clave en la biblioteca; sin Unicode avanzado (`mayus`/`minus` usan las reglas de Rust,
  `recorta` quita el espacio Unicode); `leeLinea` bloquea la VM mientras espera; el intérprete de árbol no conoce la biblioteca.

### Ergonomía tras M10

- **`show` de colecciones.** `Lista` se imprime como el arreglo que guarda (`[1, 2, 3]`) y `Mapa` como `{"a": 1, "b": 2}`. Lo decide la
  impresión del heap (`render_rec`), que reconoce las clases `Lista` y `Mapa` del preludio por su nombre y forma; también vale para `"${m}"`.
  Un `Mapa` no recuerda el orden de inserción: sus entradas salen en el orden de sus cubetas (estable entre ejecuciones, pero no
  alfabético). Las clases del usuario siguen mostrándose como `Clase(campo=valor)`, y un ciclo se corta con `...`.
- **Estrechamiento de globales `mut`.** Antes ninguna global `mut` se estrechaba, lo que hacía inútil `mut linea = leeLinea()` +
  `while linea != nil` en el nivel superior. Ahora se estrecha, con la misma regla de las locales (se ensancha al asignarle algo que puede
  ser `nil`; no se estrecha si se asigna en un bucle anidado), siempre que (1) el código esté en el nivel superior —no dentro de una
  función, método o lambda— y (2) ninguna función, método ni lambda del programa la asigne (el verificador lo calcula por cada entrada y lo
  recuerda entre entradas del REPL). Así una llamada entre la comprobación y el uso no puede cambiarla. La condición (1) cierra el caso del
  REPL en que una lambda definida después, que asigna la global, le llega como argumento a una función ya verificada. Un
  estrechamiento por `if x == nil { fail ... }` en el nivel superior no se aplica a las `mut` (no habría dónde deshacerlo al asignar).
- **Identidad de los objetos.** `==` entre objetos (instancias y arreglos) es por referencia, y `hash` es coherente con eso: un objeto da
  siempre el mismo hash y dos objetos distintos, aunque tengan los mismos campos, son claves distintas de un `Mapa` (`m.obtiene(P(1))` no
  encuentra lo que se guardó con otro `P(1)`). Las cadenas sí se comparan y se hashean por contenido; para indexar por contenido se usa una
  cadena (`"${p.nombre}"`) como clave. El hash de un objeto sale de su número de serie (orden de creación, módulo 2^32), que cada casilla del
  heap guarda en el relleno que ya tenía: no agranda el heap (1 000 000 de objetos vivos: 89,7 MB antes y después) ni cambia la velocidad.
  No sale de la casilla, porque el recolector las reutiliza y el orden de un `Mapa` con claves-objeto dependería de cuándo corra. Tras
  2^32 asignaciones los números se repiten: dos objetos tan lejanos solo comparten hash.

### Concurrencia (M11)

- **Modelo.** Tareas cooperativas dentro de una sola VM y un solo heap, más canales. No hay hilos del sistema operativo: el heap y el
  recolector no son seguros entre hilos y hacerlos seguros costaría bloqueos y carreras de datos. Una tarea solo cede el control cuando
  llama a `cede()`, cuando espera (canal vacío o lleno, `espera()`) o cuando termina. Por eso el programa es determinista, se prueba con
  `--gc-stress` como todo lo demás y no hay carreras: las variables compartidas (también las `mut` capturadas) no se pisan a mitad de una
  operación. Costo: no se usan varios núcleos y una tarea que no cede acapara la VM.
- **Sin sintaxis nueva.** Todo es biblioteca, escrita en el preludio sobre cuatro primitivas:
  `_lanza(f: () -> Unit)`, `cede()`, `_bloquea()` y `_avisa()`. La VM las atiende dentro de `Op::Native` (necesitan el estado del
  planificador, que una nativa normal no ve); la tabla `NATIVES` solo les da tipo y número.
- **API pública.**
  - `lanza(f: () -> Unit) -> Tarea`: crea la tarea y vuelve de inmediato; la nueva tarea empieza cuando la actual cede o espera.
  - `lanzaDe<T>(f: () -> T) -> TareaDe<T>`: lo mismo para una tarea que devuelve un valor. Son dos funciones porque `Unit` no puede ser
    argumento de tipo (no hay forma de escribir `lanza<Unit>`).
  - `Tarea` y `TareaDe<T>`: `espera()` (bloquea hasta que termine y entrega el resultado; si la tarea lanzó un `Fallo`, lo relanza) y
    `terminada() -> Bool`.
  - `Canal<T>(capacidad: Int)`: cola acotada. `envia(x)` espera si está llena; `recibe() -> T?` espera si está vacía y devuelve `nil` solo
    cuando el canal está cerrado y vacío; `cierra()`; enviar a un canal cerrado lanza `ErrorCanal` (hereda de `Fallo`).
  - `cede()`: pasa al final de la cola de listas.
- **Planificador.** Cola FIFO de tareas listas y lista de tareas esperando. `_bloquea()` mueve la tarea actual a las esperando; `_avisa()`
  las devuelve a todas a la cola de listas (cada una vuelve a comprobar su condición en un `while`). Si una tarea espera y no queda ninguna
  lista, es un error de ejecución «interbloqueo: todas las tareas esperan», con la posición de la espera. Cuando termina la tarea principal el
  programa acaba y las demás se descartan (como en Go).
- **VM.** Cada tarea tiene su pila de valores, de marcos y de manejadores `attempt`; la tarea en ejecución usa las variables locales de
  `execute` y las demás están guardadas. Cambiar de tarea es intercambiar ese estado. Una tarea nueva empieza en la función de su lambda
  con la lambda como único valor de la pila. Raíces del recolector: la pila actual, las de todas las tareas guardadas, las globales y las
  constantes. El límite de recursión se aplica a cada tarea por separado.
- **Errores.** El cuerpo de cada tarea lo envuelve el preludio en un `attempt`: un `Fallo` queda guardado en la `Tarea` y lo ve quien llame a
  `espera()`. Un error de ejecución (división por cero, desbordamiento...) aborta todo el programa, como en la tarea principal.
- **Límites previstos.** Sin expulsión por tiempo (un bucle que no cede bloquea al resto); `leeLinea` bloquea toda la VM; un `Fallo` de una tarea que nadie espera se pierde sin aviso; el REPL descarta las
  tareas pendientes al terminar cada entrada; el intérprete de árbol no conoce las tareas.

### Biblioteca estándar 2 y trazas de pila

- **Literales con exponente.** `1e3`, `2.5E-4`, `1_0e+1` son `Float`. Tras la `e` (y un signo opcional) tiene que haber un dígito; si no,
  el número termina antes y la `e` empieza un identificador.
- **Argumentos y entorno.** `and23 [opciones] archivo.jb a b c`: lo que sigue al archivo son los argumentos del programa y `argumentos()`
  los devuelve como `Lista<Str>`. Las opciones de la VM (`--gc-stress`, `--max-depth N`) van antes del archivo; después del archivo
  todo pertenece al programa. `entorno(nombre)` da el valor de una variable de entorno o `nil`.
- **Tiempo.** `ahora()` son milisegundos desde 1970 (UTC). `duerme(ms)` espera al menos ese tiempo: si hay otras tareas listas les cede el
  turno (con una pausa de 1 ms por vuelta para no gastar CPU de más cuando solo hay dormilonas), y si no hay ninguna duerme de verdad.
- **Azar.** `azar(n)` es un entero en `[0, n)` (error si `n <= 0`), `azarF()` un `Float` en `[0, 1)` y `semilla(n)` fija la secuencia:
  la misma semilla da siempre los mismos números. Sin `semilla`, la secuencia arranca del reloj. Generador xorshift64*; no es
  criptográfico.
- **Rutas y carpetas.** `unirRuta(a, b)`, `nombreDe(r)`, `carpetaDe(r)` y `extension(r)` trabajan sobre texto y entienden `/` y `\`.
  `esDir`, `listaDir(r)` (nombres en orden alfabético) y `creaDir(r)` (crea también los padres) usan el sistema de archivos; los fallos
  lanzan `ErrorES`. Los nombres de las nativas son internos (`_ahora`, `_azar`...) y las funciones públicas son del preludio, así que un
  programa puede reutilizar esos nombres para sus variables o funciones. Las nativas que devuelven listas devuelven arreglos (`[Str]`) y el
  preludio los convierte en `Lista<Str>`.
- **Trazas de pila.** Un error de ejecución añade la cadena de llamadas pendientes, de la más interna a la más externa:
  ``llamado desde `b` (archivo:línea:columna)``. Si hay más de once se muestran las ocho más internas y las tres más externas con una
  línea «... N llamadas más ...» (una recursión infinita no inunda la pantalla). Las funciones del preludio no aparecen: un error dentro
  de `Lista.obtiene` se señala en la línea del usuario que la llamó (antes caía en una posición sin sentido del archivo del usuario, porque
  las posiciones del preludio no pertenecen a ningún archivo). La traza va en `Diagnostic.trace`; la VM la calcula solo cuando falla, así
  que no cuesta nada en la ejecución normal.
- **Límites.** Sin formato de números (decimales fijos), sin `Mapa` ordenado, sin borrar carpetas ni copiar o renombrar archivos, sin
  separadores propios del sistema en `unirRuta` (siempre `/`; Windows acepta ambos), sin reloj monótono ni zonas horarias ni fechas.

### Pruebas, formateador y editor

- **Pruebas.** `prueba "nombre" { ... }` es una declaración del nivel superior (`prueba` es una palabra contextual: solo cuenta cuando va
  seguida de un texto fijo). El parser la convierte en una función sin parámetros `_prueba_N` y anota la prueba en `Program.tests`; el resto
  del compilador no sabe de pruebas. `and23 --prueba archivo.jb` carga el programa, ejecuta su nivel superior (sin `main`) y llama a cada
  función `_prueba_N` por separado con `Session::run_function`: un fallo (una excepción sin capturar o un error de ejecución) se informa con
  su posición y la siguiente prueba sigue. Salida 1 si alguna falla. El preludio aporta `afirma`, `afirmaIgual<T>` y `FalloPrueba`. Las
  pruebas de los módulos importados se descartan; las variables globales se comparten entre pruebas (no hay aislamiento).
- **Formateador.** `and23 fmt [--check] archivos...` (`and23_syntax::format`). Es conservador a propósito: trabaja línea a línea y no
  cambia saltos de línea ni espaciado dentro de la línea, así que no puede alterar el significado ni mover comentarios (un test comprueba
  que los tokens de todos los ejemplos y del preludio no cambian y que formatear dos veces da lo mismo). Normaliza la sangría (un nivel por
  cada línea distinta en que se abrió un `{`, `(` o `[` aún abierto, de modo que `f(g {` en una línea sangra un nivel; las líneas que
  empiezan cerrando retroceden; +4 para las que empiezan con `.`, `?.`, `&&`, `||` o `?:` o siguen a una línea que acaba en operador),
  quita espacios finales, junta las líneas en blanco y deja un solo salto de línea final. Las cadenas de varias líneas no se tocan y se
  conserva el fin de línea del archivo (LF o CRLF). Límite: no parte líneas largas ni alinea nada.
- **Editor.** `editors/vscode` es una extensión sin código: una gramática TextMate (palabras clave, cadenas con `${}`, números con
  exponente, tipos, llamadas, operadores) y la configuración del lenguaje (comentarios, pares, sangría). No hay servidor de lenguaje.

### Mn: auto-compilación (en curso)

Plan: reescribir el compilador en And23 por etapas, comprobando cada una contra la de Rust. Etapa 1, el lexer.

- **Etapa 1: `mn/lexer.jb`.** El lexer de And23 escrito en And23 (unas 330 líneas). Hace lo mismo que `lexer.rs` y vuelca los tokens con el
  formato de `and23 --tokens archivo.jb` (una línea por token; las cadenas con sus partes sangradas; `Error mensaje` si el código no se puede
  leer). `and23 mn/lexer.jb archivo.jb` y `and23 --tokens archivo.jb` tienen que dar exactamente lo mismo: el test
  `el_lexer_escrito_en_and23_coincide_con_el_de_rust` lo exige sobre todos los ejemplos, el preludio, el propio `mn/lexer.jb` y 20 casos
  límite de `mn/casos` (escapes, `${}` anidadas, Unicode, exponentes, CRLF, BOM, archivo vacío y cada error de lexer).
- **Rendimiento.** 1,7 MB de código (el preludio repetido 100 veces, 423 101 tokens): 2,3 s el de And23 contra 0,16 s el de Rust, unas 14
  veces más lento, con la misma salida.
- **Huecos del lenguaje que apareció escribirlo, y qué se hizo:**
  - Clasificar caracteres: no había forma. Ahora `c.esLetra()`, `c.esAlfanumerico()` (Unicode) y `c.esDigito()` (ASCII).
  - Recorrer un texto: `s[i]` cuenta desde el principio cada vez (O(n)) y recorrer un archivo era cuadrático. Ahora `s.caracteres()` da
    una `Lista<Str>` en O(n) y el lexer trabaja sobre ella.
  - Crear un carácter desde su código: `desdeCodigo(n)` (para los escapes Unicode de las cadenas; nil si no es un punto de código válido).
  - `une(partes, sep)` era cuadrática (concatenaba de una en una; 8,8 s para 375 KB). Ahora es una nativa lineal.
- **Huecos que siguen** (el lexer los rodea, pero el parser los notará): no hay posiciones (el lexer de Rust lleva el byte inicial y final de
  cada token y el de And23 no, porque no hay forma de pasar de un índice de carácter a un byte); no hay enumerados ni `match`, así que el tipo de
  un token es una cadena (`"Fix"`) y se compara con `==`; `if` no es una expresión (hay que escribir `if ... { return a }; return b`); los
  decimales fuera de rango son un error y no `inf`.
- **Siguiente.** Posiciones en los tokens, y entonces el parser. Antes conviene decidir si se añaden al lenguaje los enumerados y `match`
  (cambiarían mucho cómo se escribe un AST) o se sigue con cadenas y clases.

## Fuentes de referencia

- Crafting Interpreters (VM de bytecode, GC): https://craftinginterpreters.com
- Writing a Compiler in Go (AST, compilador, VM): https://compilerbook.com
- Especificación de la JVM (class file, despacho): https://docs.oracle.com/javase/specs/jvms/se21/html/
- Dora VM (lenguaje con VM en Rust, MIT): https://github.com/dinfuehr/dora
- Wren (VM pequeña con clases): https://wren.io

Los datos de crates (versiones, licencias) se verificaron en crates.io el 2026-11-05:
`gc-arena` 0.7.0 (MIT), `logos` 0.16.1, `ariadne` 0.6.0. Los libros y la especificación de la JVM no se verificaron.
