#!/usr/bin/env sh
# Instala And23 en Linux o macOS: descarga la release (por defecto la última), comprueba su suma SHA-256 y copia `and23` a un directorio.
#
#   curl -fsSL https://raw.githubusercontent.com/Sandrez12/and23/main/scripts/instalar.sh | sh
#
# Variables opcionales:
#   AND23_VERSION=v1.0.0    una versión concreta (por defecto la última)
#   AND23_DESTINO=carpeta   dónde copiar el binario (por defecto ~/.local/bin)
#   AND23_SIN_VERIFICAR=1   no exigir el archivo de sumas (solo para releases antiguas que no lo traen)
set -eu

REPO="Sandrez12/and23"
version="${AND23_VERSION:-latest}"
destino="${AND23_DESTINO:-$HOME/.local/bin}"

if [ -n "${AND23_TARGET:-}" ]; then
    target="$AND23_TARGET"
else
    sistema="$(uname -s)"
    maquina="$(uname -m)"
    case "$sistema-$maquina" in
        Linux-x86_64) target="x86_64-unknown-linux-gnu" ;;
        Darwin-arm64) target="aarch64-apple-darwin" ;;
        *)
            echo "error: no hay binario para $sistema $maquina (hay Linux x86_64, macOS Apple Silicon y Windows x86_64)." >&2
            echo "       Puedes compilar desde el código fuente: https://github.com/$REPO" >&2
            exit 1
            ;;
    esac
fi

descarga() {
    # descarga URL ARCHIVO
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$1" -O "$2"
    else
        echo "error: hace falta curl o wget." >&2
        exit 1
    fi
}

suma_de() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        echo "error: hace falta sha256sum o shasum para verificar la descarga." >&2
        exit 1
    fi
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if [ "$version" = "latest" ]; then
    descarga "https://api.github.com/repos/$REPO/releases/latest" "$tmp/latest.json"
    version="$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$tmp/latest.json" | head -n 1)"
    if [ -z "$version" ]; then
        echo "error: no se pudo saber cuál es la última versión." >&2
        exit 1
    fi
fi

archivo="and23-$version-$target.tar.gz"
base="https://github.com/$REPO/releases/download/$version"
echo "Descargando And23 $version ($target)..."
descarga "$base/$archivo" "$tmp/$archivo"

if descarga "$base/SHA256SUMS.txt" "$tmp/SHA256SUMS.txt" 2>/dev/null; then
    esperado="$(grep " $archivo\$" "$tmp/SHA256SUMS.txt" | cut -d ' ' -f 1)"
    obtenido="$(suma_de "$tmp/$archivo")"
    if [ -z "$esperado" ] || [ "$esperado" != "$obtenido" ]; then
        echo "error: la suma SHA-256 de $archivo no coincide (esperada: ${esperado:-ninguna}, obtenida: $obtenido). No se instala." >&2
        exit 1
    fi
    echo "Suma SHA-256 correcta."
elif [ "${AND23_SIN_VERIFICAR:-0}" = "1" ]; then
    echo "aviso: esta release no trae SHA256SUMS.txt; se instala sin verificar (AND23_SIN_VERIFICAR=1)." >&2
else
    echo "error: no se pudo descargar SHA256SUMS.txt para verificar la descarga (AND23_SIN_VERIFICAR=1 lo omite)." >&2
    exit 1
fi

tar xzf "$tmp/$archivo" -C "$tmp"
mkdir -p "$destino"
cp "$tmp/and23-$version-$target/and23" "$destino/and23"
chmod +x "$destino/and23"
echo "Instalado en $destino/and23"
case ":$PATH:" in
    *":$destino:"*) ;;
    *) echo "aviso: $destino no está en tu PATH; añádelo para poder escribir and23." ;;
esac
