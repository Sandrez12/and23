#!/usr/bin/env bash
# Mide cuánto tarda And23 en unas cargas fijas y escribe una tabla en Markdown.
#
#   scripts/rendimiento.sh            muestra la tabla
#   scripts/rendimiento.sh --escribe  la guarda en docs/rendimiento.md (con la fecha, la versión y la máquina)
#   REPETICIONES=9 scripts/rendimiento.sh   más repeticiones (por defecto 5; se informa la mediana)
#
# Usa el binario de release (`cargo build --release`), que construye si falta. Los tiempos dependen de la máquina:
# sirven para comparar antes y después de un cambio en la misma máquina, no entre máquinas.
set -euo pipefail

cd "$(dirname "$0")/.."
REPS="${REPETICIONES:-5}"
cargo build --release -p and23-cli >/dev/null 2>&1
BIN="$PWD/target/release/and23"
[ -x "$BIN" ] || BIN="$BIN.exe"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Segundos (con milésimas) que tarda `"$@"`, con la salida descartada. `time` de bash vale en Linux, macOS y Git Bash.
una_vez() {
    local t
    TIMEFORMAT=%R
    t=$({ time "$@" >/dev/null 2>&1; } 2>&1)
    echo "${t/,/.}"
}

# Mediana de REPS ejecuciones.
mediana() {
    local i
    local -a tiempos=()
    for ((i = 0; i < REPS; i++)); do tiempos+=("$(una_vez "$@")"); done
    printf '%s\n' "${tiempos[@]}" | sort -n | sed -n "$((REPS / 2 + 1))p"
}

# Para la comparación de la autocompilación: el compilador de Mn compilado por Mn (generación 1) y el preludio.
"$BIN" mn/compilador.jb --preludio >"$TMP/preludio.txt"
"$BIN" mn/compilador.jb mn/compilador.jb >"$TMP/compilador.txt"

TABLA="| Carga | Mediana (s) |
|---|---|"
fila() {
    local nombre="$1"
    shift
    local t
    t=$(mediana "$@")
    echo "  $nombre: $t s" >&2
    TABLA="$TABLA
| $nombre | $t |"
}

echo "Midiendo ($REPS repeticiones, mediana)..." >&2
fila "Arranque (preludio + un programa mínimo)" "$BIN" examples/aritmetica.jb
fila "Bucle de 30 M de vueltas + fib(30)" "$BIN" benchmarks/bucle.jb
fila "Lista y Mapa: 300 mil inserciones y lecturas" "$BIN" benchmarks/colecciones.jb
fila "Texto: 100 mil cadenas, unir, partir y buscar" "$BIN" benchmarks/cadenas.jb
fila "Tareas y canales: 100 mil mensajes, 4 trabajadores" "$BIN" benchmarks/tareas.jb
fila "Rust: verificar tipos de mn/checker.jb (--check)" "$BIN" --check mn/checker.jb
fila "Rust: compilar mn/compilador.jb (--dump)" "$BIN" --dump mn/compilador.jb
fila "Mn compilado por Rust: verificar mn/checker.jb" "$BIN" mn/checker.jb mn/checker.jb
fila "Mn compilado por Rust: compilar mn/compilador.jb" "$BIN" mn/compilador.jb mn/compilador.jb
fila "Mn compilado por Mn: compilar mn/compilador.jb (punto fijo)" "$BIN" --carga "$TMP/preludio.txt" "$TMP/compilador.txt" mn/compilador.jb

if [ "${1:-}" = "--escribe" ]; then
    {
        echo "# Rendimiento"
        echo
        echo "Línea de referencia para detectar regresiones. Se repite con \`scripts/rendimiento.sh --escribe\` (necesita bash y"
        echo "un binario de release; las cargas están en \`benchmarks/\` y en \`mn/\`). Los tiempos son de **esta máquina**:"
        echo "se comparan antes y después de un cambio, no entre máquinas."
        echo
        echo "- Fecha: $(date +%Y-%m-%d)"
        echo "- Versión: $("$BIN" --version) (commit $(git rev-parse --short HEAD))"
        echo "- Compilador: $(rustc --version)"
        echo "- Sistema: $(uname -srm)"
        echo "- Repeticiones: $REPS (mediana)"
        echo
        echo "$TABLA"
    } >docs/rendimiento.md
    echo "Escrito docs/rendimiento.md" >&2
else
    echo
    echo "$TABLA"
fi
