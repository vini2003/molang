# Internals Molang (PT-BR)

## Fluxo Geral

1. **Lexing** (`lexer.rs`) – transforma texto em `Token`s (identificadores, números, operadores, strings, pontuação).
2. **Parsing** (`parser.rs`) – monta `Program` com `Statement`s (expressões, atribuições, blocos, `loop`, `for_each`, `return`). Expressões viram árvores `Expr`.
3. **Decisão** (`lib.rs`) – `evaluate_expression` verifica se o programa é apenas uma expressão JIT-compatível. Se sim, segue pelo JIT; caso contrário, usa o interpretador.
4. **Caminho JIT**
   - `IrBuilder` (`ir.rs`) reduz `Expr` a `IrExpr` (somente números/caminhos/unários/binários/condicionais/builtins).
   - `jit_cache` guarda `CompiledExpression` por thread, indexado pela string original.
   - `jit::compile_expression` usa Cranelift para gerar uma função que lê slots (`f64`) do contexto: cada caminho vira um índice.
5. **Interpretador**
   - `Executor` percorre as declarações mutando `RuntimeContext`, que armazena `Value` (número, string, array, struct, null) por `QualifiedName`.
   - Implementa `loop` (limitado a 1024), `for_each` sobre arrays, `break`/`continue`/`return`.
   - Expressões respeitam precedência, curto-circuito (`&&/||`), `??` (apenas `null` conta como vazio), literais de struct, indexação de arrays e `.length`, além das chamadas `BuiltinFunction::evaluate`.
6. **Builtins**
   - `builtins.rs` provê helpers (`math.random`, `clamp`, etc.), usando `SmallRng` com mutex e funções `extern "C"` para registrar na Cranelift.

## Contexto & Valores

- `RuntimeContext` guarda `HashMap<QualifiedName, Value>`; namespaces inferidos (`temp`, `variable`, `context`, `query`).
- `Value::truthy` segue Molang (0 → falso, string/array/struct vazios → falso). Arrays viram o tamanho ao virar `f64` e structs usam `IndexMap`, portanto `temp.location.z = 3` atualiza `temp.location` inteiro. Valores `query.*` são injetados via `RuntimeContext::with_query(...)`.

## Interpretador

- Usa `ControlSignal` (`None`, `Break`, `Continue`, `Return`) para propagar fluxo.
- `loop(count, body)` avalia `count`, clampa para `[0, 1024]` e executa o corpo.
- `for_each(var, collection, ...)` espera `Value::Array` e reatribui `var` a cada elemento.
- `eval_expr` retorna `(Value, ControlSignal)` e trata operadores (lógica, `??`, ternário, builtins), literais de struct, `expr[index]` (índices negativos viram 0, grandes dão wrap) e `array.length`.

## Cranelift JIT

- `IrExpr` exclui literais complexos/control flow. `Translator` cria CLIF e slots para variáveis.
- Slots são preenchidos em `CompiledExpression::evaluate` lendo `RuntimeContext::get_number`.
- Builtins registrados com `register_builtin_symbols`.
- Detalhe do ABI: cada função compilada recebe `(RuntimeContext*, RuntimeSlot*)`. Em tempo de compilação geramos uma tabela `RuntimeSlot` com as strings canônicas (`temp.speed`, `query.foo`). O código nativo carrega ponteiro + tamanho e chama `molang_rt_get_number`, que consulta o `RuntimeContext`. Assim o JIT permanece numérico, mas consegue resolver qualquer namespace em tempo de execução sem montar arrays temporários.

## Testes

- `cargo test` roda testes que cobrem loops, `continue`, `for_each`, helpers, literais e cache.

## Exemplo

Script:

```molang
temp.values = [1, 2, 3, 4];
temp.total = 0;
for_each(temp.item, temp.values, {
  temp.total = temp.total + temp.item;
});
return temp.total;
```

- **Tokens**: `Identifier(temp)`, `Dot`, `Identifier(values)`, `Equal`, `[`, `Number(1)`, `,`, … etc.
- **AST**: duas atribuições, um `ForEach` com corpo (`temp.total += temp.item`) e um `Return`.
- **Execução**: não é JIT-compatível → `Executor` executa sequencialmente, iterando sobre `Value::Array` e retornando `Value::Number(10)`.
- **Expressão Pura** (ex.: `return temp.total + math.clamp(math.random(0,5),1,4);`) → `IrExpr::Binary` → Cranelift gera função que lê `temp.total`, chama builtins, retorna `f64`.
