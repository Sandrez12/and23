# Rendimiento

Línea de referencia para detectar regresiones. Se repite con `scripts/rendimiento.sh --escribe` (necesita bash y
un binario de release; las cargas están en `benchmarks/` y en `mn/`). Los tiempos son de **esta máquina**:
se comparan antes y después de un cambio, no entre máquinas.

- Fecha: 2026-11-11
- Versión: and23 0.9.0 (commit f3cbd3f)
- Compilador: rustc 1.99.1 (58cf8f9c3 2026-10-13)
- Sistema: MINGW64_NT-10.0-26300 3.5.7-463ebcdc.x86_64 x86_64
- Repeticiones: 5 (mediana)

| Carga | Mediana (s) |
|---|---|
| Arranque (preludio + un programa mínimo) | 0.046 |
| Bucle de 30 M de vueltas + fib(30) | 1.988 |
| Lista y Mapa: 300 mil inserciones y lecturas | 1.039 |
| Texto: 100 mil cadenas, unir, partir y buscar | 0.116 |
| Tareas y canales: 100 mil mensajes, 4 trabajadores | 0.152 |
| Rust: verificar tipos de mn/checker.jb (--check) | 0.051 |
| Rust: compilar mn/compilador.jb (--dump) | 0.490 |
| Mn compilado por Rust: verificar mn/checker.jb | 0.345 |
| Mn compilado por Rust: compilar mn/compilador.jb | 1.395 |
| Mn compilado por Mn: compilar mn/compilador.jb (punto fijo) | 1.327 |

## Lo que se descubrió al medir

- **`s.parte(sep)` era cuadrático.** Se escribía con `_busca` y `_trozo`, y cada llamada convierte posiciones carácter-byte recorriendo la cadena
  (`is_ascii`), así que partir 1,4 MB en 100 000 trozos tardaba **24 s**. Ahora es la nativa `_parte`, en una sola pasada: **0,1 s**. Un test lo vigila.
- **Costes conocidos que no se han tocado** (cada uno es O(n) por llamada, no un fallo): `s[i]` recorre la cadena hasta `i` si no es ASCII, y
  `_busca`/`_trozo` también; recorrer una cadena larga con `s[i]` en un bucle es cuadrático (usa `s.caracteres()`).
- **`Lista.ordena` pasó de O(n²) a O(n log n):** 100 000 enteros tardan menos de un segundo.
- **El verificador de Mn es unas 6 veces más lento que el de Rust** (0,35 s frente a 0,05 s) y el compilador de Mn unas 3 veces (1,4 s frente a
  0,5 s, que incluye arrancar el proceso): es lo esperable de una VM de pila frente a código nativo.
- **`--gc-stress`** (GC en cada punto seguro) hace que el verificador de Mn tarde minutos sobre un archivo; por eso solo se aplica a programas más pequeños.
