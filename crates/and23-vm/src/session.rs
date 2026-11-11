use crate::vm::Vm;
use and23_bytecode::Compiler;
use and23_syntax::{Diagnostic, Program};
use and23_types::Checker;
use std::io::Write;

/// Compilador y VM con estado compartido: las variables y funciones sobreviven entre llamadas
/// a `run` (REPL).
pub struct Session<W: Write> {
    checker: Checker,
    compiler: Compiler,
    vm: Vm<W>,
}

impl<W: Write> Session<W> {
    pub fn new(out: W) -> Self {
        let mut session = Session {
            checker: Checker::new(),
            compiler: Compiler::new(),
            vm: Vm::new(out),
        };
        // El preludio define `Fallo`, la clase base de las excepciones.
        let prelude = and23_syntax::parse(and23_syntax::PRELUDE).expect("el preludio es válido");
        session
            .run(&prelude, false)
            .expect("el preludio se ejecuta sin errores");
        // Los programas pueden definir funciones con los nombres del preludio (la suya gana).
        session.checker.seal_prelude();
        session.compiler.seal_prelude();
        session
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.vm = self.vm.with_max_depth(max_depth);
        self
    }

    /// Tope de instrucciones de toda la sesión (ver `Vm::with_step_limit`).
    pub fn with_step_limit(mut self, steps: u64) -> Self {
        self.vm = self.vm.with_step_limit(steps);
        self
    }

    /// Argumentos que el programa lee con `args()`.
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.vm.set_args(args);
        self
    }

    /// Recolecta basura en cada punto seguro (modo de prueba del GC).
    pub fn with_gc_stress(mut self, stress: bool) -> Self {
        self.vm.set_gc_stress(stress);
        self
    }

    pub fn heap_stats(&self) -> crate::HeapStats {
        self.vm.heap_stats()
    }

    pub fn out(&mut self) -> &mut W {
        &mut self.vm.out
    }

    /// Verifica tipos, compila y ejecuta el programa. `echo` imprime también el valor de las expresiones
    /// sueltas del nivel superior (modo REPL).
    pub fn run(&mut self, program: &Program, echo: bool) -> Result<(), Diagnostic> {
        let first_new_global = self.compiler.module().globals.len();
        let saved_checker = self.checker.clone();
        let res = self.checker.check(program)?;
        let script = match self.compiler.compile(program, &res, echo) {
            Ok(script) => script,
            Err(e) => {
                self.checker = saved_checker;
                return Err(e);
            }
        };
        let result = self.vm.run(self.compiler.module(), script);
        if result.is_err() {
            // Una global cuyo `fix`/`mut` no llegó a ejecutarse no existe: puede redeclararse.
            let vm = &self.vm;
            let forgotten = self
                .compiler
                .forget_globals(first_new_global, |slot| vm.is_defined(slot));
            self.checker.forget_globals(&forgotten);
        }
        result
    }

    /// Ejecuta la función de nivel superior `name`, sin argumentos (las pruebas de `and23 --prueba`).
    pub fn run_function(&mut self, name: &str) -> Result<(), Diagnostic> {
        let index = self.compiler.function_index(name).ok_or_else(|| {
            Diagnostic::new(
                and23_syntax::Span::new(0, 0),
                format!("no existe la función `{name}`"),
            )
        })?;
        self.vm.run(self.compiler.module(), index)
    }

    /// Las tareas que fallaron y que nadie esperó con `espera()` (su `Fallo` se habría perdido): `Some(texto)` si hubo
    /// alguna, y las olvida. Se llama al terminar el programa o cada prueba.
    pub fn lost_task_failures(&mut self) -> Option<String> {
        let d = self.run_function("_fallosPerdidos").err()?;
        let prefix = "excepción sin capturar `Fallo`: ";
        Some(
            d.message
                .strip_prefix(prefix)
                .unwrap_or(&d.message)
                .to_string(),
        )
    }

    /// El código de salida que pidió el programa con `salir(n)`, si lo hizo (la ejecución ya se detuvo).
    pub fn exit_code(&self) -> Option<u8> {
        self.vm.exit_code()
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
