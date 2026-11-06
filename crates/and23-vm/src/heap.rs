//! Heap de objetos (cadenas, arreglos e instancias de clases) con recolector mark-sweep.
//!
//! Los objetos viven en un `Vec` de ranuras y se referencian por índice (`ObjRef`). El heap
//! nunca recolecta por su cuenta: la VM llama a `collect` en puntos seguros (antes de las
//! instrucciones que reservan memoria), cuando todo valor vivo está en la pila, en las globales
//! o en las constantes. Así no hay valores "en la mano" de Rust que el recolector no vea.

use crate::value::Value;
use std::fmt::Write;

/// Referencia a un objeto del heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjRef(u32);

#[derive(Debug)]
pub enum Object {
    Str(Box<str>),
    Array(Vec<Value>),
    /// Instancia de la clase número `class` del módulo, con sus campos en orden de declaración.
    Instance {
        class: u32,
        fields: Vec<Value>,
    },
    /// Lambda: la función `function` del módulo y los valores que capturó.
    Closure {
        function: u32,
        captures: Vec<Value>,
    },
    /// Caja compartida de una variable `mut` capturada por una lambda.
    Cell(Value),
}

/// Lo que el heap sabe de una clase: solo para imprimir sus instancias.
struct ClassMeta {
    name: String,
    fields: Vec<String>,
}

impl Object {
    /// Tamaño aproximado en bytes, para decidir cuándo recolectar.
    fn size(&self) -> usize {
        OBJECT_OVERHEAD
            + match self {
                Object::Str(s) => s.len(),
                Object::Array(v) => v.capacity() * std::mem::size_of::<Value>(),
                Object::Instance { fields, .. } => fields.len() * std::mem::size_of::<Value>(),
                Object::Closure { captures, .. } => captures.len() * std::mem::size_of::<Value>(),
                Object::Cell(_) => std::mem::size_of::<Value>(),
            }
    }
}

const OBJECT_OVERHEAD: usize = 32;
/// No se recolecta mientras el heap ocupe menos que esto.
const MIN_THRESHOLD: usize = 1024 * 1024;

struct Slot {
    object: Option<Object>,
    marked: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeapStats {
    /// Objetos vivos ahora.
    pub live_objects: usize,
    /// Ranuras que el heap llegó a reservar (cota del máximo de objetos simultáneos).
    pub slots: usize,
    /// Objetos creados desde el inicio.
    pub allocated: u64,
    /// Objetos liberados por el recolector.
    pub freed: u64,
    /// Recolecciones hechas.
    pub collections: u64,
    /// Bytes aproximados que ocupan los objetos vivos.
    pub bytes: usize,
}

pub struct Heap {
    slots: Vec<Slot>,
    free: Vec<u32>,
    bytes: usize,
    threshold: usize,
    stress: bool,
    live: usize,
    allocated: u64,
    freed: u64,
    collections: u64,
    classes: Vec<ClassMeta>,
}

impl Default for Heap {
    fn default() -> Self {
        Heap {
            slots: Vec::new(),
            free: Vec::new(),
            bytes: 0,
            threshold: MIN_THRESHOLD,
            stress: false,
            live: 0,
            allocated: 0,
            freed: 0,
            collections: 0,
            classes: Vec::new(),
        }
    }
}

impl Heap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Modo stress: recolecta en cada punto seguro, para destapar raíces olvidadas.
    pub fn set_stress(&mut self, stress: bool) {
        self.stress = stress;
    }

    pub fn stats(&self) -> HeapStats {
        HeapStats {
            live_objects: self.live,
            slots: self.slots.len(),
            allocated: self.allocated,
            freed: self.freed,
            collections: self.collections,
            bytes: self.bytes,
        }
    }

    pub fn alloc_str(&mut self, text: String) -> Value {
        self.alloc(Object::Str(text.into_boxed_str()))
    }

    pub fn alloc_array(&mut self, items: Vec<Value>) -> Value {
        self.alloc(Object::Array(items))
    }

    pub fn alloc_instance(&mut self, class: u32, fields: Vec<Value>) -> Value {
        self.alloc(Object::Instance { class, fields })
    }

    pub fn alloc_closure(&mut self, function: u32, captures: Vec<Value>) -> Value {
        self.alloc(Object::Closure { function, captures })
    }

    pub fn alloc_cell(&mut self, value: Value) -> Value {
        self.alloc(Object::Cell(value))
    }

    /// Función de una lambda, si `r` lo es.
    pub fn closure_function(&self, r: ObjRef) -> Option<u32> {
        match self.get(r) {
            Object::Closure { function, .. } => Some(*function),
            _ => None,
        }
    }

    /// Valor capturado número `index` de la lambda `r`.
    pub fn capture(&self, r: ObjRef, index: u32) -> Value {
        match self.get(r) {
            Object::Closure { captures, .. } => captures[index as usize],
            _ => unreachable!("el compilador garantiza una lambda"),
        }
    }

    pub fn cell(&self, r: ObjRef) -> Value {
        match self.get(r) {
            Object::Cell(value) => *value,
            _ => unreachable!("el compilador garantiza una celda"),
        }
    }

    pub fn set_cell(&mut self, r: ObjRef, value: Value) {
        match self.slots[r.0 as usize].object.as_mut() {
            Some(Object::Cell(slot)) => *slot = value,
            _ => unreachable!("el compilador garantiza una celda"),
        }
    }

    /// Registra las clases del módulo que el heap aún no conoce (el REPL las va añadiendo).
    pub fn sync_classes(&mut self, defs: &[(&str, &[String])]) {
        for (name, fields) in defs.iter().skip(self.classes.len()) {
            self.classes.push(ClassMeta {
                name: name.to_string(),
                fields: fields.to_vec(),
            });
        }
    }

    /// Clase de una instancia.
    pub fn class_of(&self, r: ObjRef) -> Option<u32> {
        match self.get(r) {
            Object::Instance { class, .. } => Some(*class),
            _ => None,
        }
    }

    pub fn field(&self, r: ObjRef, index: u32) -> Value {
        match self.get(r) {
            Object::Instance { fields, .. } => fields[index as usize],
            _ => unreachable!("el verificador de tipos garantiza un objeto"),
        }
    }

    pub fn set_field(&mut self, r: ObjRef, index: u32, value: Value) {
        match self.slots[r.0 as usize].object.as_mut() {
            Some(Object::Instance { fields, .. }) => fields[index as usize] = value,
            _ => unreachable!("el verificador de tipos garantiza un objeto"),
        }
    }

    fn alloc(&mut self, object: Object) -> Value {
        self.bytes += object.size();
        self.live += 1;
        self.allocated += 1;
        let slot = Slot {
            object: Some(object),
            marked: false,
        };
        let index = match self.free.pop() {
            Some(index) => {
                self.slots[index as usize] = slot;
                index
            }
            None => {
                self.slots.push(slot);
                (self.slots.len() - 1) as u32
            }
        };
        Value::Obj(ObjRef(index))
    }

    pub fn get(&self, r: ObjRef) -> &Object {
        self.slots[r.0 as usize]
            .object
            .as_ref()
            .expect("referencia a un objeto ya liberado: falta una raíz en el recolector")
    }

    pub fn str(&self, r: ObjRef) -> &str {
        match self.get(r) {
            Object::Str(s) => s,
            _ => unreachable!("el verificador de tipos garantiza una cadena"),
        }
    }

    /// Agrega `value` al arreglo `r`.
    pub fn push(&mut self, r: ObjRef, value: Value) {
        let Some(Object::Array(items)) = self.slots[r.0 as usize].object.as_mut() else {
            unreachable!("el verificador de tipos garantiza un arreglo");
        };
        let before = items.capacity();
        items.push(value);
        let after = items.capacity();
        self.bytes += (after - before) * std::mem::size_of::<Value>();
    }

    /// Quita y devuelve el último elemento del arreglo `r`, si hay.
    pub fn pop(&mut self, r: ObjRef) -> Option<Value> {
        let Some(Object::Array(items)) = self.slots[r.0 as usize].object.as_mut() else {
            unreachable!("el verificador de tipos garantiza un arreglo");
        };
        items.pop()
    }

    /// Identidad estable del objeto (no cambia mientras vive: el heap no mueve objetos).
    pub fn id(&self, r: ObjRef) -> u32 {
        r.0
    }

    pub fn array_mut(&mut self, r: ObjRef) -> &mut Vec<Value> {
        match self.slots[r.0 as usize].object.as_mut() {
            Some(Object::Array(items)) => items,
            _ => unreachable!("el verificador de tipos garantiza un arreglo"),
        }
    }

    pub fn should_collect(&self) -> bool {
        self.stress || self.bytes >= self.threshold
    }

    /// Mark-sweep. `roots` son todos los valores vivos fuera del heap.
    pub fn collect(&mut self, roots: impl Iterator<Item = Value>) {
        self.collections += 1;
        // Marcado: recorrido con pila explícita (los arreglos anidados no recursan en Rust).
        let mut gray: Vec<u32> = Vec::new();
        for value in roots {
            self.mark(value, &mut gray);
        }
        while let Some(index) = gray.pop() {
            let mut i = 0;
            loop {
                let child = match self.slots[index as usize].object.as_ref() {
                    Some(Object::Array(items)) => items.get(i).copied(),
                    Some(Object::Instance { fields, .. }) => fields.get(i).copied(),
                    Some(Object::Closure { captures, .. }) => captures.get(i).copied(),
                    Some(Object::Cell(value)) => (i == 0).then_some(*value),
                    _ => None,
                };
                let Some(child) = child else { break };
                self.mark(child, &mut gray);
                i += 1;
            }
        }
        // Barrido: libera lo que no se marcó y limpia las marcas.
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.marked {
                slot.marked = false;
            } else if let Some(object) = slot.object.take() {
                self.bytes -= object.size();
                self.live -= 1;
                self.freed += 1;
                self.free.push(index as u32);
            }
        }
        self.threshold = (self.bytes * 2).max(MIN_THRESHOLD);
    }

    fn mark(&mut self, value: Value, gray: &mut Vec<u32>) {
        if let Value::Obj(r) = value {
            let slot = &mut self.slots[r.0 as usize];
            if !slot.marked {
                slot.marked = true;
                gray.push(r.0);
            }
        }
    }

    /// Texto de un valor tal como lo imprime `show`.
    pub fn render(&self, value: Value) -> String {
        let mut out = String::new();
        self.render_into(value, false, &mut out);
        out
    }

    /// Agrega el texto de `value` a `out`. Dentro de un arreglo las cadenas van entre comillas.
    pub fn render_into(&self, value: Value, quoted: bool, out: &mut String) {
        self.render_rec(value, quoted, out, &mut Vec::new());
    }

    /// `open` son los objetos que se están imprimiendo: un ciclo (`a.sig = a`) se corta con `...`.
    fn render_rec(&self, value: Value, quoted: bool, out: &mut String, open: &mut Vec<ObjRef>) {
        match value {
            Value::Obj(r) if open.contains(&r) => out.push_str("..."),
            Value::Obj(r) => match self.get(r) {
                Object::Str(s) if quoted => {
                    out.push('"');
                    for c in s.chars() {
                        match c {
                            '"' => out.push_str("\\\""),
                            '\\' => out.push_str("\\\\"),
                            '\n' => out.push_str("\\n"),
                            '\t' => out.push_str("\\t"),
                            c => out.push(c),
                        }
                    }
                    out.push('"');
                }
                Object::Str(s) => out.push_str(s),
                Object::Array(items) => {
                    open.push(r);
                    out.push('[');
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        self.render_rec(*item, true, out, open);
                    }
                    out.push(']');
                    open.pop();
                }
                Object::Instance { class, fields } => {
                    let meta = &self.classes[*class as usize];
                    open.push(r);
                    out.push_str(&meta.name);
                    out.push('(');
                    for (i, item) in fields.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(&meta.fields[i]);
                        out.push('=');
                        self.render_rec(*item, true, out, open);
                    }
                    out.push(')');
                    open.pop();
                }
                Object::Closure { .. } => out.push_str("<función>"),
                Object::Cell(inner) => self.render_rec(*inner, quoted, out, open),
            },
            other => {
                let _ = write!(out, "{other}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(v: Value) -> ObjRef {
        match v {
            Value::Obj(r) => r,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn libera_lo_inalcanzable_y_conserva_lo_alcanzable() {
        let mut heap = Heap::new();
        let leaf = heap.alloc_str("hoja".to_string());
        let garbage = heap.alloc_str("basura".to_string());
        let tree = heap.alloc_array(vec![leaf, Value::Int(1)]);
        let root = heap.alloc_array(vec![tree]);
        heap.collect([root].into_iter());
        let stats = heap.stats();
        assert_eq!((stats.live_objects, stats.freed), (3, 1));
        assert_eq!(heap.str(obj(leaf)), "hoja");
        assert_eq!(heap.render(root), "[[\"hoja\", 1]]");
        let _ = garbage;
    }

    #[test]
    fn reutiliza_las_ranuras_liberadas() {
        let mut heap = Heap::new();
        for i in 0..100 {
            heap.alloc_str(format!("s{i}"));
            heap.collect(std::iter::empty());
        }
        assert_eq!(heap.stats().slots, 1);
        assert_eq!(heap.stats().live_objects, 0);
        assert_eq!(heap.stats().bytes, 0);
    }

    #[test]
    fn dos_raices_comparten_un_objeto() {
        let mut heap = Heap::new();
        let shared = heap.alloc_str("compartida".to_string());
        let a = heap.alloc_array(vec![shared]);
        let b = heap.alloc_array(vec![shared]);
        heap.collect([a].into_iter());
        assert_eq!(heap.stats().live_objects, 2);
        heap.collect([a, b].into_iter());
        assert_eq!(heap.stats().live_objects, 2);
        heap.collect(std::iter::empty());
        assert_eq!(heap.stats().live_objects, 0);
    }

    #[test]
    fn el_umbral_crece_con_lo_vivo() {
        let mut heap = Heap::new();
        assert!(!heap.should_collect());
        let big = heap.alloc_str("x".repeat(2 * 1024 * 1024));
        assert!(heap.should_collect());
        heap.collect([big].into_iter());
        assert!(!heap.should_collect(), "el umbral debe duplicar lo vivo");
        heap.set_stress(true);
        assert!(heap.should_collect());
    }
}
