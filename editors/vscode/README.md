# And23 para VS Code

Resaltado de sintaxis y configuración básica (comentarios `//`, pares de paréntesis y sangría) para los
archivos `.jb` del lenguaje [And23](https://github.com/Sandrez12/and23).

## Instalar

Sin publicar en el Marketplace: copia esta carpeta a tu carpeta de extensiones y reinicia VS Code.

- Windows: `%USERPROFILE%\.vscode\extensions\and23`
- Linux y macOS: `~/.vscode/extensions/and23`

O empaquétala con [`vsce`](https://github.com/microsoft/vscode-vsce) (`npx @vscode/vsce package`) e instala el
`.vsix` resultante con «Extensions: Install from VSIX...».

## Formato al guardar

El formateador es una orden de la línea de comandos: `and23 fmt archivo.jb`. Para ejecutarlo al guardar puedes usar
una extensión como *Run on Save* con `and23 fmt ${file}`.
