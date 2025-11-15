# TODO – Full JIT Pipeline

- [ ] **Value lowerings**  
  - Add `Translator::assign_expression` that can emit:  
    - numeric stores (current `store_number`).  
    - string literals via `molang_rt_set_string`.  
    - array literals by allocating a temp slot, clearing it, translating each element, and calling the appropriate `molang_rt_array_push_*` helper.  
    - struct literals by synthesizing temporary slots per field (respecting insertion order) and copying the values into the final target slot.  
  - Update `IrExpr::Array/Struct/String/Flow/Index` handling in `translate()` to route through the new assignment path (rather than throwing `UnsupportedExpression`).

- [ ] **Index expressions & array ops**  
  - Lower `IrExpr::Index` to runtime helper calls.  
  - Support `array.length` reads by detecting `.length` suffixes during load.  
  - Ensure assignments such as `temp.values[temp.idx] = ...` rewrite to slot copies (likely via `array_copy_element` + setter helpers).

- [ ] **Control flow statements**  
  - Implement `IrStatement::Loop` lowering: translate the loop counter, build the iteration blocks, track loop-local `break`/`continue` destinations, and emit runtime slot copies as necessary.  
  - Implement `IrStatement::ForEach`: translate the collection expression, use `array_length`/`array_copy_element` helpers, and integrate with the same break/continue infrastructure.

- [ ] **Flow expressions (`IrExpr::Flow`)**  
  - Replace the current error with lowering that branches to the active loop’s break/continue block.  
  - Maintain a stack of loop contexts in `Translator`.

- [ ] **Testing & cleanup**  
  - Re-enable the high-level Molang tests (`loop`, `for_each`, struct literals, array indexing) and add targeted unit tests for the new helper flow.  
  - Once the translator consumes the helper functions, drop any remaining unused-field warnings and ensure `cargo test` passes.  
  - Document the helper ABI and new lowering behavior in `INTERNALS.md`.
