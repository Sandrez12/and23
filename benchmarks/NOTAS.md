## Lo que se descubrió al medir

- **`s.parte(sep)` era cuadrático.** Se escribía con `_busca` y `_trozo`, y cada llamada convierte posiciones carácter-byte recorriendo la cadena
  (`is_ascii`), así que partir 1,4 MB en 100 000 trozos tardaba **24 s**. Ahora es la nativa `_parte`, en una sola pasada: **0,1 s**. Un test lo vigila.
- **Costes conocidos que no se han tocado** (cada uno es O(n) por llamada, no un fallo): `s[i]` recorre la cadena hasta `i` si no es ASCII, y
  `_busca`/`_trozo` también; recorrer una cadena larga con `s[i]` en un bucle es cuadrático (usa `s.caracteres()`).
- **`Lista.ordena` pasó de O(n²) a O(n log n):** 100 000 enteros tardan menos de un segundo.
- **El verificador de Mn es unas 6 veces más lento que el de Rust** (0,35 s frente a 0,05 s) y el compilador de Mn unas 3 veces (1,4 s frente a
  0,5 s, que incluye arrancar el proceso): es lo esperable de una VM de pila frente a código nativo.
- **`--gc-stress`** (GC en cada punto seguro) hace que el verificador de Mn tarde minutos sobre un archivo; por eso solo se aplica a programas más pequeños.
