use egg::{
    AstSize, EGraph, Extractor, Id, Language, Pattern, PatternAst, RecExpr, Rewrite, Runner, Var,
};
use pyo3::types::{PyList, PyString, PyTuple};
use pyo3::{basic::CompareOp, prelude::*};

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use crate::lang::{PythonAnalysis, PythonApplier, PythonNode};
use crate::util::{build_node, build_pattern};

#[pyclass]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PyId(pub Id);

#[pymethods]
impl PyId {
    fn __richcmp__(&self, other: Self, op: CompareOp) -> bool {
        match op {
            CompareOp::Lt => self.0 < other.0,
            CompareOp::Le => self.0 <= other.0,
            CompareOp::Eq => self.0 == other.0,
            CompareOp::Ne => self.0 != other.0,
            CompareOp::Gt => self.0 > other.0,
            CompareOp::Ge => self.0 >= other.0,
        }
    }
}

#[pyclass]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PyVar(pub Var);

#[pymethods]
impl PyVar {
    #[new]
    fn new(str: &PyString) -> Self {
        Self::from_str(str.to_string_lossy().as_ref())
    }

    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish().into()
    }

    fn __richcmp__(&self, other: Self, op: CompareOp) -> bool {
        match op {
            CompareOp::Lt => self.0 < other.0,
            CompareOp::Le => self.0 <= other.0,
            CompareOp::Eq => self.0 == other.0,
            CompareOp::Ne => self.0 != other.0,
            CompareOp::Gt => self.0 > other.0,
            CompareOp::Ge => self.0 >= other.0,
        }
    }
}

impl PyVar {
    pub fn from_str(str: &str) -> Self {
        let v = format!("?{}", str);
        PyVar(v.parse().unwrap())
    }
}

#[pyclass]
pub struct PyPattern {
    pub pattern: Pattern<PythonNode>,
}

#[pyclass]
pub struct PyRewrite {
    pub rewrite: Rewrite<PythonNode, PythonAnalysis>,
}

#[pymethods]
impl PyRewrite {
    #[new]
    #[args(name = "\"\"")]
    fn new(searcher: PyPattern, applier: &PyAny, name: &str) -> Self {
        let rewrite = if applier.is_callable() {
            let applier = PythonApplier {
                eval: applier.into(),
                vars: searcher.pattern.vars(),
            };
            Rewrite::new(name, searcher.pattern, applier).unwrap()
        } else if let Ok(pat) = applier.extract::<PyPattern>() {
            Rewrite::new(name, searcher.pattern, pat.pattern).unwrap()
        } else {
            panic!("Applier must be a pattern or callable");
        };
        PyRewrite { rewrite }
    }

    #[getter]
    fn name(&self) -> &str {
        self.rewrite.name.as_str()
    }
}

impl<'source> FromPyObject<'source> for PyPattern {
    fn extract(obj: &'source PyAny) -> PyResult<Self> {
        let mut ast = PatternAst::default();
        build_pattern(&mut ast, obj);
        let pattern = Pattern::from(ast);
        Ok(Self { pattern })
    }
}

#[pyclass(subclass)]
pub struct PyEGraph {
    pub egraph: EGraph<PythonNode, PythonAnalysis>,
}

#[pymethods]
impl PyEGraph {
    #[new]
    fn new(eval: Option<PyObject>) -> Self {
        Self {
            egraph: EGraph::new(PythonAnalysis { eval }),
        }
    }

    fn add(&mut self, expr: &PyAny) -> PyId {
        PyId(build_node(&mut self.egraph, expr))
    }

    #[args(exprs = "*")]
    fn union(&mut self, exprs: &PyTuple) -> bool {
        assert!(exprs.len() > 1);
        let mut exprs = exprs.iter();
        let id = self.add(exprs.next().unwrap()).0;
        let mut did_something = false;
        for expr in exprs {
            let added = self.add(expr);
            did_something |= self.egraph.union(id, added.0);
        }
        did_something
    }

    #[args(exprs = "*")]
    fn equiv(&mut self, exprs: &PyTuple) -> bool {
        assert!(exprs.len() > 1);
        let mut exprs = exprs.iter();
        let id = self.add(exprs.next().unwrap()).0;
        let mut all_equiv = true;
        for expr in exprs {
            let added = self.add(expr);
            all_equiv &= added.0 == id
        }
        all_equiv
    }

    fn rebuild(&mut self) -> usize {
        self.egraph.rebuild()
    }

    #[args(iter_limit = "10", time_limit = "10.0", node_limit = "100_000")]
    fn run(
        &mut self,
        rewrites: &PyList,
        iter_limit: usize,
        time_limit: f64,
        node_limit: usize,
    ) -> PyResult<()> {
        let refs = rewrites
            .iter()
            .map(FromPyObject::extract)
            .collect::<PyResult<Vec<PyRef<PyRewrite>>>>()?;
        let egraph = std::mem::take(&mut self.egraph);
        let scheduled_runner = Runner::<PythonNode, PythonAnalysis>::default();
        let runner = scheduled_runner
            .with_iter_limit(iter_limit)
            .with_node_limit(node_limit)
            .with_time_limit(Duration::from_secs_f64(time_limit))
            .with_egraph(egraph)
            .run(refs.iter().map(|r| &r.rewrite));

        self.egraph = runner.egraph;
        Ok(())
    }

    #[args(exprs = "*")]
    fn extract(&mut self, py: Python, exprs: &PyTuple) -> Vec<PyObject> {
        let ids: Vec<Id> = exprs.iter().map(|expr| self.add(expr).0).collect();
        let extractor = Extractor::new(&self.egraph, AstSize);
        ids.iter()
            .map(|&id| {
                let (_cost, recexpr) = extractor.find_best(id);
                reconstruct(py, &recexpr)
            })
            .collect()
    }

    fn dump(&self) -> PyResult<()> {
        let dump = self.egraph.dump();
        println!("{:?}", dump);
        Ok(())
    }
    fn pretty_dump(&self, py: Python) -> PyResult<String> {
        use egg::{AstSize, Extractor, Id};
        use pyo3::types::{PyString, PyTuple};

        let extractor = Extractor::new(&self.egraph, AstSize);
        let mut out = String::new();

        // Helper: reconstruct child id minimally
        let reconstruct_child = |child_id: Id| {
            let (_cost, expr) = extractor.find_best(child_id);
            reconstruct(py, &expr)
        };

        for eclass in self.egraph.classes() {
            let id: Id = eclass.id;
            out.push_str(&format!("{}: [", usize::from(id)));
            let mut first = true;

            for node in &eclass.nodes {
                if !first {
                    out.push_str(", ");
                }
                first = false;

                // Default label: class name + arity
                let mut label = {
                    let class_str = match node.class.as_ref(py).str() {
                        Ok(s) => s.to_str().unwrap_or("<?>").to_string(),
                        Err(_) => "<class>".to_string(),
                    };
                    format!("{}(children={})", class_str, node.children.len())
                };

                // Heuristic for detective.ir.Operation (5 fields: name,args,regions,attributes,result_types)
                if node.children.len() == 5 {
                    let name_obj = reconstruct_child(node.children[0]);
                    // Extract string for op name if possible
                    let name_s = name_obj
                        .cast_as::<PyString>(py)
                        .ok()
                        .map(|s| s.to_str().unwrap_or("<?>").to_string())
                        .unwrap_or_else(|| {
                            // fallback to str(name_obj)
                            name_obj
                                .as_ref(py)
                                .str()
                                .map(|s| s.to_str().unwrap_or("<?>").to_string())
                                .unwrap_or_else(|_| "<?>".to_string())
                        });

                    // lengths: args (tuple), regions (tuple), attributes (tuple), result_types (tuple)
                    let tuple_len = |child_id: Id| -> Option<usize> {
                        let obj = reconstruct_child(child_id);
                        obj.cast_as::<PyTuple>(py).ok().map(|t| t.len())
                    };
                    let args_len = tuple_len(node.children[1]).unwrap_or(0);
                    let regions_len = tuple_len(node.children[2]).unwrap_or(0);
                    let attrs_len = tuple_len(node.children[3]).unwrap_or(0);
                    let results_len = tuple_len(node.children[4]).unwrap_or(0);

                    label = format!(
                        "Operation(name='{}', args={}, regions={}, attrs={}, results={})",
                        name_s, args_len, regions_len, attrs_len, results_len
                    );
                } else if node.children.len() == 3 {
                    // Heuristic: linalg/yield or arith ops often appear as 3-field NamedTuples (for region payloads).
                    // We can limit to op name only to avoid value explosions.
                    // Attempt to reconstruct first child (name-like) if it's a string.
                    let maybe_name = {
                        let obj = reconstruct_child(node.children[0]);
                        obj.cast_as::<PyString>(py)
                            .ok()
                            .map(|s| s.to_str().unwrap_or("<?>").to_string())
                    };
                    if let Some(name_s) = maybe_name {
                        label = format!("{}(children={})", name_s, node.children.len());
                    }
                }

                out.push_str(&label);
            }
            out.push_str("]\n");
        }
        Ok(out)
    }

    /// Return the e-class id for a given expression by adding it (idempotent).
    fn class_id_for(&mut self, expr: &PyAny) -> PyId {
        self.add(expr)
    }

    /// Return compact labels for operations in an e-class.
    /// ops_only: omit non-operation nodes; include_bodies: summarize linalg.generic body ops.
    #[args(ops_only = "true", include_bodies = "true")]
    fn class_ops(
        &self,
        py: Python,
        id: PyId,
        ops_only: bool,
        include_bodies: bool,
    ) -> PyResult<Vec<String>> {
        use egg::{AstSize, Extractor, Id};
        use pyo3::types::{PyString, PyTuple};

        let extractor = Extractor::new(&self.egraph, AstSize);
        let eclass = &self.egraph[id.0];
        let mut out: Vec<String> = Vec::new();

        let reconstruct_child = |child_id: Id| {
            let (_cost, expr) = extractor.find_best(child_id);
            reconstruct(py, &expr)
        };

        for node in &eclass.nodes {
            if node.children.len() == 5 {
                // detective.ir.Operation
                let name_obj = reconstruct_child(node.children[0]);
                let name_s = name_obj
                    .cast_as::<PyString>(py)
                    .ok()
                    .map(|s| s.to_str().unwrap_or("<?>").to_string())
                    .unwrap_or_else(|| name_obj.as_ref(py).str().map(|s| s.to_str().unwrap_or("<?>").to_string()).unwrap_or_else(|_| "<?>".to_string()));

                let tuple_len = |child_id: Id| -> Option<usize> {
                    let obj = reconstruct_child(child_id);
                    obj.cast_as::<PyTuple>(py).ok().map(|t| t.len())
                };
                let args_len = tuple_len(node.children[1]).unwrap_or(0);
                let regions_len = tuple_len(node.children[2]).unwrap_or(0);
                let results_len = tuple_len(node.children[4]).unwrap_or(0);

                if include_bodies && name_s == "linalg.generic" {
                    // Try to summarize inner body ops as addf/mulf tokens
                    let mut tokens: Vec<String> = Vec::new();
                    let regions_obj = reconstruct_child(node.children[2]);
                    if let Ok(regions_tuple) = regions_obj.cast_as::<PyTuple>(py) {
                        if let Ok(region_obj) = regions_tuple.get_item(0) {
                            if let Ok(blocks_obj) = region_obj.getattr("blocks") {
                                if let Ok(blocks_tuple) = blocks_obj.cast_as::<PyTuple>() {
                                    if let Ok(block_obj) = blocks_tuple.get_item(0) {
                                        if let Ok(ops_obj) = block_obj.getattr("ops") {
                                            if let Ok(ops_tuple) = ops_obj.cast_as::<PyTuple>() {
                                                for i in 0..ops_tuple.len() {
                                                    if let Ok(op_obj) = ops_tuple.get_item(i) {
                                                        if let Ok(n) = op_obj.getattr("name") {
                                                            if let Ok(s) = n.str() {
                                                                let t = s.to_str().unwrap_or("");
                                                                if t.contains("arith.addf") { tokens.push("addf".to_string()); }
                                                                if t.contains("arith.mulf") { tokens.push("mulf".to_string()); }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    tokens.sort();
                    tokens.dedup();
                    if tokens.is_empty() {
                        out.push(format!("linalg.generic(args={}, regions={}, res={})", args_len, regions_len, results_len));
                    } else {
                        out.push(format!("linalg.generic{{body: {}}}", tokens.join("+")));
                    }
                } else {
                    out.push(format!("{} (args={}, res={})", name_s, args_len, results_len));
                }
            } else if !ops_only {
                // Fallback compact label
                let class_str = match node.class.as_ref(py).str() {
                    Ok(s) => s.to_str().unwrap_or("<?>").to_string(),
                    Err(_) => "<class>".to_string(),
                };
                out.push(format!("{}(children={})", class_str, node.children.len()));
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    /// Describe an e-class by id with compact operation labels.
    #[args(ops_only = "true", include_bodies = "true")]
    fn describe_class(
        &self,
        py: Python,
        id: PyId,
        ops_only: bool,
        include_bodies: bool,
    ) -> PyResult<String> {
        let labels = self.class_ops(py, id, ops_only, include_bodies)?;
        Ok(format!("{}: [{}]", usize::from(id.0), labels.join(", ")))
    }

    /// Return all current e-class ids.
    fn class_ids(&self) -> Vec<PyId> {
        self.egraph
            .classes()
            .map(|ec| PyId(ec.id))
            .collect::<Vec<_>>()
    }

    /// Reconstruct concrete Python objects for each enode in an e-class
    fn class_enodes(&self, py: Python, id: PyId) -> PyResult<Vec<PyObject>> {
        use egg::{AstSize, Extractor, Id};
        let extractor = Extractor::new(&self.egraph, AstSize);
        let eclass = &self.egraph[id.0];
        let reconstruct_child = |child_id: Id| {
            let (_cost, expr) = extractor.find_best(child_id);
            reconstruct(py, &expr)
        };
        let mut out: Vec<PyObject> = Vec::with_capacity(eclass.nodes.len());
        for node in &eclass.nodes {
            let obj = node.to_object(py, |child_id| reconstruct_child(child_id));
            out.push(obj);
        }
        Ok(out)
    }
}
pub(crate) fn reconstruct(py: Python, recexpr: &RecExpr<PythonNode>) -> PyObject {
    let mut objs = Vec::<PyObject>::with_capacity(recexpr.as_ref().len());
    for node in recexpr.as_ref() {
        let obj = node.to_object(py, |id| objs[usize::from(id)].clone());
        objs.push(obj)
    }
    objs.pop().unwrap()
}
