# Internals Molang (PT-BR)

## Fluxo Geral

1. **Lexing** (`lexer.rs`) – transforma texto em `Token`s (identificadores, números, operadores, strings, pontuação).
2. **Parsing** (`parser.rs`) – monta `Program` com `Statement`s (expressões, atribuições, blocos, `loop`, `for_each`, `return`). Expressões viram árvores `Expr`.
3. **Redução IR** (`lib.rs`) – `evaluate_expression` verifica se o programa é uma expressão única sem controle de fluxo (`Program::as_jit_expression`):
   - Se sim → compilação JIT com cache via `jit_cache`
   - Se não → compilação JIT sob demanda via `jit::compile_program`
4. **Caminho JIT para Expressões** (com cache):
   - `IrBuilder` (`ir.rs`) reduz a AST da expressão para `IrExpr`.
   - `jit_cache` armazena expressões compiladas em um mapa thread-local indexado pelo código-fonte original. Se não estiver em cache, `jit::compile_expression` (Cranelift) constrói uma função que lê variáveis necessárias dos slots de runtime.
   - `CompiledExpression::evaluate` executa o código nativo compilado via JIT e retorna o `f64` resultante.
5. **Caminho JIT para Programas** (sob demanda):
   - `IrBuilder` reduz o programa inteiro para `IrProgram` com IR em nível de declaração.
   - `jit::compile_program` compila todas as declarações, controle de fluxo, loops e expressões para código de máquina nativo.
   - Suporta: `loop()` com break/continue, `for_each()` com binding de elementos, operações em arrays, literais de struct, atribuições de string.
6. **Builtins** – Funções `math.*` são compiladas via JIT para chamadas nativas diretas usando helpers de `builtins.rs`. Um RNG global (protegido por mutex) fornece aleatoriedade thread-safe. As funções são registradas via `BuiltinFunction::symbol_name` para resolução de símbolos no Cranelift.

## Contexto & Valores

- `RuntimeContext` guarda `HashMap<QualifiedName, Value>`; namespaces inferidos (`temp`, `variable`, `context`, `query`).
- `Value::truthy` segue Molang (0 → falso, string/array/struct vazios → falso). Arrays viram o tamanho ao virar `f64` e structs usam `IndexMap`, portanto `temp.location.z = 3` atualiza `temp.location` inteiro. Valores `query.*` são injetados via `RuntimeContext::with_query(...)`.
- Código compilado via JIT acessa o contexto de runtime através de helpers FFI (funções `molang_rt_*`) que leem e escrevem valores com segurança.

## Detalhes do Cranelift JIT

### IR de Expressões
- `IrExpr` suporta: constantes, paths, operadores unários/binários, condicionais, chamadas builtin, arrays, structs, strings, indexação e controle de fluxo.
- Quando usados como valores (não atribuições), arrays retornam seu comprimento, permitindo que `return [1,2,3]` produza `3.0`.

### IR de Declarações
- `IrStatement` inclui: atribuições, blocos, loops, for_each, return e declarações de expressão.
- `loop(count, body)` compila para loop nativo com blocos header/body/increment e suporte a break/continue.
- `for_each(var, collection, body)` compila para iteração de array com cópia de elementos via `molang_rt_array_copy_element`.
- Controle de fluxo (`break`/`continue`) compila para jumps diretos para blocos apropriados rastreados via pilha `LoopContext`.

### Geração de Código
- `jit.rs` traduz IR para CLIF via `Translator`. Cada variável referenciada vira um índice de slot.
- Builtins são declarados através de `BuiltinFunction::symbol_name` e registrados com o builder JIT do Cranelift (`register_builtin_symbols`).
- `jit_cache` cacheia `Arc<CompiledExpression>` por thread para evitar recompilação de expressões puras.

### Helpers de Runtime
- Todas as funções compiladas recebem parâmetros `(RuntimeContext*, RuntimeSlot*)`.
- `RuntimeSlot` é uma tabela em tempo de compilação armazenando strings de path canônicos (`temp.speed`, `query.foo`, etc.).
- Helpers FFI (`molang_rt_*`) fornecem acesso seguro a valores de runtime:
  - `molang_rt_get_number` / `molang_rt_set_number` - acesso a variáveis numéricas
  - `molang_rt_set_string` - atribuição de literal string (via dados globais)
  - `molang_rt_array_push_number` / `molang_rt_array_push_string` - construção de arrays
  - `molang_rt_array_get_number` - acesso a elemento de array
  - `molang_rt_array_length` - consultas de comprimento de array
  - `molang_rt_array_copy_element` - suporte a iteração de arrays
  - `molang_rt_copy_value` - atribuição variável-para-variável
  - `molang_rt_clear_value` - deleção de variável

### Estratégia de Atribuição
- Atribuições numéricas simples usam `molang_rt_set_number`
- Literais string compilam para objetos de dados globais com chamadas `molang_rt_set_string`
- Literais array limpam o slot alvo e então fazem push de elementos via helpers tipados
- Literais struct atribuem cada campo individualmente (construindo structs pais automaticamente)
- Cópias path-para-path otimizam para `molang_rt_copy_value` ao invés de load+store

## Testes

- `cargo test` roda testes que cobrem loops, `continue`, `for_each`, helpers, literais e cache.

## Exemplo Completo

Script:

```molang
temp.values = [1, 2, 3, 4];
temp.total = 0;
for_each(temp.item, temp.values, {
  temp.total = temp.total + temp.item;
});
return temp.total;
```

1. **Lexing**
   - Tokens: `Identifier(temp)`, `Dot`, `Identifier(values)`, `Equal`, `[`, `Number(1)`, `,`, `Number(2)`, `,`, `Number(3)`, `,`, `Number(4)`, `]`, `;`, … etc.
2. **Parsing**
   - Declarações:
     1. `Assignment` alvo `["temp","values"]`, valor `Expr::Array([...nós Number...])`
     2. `Assignment` alvo `["temp","total"]`, valor `Expr::Number(0)`
     3. `ForEach`:
        - path da variável `["temp","item"]`
        - coleção `Expr::Path(["temp","values"])`
        - corpo `Block` com uma única `Assignment` que adiciona `temp.item` a `temp.total`
     4. `Return(Expr::Path(["temp","total"]))`
3. **Redução IR**
   - `Program::as_jit_expression` retorna `None` (múltiplas declarações), então usamos `jit::compile_program`.
   - `IrBuilder::lower_program` produz `IrProgram` com 4 declarações.
4. **Compilação JIT**
   - Primeira atribuição: `assign_expression` limpa slot `temp.values`, então faz push de 4 elementos numéricos via `molang_rt_array_push_number`.
   - Segunda atribuição: Traduz para `molang_rt_set_number(temp.total, 0.0)`.
   - ForEach: Compila para:
     - Chama `molang_rt_array_length(temp.values)` para obter contagem
     - Cria blocos de loop (header, body, increment, exit)
     - Corpo do loop chama `molang_rt_array_copy_element` para bind do elemento atual para `temp.item`
     - Traduz declaração do corpo (carrega `temp.total`, carrega `temp.item`, adiciona, armazena)
     - Bloco de incremento avança contador do loop
   - Return: Carrega `temp.total` via `molang_rt_get_number` e retorna.
5. **Execução**
   - `CompiledExpression::evaluate` chama a função compilada via JIT com ponteiro RuntimeContext.
   - Código nativo executa, chamando helpers de runtime conforme necessário.
   - Resultado final: `10.0`

### Exemplo de Expressão Pura

Para uma expressão pura como `return temp.total + math.clamp(math.random(0, 5), 1, 4);`:
- Parser gera um único `Expr::Binary`
- Redução constrói uma árvore `IrExpr::Binary` com chamadas builtin (`FunctionRef::Builtin(MathClamp)`)
- Cranelift emite código de máquina que:
  1. Chama `molang_rt_get_number` para `temp.total`
  2. Chama `molang_builtin_random` e `molang_builtin_clamp` (chamadas de função diretas)
  3. Adiciona os resultados
  4. Retorna o `f64` final
- Resultado é cacheado para avaliações subsequentes com o mesmo texto-fonte
