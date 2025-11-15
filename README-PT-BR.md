# Molang Compiler & Runtime (PT-BR)

## Visão Geral

- Implementação em Rust do tempo de execução Molang com dois motores: JIT (Cranelift) para expressões puras e interpretador para blocos/loops.
- Gramática segue o spec do Bedrock (identificadores case-insensitive, ternário, `??`, `loop`, `for_each`, blocos `{}`).
- `RuntimeContext` expõe namespaces `temp.`, `variable.`, `context.`, suportando números, strings e arrays.
- Builtins disponíveis: `math.cos/sin/abs/random/random_integer/clamp/sqrt/floor/ceil/round/trunc`.

## Funcionalidades Suportadas

- Operadores numéricos, `?:`, `??`, `&&/||/!`, literais numéricos/strings/arrays/structs.
- Namespaces com caminhos pontuados (`temp.foo.bar`) e `query.*`.
- Blocos com várias declarações; `loop`, `for_each`, `break`, `continue`, `return`.
- Literais de struct `{ x: 1 }`, atribuições encadeadas (`temp.location.z = 3`) e arrays com indexação (`temp.values[i]`) e `.length`.
- Cache JIT para expressões puras (reaproveita o código nativo compilado).
- Namespace `query.*` pode receber valores via `RuntimeContext::with_query("foo", valor)`.

## Não Suportado

- Recursos específicos do Minecraft (textures/geometry/queries avançados).
- Operador `->`, referências de entidades, operadores experimentais.
- Persistência de `variable.` entre execuções, operações avançadas em arrays (slice/mutação).

## Notas de Comportamento

- Apenas expressões puras (sem blocos/arrays/strings/flow) passam pelo JIT; o restante usa o interpretador.
- `math.random` usa `SmallRng` global com mutex.
- `loop` é limitado a 1024 iterações.
- `??` trata apenas `Value::Null` como ausente.

## Exemplos

```molang
temp.location = { x: 1, y: 2 };
temp.location.z = 3;
return temp.location.x + temp.location.y + temp.location.z;  # -> 6
```

```molang
temp.counter = 0;
loop(10, {
  temp.counter = temp.counter + 1;
  (temp.counter > 5) ? break;
});
return temp.counter;      # -> 6
```

```molang
temp.values = [2, 4, 6, 8];
temp.total = 0;
for_each(temp.item, temp.values, {
  temp.total = temp.total + temp.item;
  (temp.item >= 6) ? break;
});
return temp.total;        # -> 12

temp.index = 1;
return temp.values[temp.index] + temp.values[3] + temp.values.length;  # -> 33
```

```molang
math.clamp(math.random(0, 5), 1, 4) ?? 2
```

## Uso

```bash
cargo test

cargo run -- "return math.sqrt(16);"
```

Em Rust é possível injetar queries:

```rust
let mut ctx = RuntimeContext::default()
    .with_query("speed", 2.5)
    .with_query("offset", -1.0);
let value = evaluate_expression("query.speed + math.abs(query.offset)", &mut ctx).unwrap();
```
