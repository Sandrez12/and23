use crate::vm::Vm;
use and23_bytecode::Compiler;
use and23_syntax::{Diagnostic, Program};
use std::io::Write;

/// Compilador y VM con estado compartido: las variables y funciones sobreviven entre llamadas
/// a `run` (REPL).
pub struct Session<W: Write> {
    compiler: Compiler,
    vm: Vm<W>,
}

impl<W: Write> Session<W> {
    pub fn new(out: W) -> Self {
        Session {
            compiler: Compiler::new(),
            vm: Vm::new(out),
        }
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.vm = self.vm.with_max_depth(max_depth);
        self
    }

    pub fn out(&mut self) -> &mut W {
        &mut self.vm.out
    }

    /// Compila y ejecuta el programa. `echo` imprime también el valor de las expresiones
    /// sueltas del nivel superior (modo REPL).
    pub fn run(&mut self, program: &Program, echo: bool) -> Result<(), Diagnostic> {
        let first_new_global = self.compiler.module().globals.len();
        let script = self.compiler.compile(program, echo)?;
        let result = self.vm.run(self.compiler.module(), script);
        if result.is_err() {
            // Una global cuyo `fix`/`mut` no llegó a ejecutarse no existe: puede redeclararse.
            let vm = &self.vm;
            self.compiler
                .forget_globals(first_new_global, |slot| vm.is_defined(slot));
        }
        result
    }

    /// Llama a `main()` si el programa la definió.
    pub fn run_main(&mut self) -> Result<(), Diagnostic> {
        let Some(index) = self.compiler.main_index() else {
            return Ok(());
        };
        let main = &self.compiler.module().functions[index];
        if main.arity != 0 {
            return Err(Diagnostic::new(
                main.name_span,
                "`main` no puede tener parámetros",
            ));
        }
        self.vm.run(self.compiler.module(), index)
    }
}
