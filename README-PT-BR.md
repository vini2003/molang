# Molang Compiler & Runtime (PT-BR)

## Visão Geral

- Implementação em Rust do tempo de execução Molang com compilação JIT completa via Cranelift - todo código é compilado para código de máquina nativo.
- Gramática segue o spec do Bedrock (identificadores case-insensitive, ternário, `??`, `loop`, `for_each`, blocos `{}`).
- `RuntimeContext` expõe namespaces `temp.`, `variable.`, `context.`, suportando números, strings e arrays.
- Biblioteca matemática completa com 67+ funções incluindo trigonometria, easing, interpolação e mais (namespace math completo do Bedrock).
- REPL interativo com suporte multi-linha, histórico de comandos e destaque de sintaxe.

## Funcionalidades Suportadas

- Operadores numéricos, `?:`, `??`, `&&/||/!`, literais numéricos/strings/arrays/structs.
- Namespaces com caminhos pontuados (`temp.foo.bar`) e `query.*`.
- Blocos com várias declarações; `loop`, `for_each`, `break`, `continue`, `return`.
- Literais de struct `{ x: 1 }`, atribuições encadeadas (`temp.location.z = 3`) e arrays com indexação (`temp.values[i]`) e `.length`.
- Funções `math.*` compiladas em JIT para chamadas nativas diretas.
- Namespace `query.*` pode receber valores via `RuntimeContext::with_query("foo", valor)`.
- Cache JIT para expressões puras (reaproveita o código nativo compilado).
- Controle de fluxo: loops, for_each, break e continue todos compilados para instruções de controle de fluxo nativas.

## Funções Matemáticas

Todas as funções matemáticas são compiladas via JIT para chamadas nativas diretas para máximo desempenho.

### Funções Básicas
- `math.abs(x)` - Valor absoluto
- `math.floor(x)`, `math.ceil(x)`, `math.round(x)`, `math.trunc(x)` - Arredondamento
- `math.clamp(valor, min, max)` - Limita valor ao intervalo
- `math.max(a, b)`, `math.min(a, b)` - Mínimo/máximo
- `math.mod(valor, denominador)` - Módulo
- `math.sign(x)` - Retorna 1 se positivo, -1 caso contrário
- `math.copy_sign(a, b)` - Retorna `a` com o sinal de `b`
- `math.sqrt(x)` - Raiz quadrada
- `math.pi()` - Retorna a constante π

### Funções Trigonométricas (em graus)
- `math.cos(graus)`, `math.sin(graus)` - Cosseno e seno
- `math.acos(x)`, `math.asin(x)`, `math.atan(x)` - Funções trigonométricas inversas
- `math.atan2(y, x)` - Arco-tangente de dois argumentos

### Exponencial e Logarítmica
- `math.exp(x)` - e^x
- `math.ln(x)` - Logaritmo natural
- `math.pow(base, expoente)` - Função potência

### Funções Aleatórias
- `math.random(min, max)` - Float aleatório no intervalo
- `math.random_integer(min, max)` - Inteiro aleatório no intervalo
- `math.die_roll(num, min, max)` - Soma de `num` floats aleatórios
- `math.die_roll_integer(num, min, max)` - Soma de `num` inteiros aleatórios

### Funções de Ângulo
- `math.min_angle(graus)` - Normaliza ângulo para [-180, 180)

### Funções de Interpolação
- `math.lerp(inicio, fim, t)` - Interpolação linear
- `math.inverse_lerp(inicio, fim, valor)` - Interpolação linear inversa
- `math.lerprotate(inicio, fim, t)` - Interpolação de rotação mais curta
- `math.hermite_blend(t)` - Suavização Hermite: 3t² - 2t³

### Funções de Easing

Todas as funções de easing recebem parâmetros `(inicio, fim, t)` onde `t` está em [0,1].

**Quadrática**: `ease_in_quad`, `ease_out_quad`, `ease_in_out_quad`

**Cúbica**: `ease_in_cubic`, `ease_out_cubic`, `ease_in_out_cubic`

**Quártica**: `ease_in_quart`, `ease_out_quart`, `ease_in_out_quart`

**Quíntica**: `ease_in_quint`, `ease_out_quint`, `ease_in_out_quint`

**Senoidal**: `ease_in_sine`, `ease_out_sine`, `ease_in_out_sine`

**Exponencial**: `ease_in_expo`, `ease_out_expo`, `ease_in_out_expo`

**Circular**: `ease_in_circ`, `ease_out_circ`, `ease_in_out_circ`

**Back** (ultrapassagem): `ease_in_back`, `ease_out_back`, `ease_in_out_back`

**Elástica** (mola): `ease_in_elastic`, `ease_out_elastic`, `ease_in_out_elastic`

**Bounce** (quique): `ease_in_bounce`, `ease_out_bounce`, `ease_in_out_bounce`

## Não Suportado

- Recursos específicos do Minecraft (textures/geometry/queries avançados).
- Operador `->`, referências de entidades, operadores experimentais.
- Persistência de `variable.` entre execuções, operações avançadas em arrays (slice/mutação).

## Notas de Comportamento

- Todo código é compilado via JIT para código de máquina nativo - não há interpretador de fallback.
- Expressões puras são cacheadas; programas com declarações são compilados sob demanda.
- `math.random` usa `SmallRng` global com mutex.
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

### REPL Interativo

Execute sem argumentos para iniciar o REPL interativo:

```bash
cargo run --release
```

Funcionalidades:
- Entrada multi-linha com continuação `\`
- Histórico de comandos (setas ↑ e ↓)
- Comandos especiais: `:help`, `:vars`, `:clear`, `:exit`
- Destaque de sintaxe e saída colorida

Veja [REPL_DEMO.md](REPL_DEMO.md) para exemplos.

### Expressão Única

Avalie uma única expressão pela linha de comando:

```bash
cargo run -- "return math.sqrt(16);"
cargo run -- "temp.x = 5; temp.y = 10; return temp.x + temp.y"
```

### Executar Testes

```bash
cargo test
```

Em Rust é possível injetar queries:

```rust
let mut ctx = RuntimeContext::default()
    .with_query("speed", 2.5)
    .with_query("offset", -1.0);
let value = evaluate_expression("query.speed + math.abs(query.offset)", &mut ctx).unwrap();
```
