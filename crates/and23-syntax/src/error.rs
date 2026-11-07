use crate::span::Span;
use std::fmt;

/// Error de compilación o ejecución con su posición en el código fuente.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
    /// Llamadas pendientes cuando falló la ejecución, de la más interna a la más externa: dónde se llamó
    /// (`None` en la línea que resume las omitidas) y el texto de la línea.
    pub trace: Vec<(Option<Span>, String)>,
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            span,
            message: message.into(),
            trace: Vec::new(),
        }
    }

    /// Las líneas de la traza, con `locate` dando «archivo:línea:columna» de cada posición.
    pub fn trace_text(&self, locate: impl Fn(Span) -> String) -> String {
        self.trace
            .iter()
            .map(|(span, text)| match span {
                Some(span) => format!(
                    "  {text} ({})
",
                    locate(*span)
                ),
                None => format!(
                    "  {text}
"
                ),
            })
            .collect()
    }

    /// Línea (desde 1) y columna (desde 1, en caracteres) donde empieza el error.
    pub fn line_col(&self, src: &str) -> (usize, usize) {
        let start = self.span.start.min(src.len());
        let line = src[..start].matches('\n').count() + 1;
        let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
        (line, src[line_start..start].chars().count() + 1)
    }

    /// Texto listo para mostrar al usuario, con la línea fuente y el subrayado.
    pub fn render(&self, file: &str, src: &str) -> String {
        let start = self.span.start.min(src.len());
        let (line_no, col) = self.line_col(src);
        let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = src[start..].find('\n').map_or(src.len(), |i| start + i);
        let line = src[line_start..line_end].trim_end_matches('\r');
        let end = self.span.end.clamp(start, line_end);
        let width = src[start..end].chars().count().max(1);

        let num = line_no.to_string();
        let pad = " ".repeat(num.len());
        format!(
            "error: {msg}\n{pad}--> {file}:{line_no}:{col}\n{pad} |\n{num} | {line}\n{pad} | {indent}{carets}\n",
            msg = self.message,
            indent = " ".repeat(col - 1),
            carets = "^".repeat(width),
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Diagnostic {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_apunta_a_linea_y_columna() {
        let src = "show(1)\nshow(1 + @)\n";
        let at = src.find('@').unwrap();
        let d = Diagnostic::new(Span::new(at, at + 1), "carácter inesperado");
        assert_eq!(d.line_col(src), (2, 10));
        let out = d.render("t.jb", src);
        assert!(out.contains("--> t.jb:2:10"));
        assert!(out.contains("2 | show(1 + @)"));
        assert!(out.ends_with("^\n"));
    }

    #[test]
    fn columna_cuenta_caracteres_no_bytes() {
        let src = "ñandú + @";
        let at = src.find('@').unwrap();
        let d = Diagnostic::new(Span::new(at, at + 1), "x");
        assert_eq!(d.line_col(src), (1, 9));
    }
}
