use crate::ir::IrExpr;
use crate::jit::{self, CompiledExpression};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

thread_local! {
    static CACHE: RefCell<HashMap<String, Arc<CompiledExpression>>> =
        RefCell::new(HashMap::new());
}

/// Looks up or compiles a pure expression and stores it in a thread-local cache.
pub fn compile_cached(key: &str, ir: &IrExpr) -> Result<Arc<CompiledExpression>, jit::JitError> {
    if let Some(existing) = CACHE.with(|cache| cache.borrow().get(key).cloned()) {
        return Ok(existing);
    }

    let compiled = Arc::new(jit::compile_expression(ir)?);
    CACHE.with(|cache| {
        cache.borrow_mut().insert(key.to_string(), compiled.clone());
    });
    Ok(compiled)
}

#[cfg(test)]
pub fn cache_size() -> usize {
    CACHE.with(|cache| cache.borrow().len())
}

#[cfg(test)]
pub fn clear_cache() {
    CACHE.with(|cache| cache.borrow_mut().clear());
}
