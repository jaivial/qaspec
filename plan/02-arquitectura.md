# 02. Arquitectura de qaspec

## Principios

1. **El spec describe intención, no implementación.** No hay locators ni `await`. Solo objetivos y comprobaciones.
2. **agent-browser es la única interfaz con el navegador.** Nada de Playwright, Puppeteer ni CDP directo.
   Cada acción es un comando `agent-browser --json ...` que queda registrado y se puede repetir.
3. **El agente ve todo lo que vería un QA y además lo técnico**: pantalla, consola, errores, red, storage y estado JS.
4. **Un solo navegador, siempre caliente.** Chromium es el recurso caro: un fichero de test es un
   recorrido largo con varias comprobaciones en orden, y toda la ejecución reutiliza el mismo navegador
   y las mismas sesiones autenticadas (ver sección 3).
5. **Configuración, no código**: base URLs, entornos, credenciales y relaciones entre proyectos viven en
   `qaspec.toml`. Los specs solo los nombran.
6. **Rust**: un binario y arranque instantáneo.

### Coste medido (agent-browser 0.27, esta máquina)

| Recurso | Coste |
|---|---|
| Sesión nueva de agent-browser (`--session X`) | **unos 1,5 GB de RSS** (un Chromium entero), unos 0,9 s en frío |
| Pestaña nueva dentro de la sesión | unos 128 MB |
| `open` con el navegador ya caliente | unos 0,2 s |
| `snapshot -i -c` | unos 0,15 s |

Las pestañas de una sesión **comparten cookies** (verificado), y `state save` / `state load` exportan e
importan cookies y storage en JSON. `e2e`, en cambio, abre un contexto nuevo por test y usa workers en
paralelo: aísla bien los tests, pero sale caro en memoria.

---

## 1. Formato de spec: un fichero, varias comprobaciones en orden

Sintaxis TypeScript (`*.qa.ts`) para tener resaltado y autocompletado con `qaspec.d.ts`. **Nunca se
ejecuta**: el runner la parsea con `oxc_parser` y la convierte en un plan declarativo. Solo se aceptan
las llamadas del DSL con literales y plantillas `${...}`. Cualquier otra cosa es un error con su posición.

```ts
import { suite, step, goal, expect, capture } from 'qaspec';

suite('SAGE en Neural', { project: 'neural', as: 'editor', start: '/' }, () => {

  step('abrir el asistente', () => {
    goal('open the side panel and switch to the SAGE tab');
    expect('the SAGE composer is visible');
  });

  // Continúa en el estado exacto que dejó el paso anterior (mismo navegador, misma pestaña).
  step('preguntar por Panorama', () => {
    goal('ask SAGE: ${params.question}');
    expect('SAGE answers and the answer explains the Panorama page');
    expect.network('POST /api/sage/**').status(200);
    expect.console.noErrors();                 // solo errores ocurridos DURANTE este paso
    capture('answer', 'the text of the last SAGE answer');
  });

  // `start` hace el paso recuperable: navega primero y no depende del estado anterior.
  step('las páginas principales cargan', { start: '/articles', onFail: 'continue' }, () => {
    goal('visit Articles, Analytics and Panorama from the navigation');
    expect('each page rendered its own content, none is blank');
    expect.errors.none();
  });

  step('la conversación sigue tras recargar', () => {
    goal('reload the page and reopen SAGE');
    expect('the previous answer "${answer}" is still in the conversation');
  });
});
```

### Semántica
- **`suite`** = un fichero o un recorrido. Define el proyecto, la identidad (`as`), la ruta inicial y
  los parámetros por defecto. Un fichero puede tener varias suites y se ejecutan en el orden del fichero.
- **`step`** = una unidad con veredicto propio (`passed|failed|blocked|skipped`). Los pasos se ejecutan
  **en orden** y **heredan el estado del navegador** del paso anterior: URL, formularios, storage y sesión.
- **Política ante fallos** (`onFail`):
  - `stop` (por defecto en pasos sin `start`): los pasos siguientes que dependen del estado se marcan `skipped`.
  - `continue`: se sigue con el siguiente paso. Si ese paso tiene `start`, se navega primero, así que
    vuelve a estar en un estado conocido.
  - `needs: ['nombre de paso']`: dependencia explícita, con lo que solo se saltan los pasos que dependen del que falló.
- **Ventanas de señales**: antes de cada paso, el runner lee y limpia (`--clear`) `console`, `errors` y
  `network requests`. Las aserciones `expect.console`, `expect.errors` y `expect.network` miran solo
  **lo ocurrido en ese paso**. Con `{ scope: 'suite' }` miran toda la suite.
- **Variables**: `capture(name, '<descripción>')` (lo extrae el agente),
  `capture.state(name, 'window.x.id')`, `capture.network(name, 'POST /api/orders', '$.id')` y
  `capture.url(name, /orders\/(\d+)/)`. Se usan después como `${name}` en goals y expects, incluso
  en otros ficheros de la misma ejecución (sección 4).

Tipos de aserción:

| Tipo | Quién la evalúa | Coste |
|---|---|---|
| `expect('<texto>')` | LLM juez: snapshot, señales del paso y screenshot opcional; debe citar evidencia | 1 llamada (cacheable) |
| `expect.console.*`, `expect.errors.*` | Rust, sobre `console/errors --json` | 0 |
| `expect.network(...)` | Rust, sobre `network requests --json` | 0 |
| `expect.state(js)`, `expect.storage.*`, `expect.cookie(...)` | Rust, sobre `eval` / `storage` / `cookies` | 0 |
| `expect.url(...)`, `expect.visible(...)` | Rust, sobre `get url` / `snapshot` | 0 |
| `expect.vitals.lcp.lt(2500)` | Rust, sobre `vitals --json` | 0 |

Las aserciones deterministas se evalúan primero. Si una falla, no se llama al juez.

Variante adicional: `*.qa.md` (frontmatter + `## paso` + listas `- goal:` / `- expect:`) que se compila al mismo AST.

---

## 2. Parámetros, entornos y credenciales (`qaspec.toml`)

```toml
default_env = "dev"

# ---- Proyectos -------------------------------------------------------------
[projects.neural]
specs    = "specs/neural/**/*.qa.ts"
health   = "/api/health"                     # si falla, todo el proyecto queda `blocked` (ENVIRONMENT_UNAVAILABLE)
[projects.neural.env.dev]
base_url = "https://neural.dev.example.com"
[projects.neural.env.local]
base_url = "http://localhost:5173"

[projects.cortex]
specs    = "specs/cortex/**/*.qa.ts"
depends_on = ["neural"]                       # orden: primero neural, y si neural está caído, cortex queda blocked
locale   = "pt-BR"                            # pista para el agente: la UI está en portugués
[projects.cortex.env.dev]
base_url = "https://cortex.dev.example.com"

# ---- Identidades (credenciales) ------------------------------------------------
[projects.neural.identities.editor]
username = "qa@neural.example.com"
password = { file = "~/.config/qaspec/neural-password" }   # o { env = "NEURAL_PW" } o { cmd = "pass show neural" }
login    = "specs/neural/_login.qa.ts"        # spec de login (agente o pasos deterministas)
valid_if = { url_not = "/login" }             # cómo saber si el estado guardado sigue valiendo
ttl      = "8h"

[projects.cortex.identities.tester]
username = "qa@cortex.example.com"
password = { file = "~/.config/qaspec/cortex-password" }
totp     = { file = "~/.config/qaspec/cortex-totp-seed" }   # opcional: el runner genera el código

# ---- Parámetros libres -----------------------------------------------------
[params]
question = "what is the Panorama page for?"
[env.local.params]
question = "¿para qué sirve Panorama?"

# ---- Modelo y runner -------------------------------------------------------
[llm]
provider = "openai-compatible"
base_url = "http://localhost:8317/v1"
api_key  = { file = "~/.config/qaspec/llm-key" }
actor    = "claude-haiku-4-5-20251001"
judge    = "claude-haiku-4-5-20251001"

[browser]
max_sessions = 1          # navegadores Chromium simultáneos (ver sección 3)
headed = false
```

**Precedencia** (de mayor a menor): `--set clave=valor` en la CLI, variables `QASPEC_*`
(p. ej. `QASPEC_PROJECTS__NEURAL__BASE_URL`), `[env.<env>]` / `[projects.X.env.<env>]`, los valores
base de `qaspec.toml` y, por último, los defaults de la suite en el spec.

**En los specs**: `${params.question}`, `${project.base_url}`, `${projects.cortex.base_url}`,
`${identity.username}`. Las rutas relativas (`start: '/articles'`) se resuelven contra el `base_url`
del proyecto de la suite.

**Secretos**: `password`, `totp`, `api_key` y cualquier valor `{ file|env|cmd }` es un *handle*. El
modelo solo ve `<secret:neural.editor.password>`. La herramienta `fill_secret` hace que Rust rellene el
campo, y el valor se redacta en el reporte, los logs y el timeline. `qaspec check` valida que todos los
secretos referenciados existen, sin leerlos en voz alta.

**Datos parametrizados**: `suite.each([{plan:'pro'},{plan:'team'}], 'upgrade to ${plan}', ...)` repite
la suite **en serie y en el mismo navegador**.

---

## 3. Modelo de ejecución: un navegador, sesiones reutilizadas

```
qaspec run
 └─ BrowserPool (max_sessions = 1 por defecto)
     └─ agent-browser --session qaspec-<run-id>      ← UN Chromium para toda la ejecución
         ├─ tab "neural"  (cookies de neural-dev…)
         └─ tab "cortex"  (cookies de cortex-dev…)
```

1. **Planificación.** El runner construye un DAG con los proyectos (`depends_on`), los ficheros
   (`needs` entre suites) y los pasos. Después **ordena para minimizar cambios de identidad**: agrupa
   las suites por `(proyecto, identidad)` sin romper ninguna dependencia.
2. **Un navegador caliente.** Se lanza una sola sesión agent-browser y se reutiliza para todos los
   ficheros. No se cierra entre tests. Nunca se abre otro Chromium salvo que `max_sessions > 1`.
3. **Una pestaña por proyecto** (`tab new --label <proyecto>`). Cada proyecto tiene su origen, sus
   cookies conviven en el mismo navegador y las cambia de pestaña cuestan unos 128 MB y casi nada de tiempo.
4. **Autenticación una sola vez y persistente entre ejecuciones.**
   - Primera vez: se ejecuta el spec `login` de la identidad y se hace
     `state save .qaspec/state/<env>/<proyecto>.<identidad>.json` (gitignored, modo 600).
   - Siguientes: `state load`, y se comprueba `valid_if` (una llamada `get url`/`eval`, unos 0,2 s). Si ha
     caducado, se vuelve a hacer el login y se guarda de nuevo, de forma automática.
   - Login sin modelo cuando se puede: el replay cache (sección 5) graba el login en la primera
     ejecución y en las siguientes lo repite con `batch` en 1-2 s.
5. **Cambio de identidad en el mismo proyecto** (p. ej. `admin` y luego `viewer` en Neural): no se abre
   otro navegador. agent-browser solo tiene `cookies clear` **global** (verificado en 0.27), así que el cambio es:
   `state save` de la identidad saliente, `cookies clear` + `storage local/session clear`, `state load` de
   **todas** las identidades activas de los demás proyectos y `state load` de la entrante. Cuesta menos de 1 s
   y por eso el planificador agrupa por identidad para hacerlo pocas veces. Si dos suites necesitan **a la
   vez** dos usuarios en el mismo origen (chat entre usuarios), es el único caso que pide una segunda sesión
   y queda limitado por `max_sessions`.
6. **Higiene entre suites** (`reset` en la suite, por defecto `navigate`):
   `none` (continúa justo donde quedó la anterior), `navigate` (va a `start`), `storage` (además
   limpia el local/session storage pero conserva las cookies de auth) y `state` (recarga el estado guardado de la identidad).
7. **Paralelismo opcional, nunca por defecto.** `--jobs N` (o `max_sessions`) solo con RAM de sobra.
   El runner estima unos 1,5 GB por sesión y se niega si `N × 1,5 GB` supera la memoria libre, salvo con `--force`.
   El paralelismo útil y barato es el del **modelo**: mientras el navegador ejecuta comandos, el juez de
   un paso anterior evalúa en segundo plano los expects que no necesitan pantalla (timeline y señales ya capturadas).
8. **Recuperación.** Si el daemon o Chromium muere, se relanza la sesión, se hace `state load` de las
   identidades activas, el paso en curso queda `blocked (BROWSER_CRASHED)` y se continúa por el siguiente paso con `start`.
9. **Modo `--keep-open`.** El navegador sigue vivo al terminar (`qaspec run --keep-open`). La siguiente
   ejecución se conecta a él (`agent-browser --session qaspec-dev`) y ahorra el arranque y el login. Es
   ideal en desarrollo local, con `qaspec watch`.

---

## 4. Proyectos interrelacionados

Casos que cubre:

| Caso | Cómo |
|---|---|
| **Orden y salud**: cortex usa la API de neural | `depends_on = ["neural"]`. Si `health` de neural falla, las suites de cortex quedan `blocked`, no `failed` |
| **SSO / identidad compartida** | `[projects.cortex.identities.tester] via = "idp.tester"`: el login se hace en el proyecto `idp` y las cookies del IdP en la misma sesión dan acceso a los dos |
| **Flujo que cruza aplicaciones** (crear en A y verificar en B) | Una suite puede cambiar de proyecto entre pasos: `step('...', { project: 'cortex', as: 'tester' }, ...)`. El runner cambia de pestaña |
| **Datos que viajan entre ficheros** | `capture('orderId', ...)` en `neural/orders.qa.ts`, y `suite(..., { needs: ['neural/orders.qa.ts'] })` en cortex usa `${neural.orders.orderId}` |
| **Ejecutar un subconjunto** | `qaspec run --project cortex` incluye solo lo necesario de neural (salud, login y suites de las que dependen los `needs`) |
| **Monorepo o repos separados** | `qaspec.toml` con `[include] paths = ["../neural/qaspec.toml"]`. Cada repo define sus proyectos y el workspace los une |

Ejemplo de flujo cruzado:
```ts
suite('un artículo publicado en Neural aparece en Cortex', { project: 'neural', as: 'editor', start: '/articles' }, () => {
  step('publicar', () => {
    goal('create and publish an article titled "qaspec ${run.id}"');
    capture.network('articleId', 'POST /api/articles', '$.id');
    expect.network('POST /api/articles').status(201);
  });
  step('verlo en cortex', { project: 'cortex', as: 'tester', start: '/' }, () => {
    goal('search for "qaspec ${run.id}"');
    expect('the article "qaspec ${run.id}" appears in the results');
  });
});
```
`${run.id}` es único por ejecución y evita choques con datos de ejecuciones anteriores.

---

## 5. Componentes (workspace Cargo)

```
crates/
  qaspec-cli/       # binario `qaspec`: run, check, watch, record, explore, report, state
  qaspec-config/    # qaspec.toml + includes + entornos + precedencia + handles de secretos
  qaspec-spec/      # parser .qa.ts/.qa.md -> AST -> Plan (suites, steps, captures, needs) con errores con posición
  qaspec-plan/      # DAG proyectos/ficheros/pasos, orden por identidad, política de fallos
  qaspec-browser/   # cliente agent-browser: UNA sesión, pestañas por proyecto, state save/load, ventanas de señales
  qaspec-agent/     # bucle del agente: observación -> LLM (tool calls) -> comando agent-browser
  qaspec-assert/    # evaluadores deterministas y juez LLM
  qaspec-cache/     # replay de goals verificados (por proyecto, identidad, paso y fingerprint de UI)
  qaspec-llm/       # Anthropic, OpenAI-compatible (gateway local), Ollama
  qaspec-report/    # JSON (schema estable), JUnit, Markdown, HTML con timeline/vídeo
```

### qaspec-browser
- Wrapper tipado sobre `agent-browser --json --session <run> <cmd>`. Lleva la cuenta de la pestaña
  activa por proyecto (`tab t<N>` / labels) y de la identidad cargada en cada origen.
- `begin_step()` limpia y marca las señales; `end_step()` recoge `console`, `errors`, `network requests`
  y el snapshot final, y los asocia al paso.
- Replay con `batch --bail` (una invocación por secuencia).
- Test de contrato por versión de agent-browser (hoy, 0.27).

### qaspec-agent
- **Herramientas que ve el modelo** (todas son 1 comando agent-browser): `snapshot`, `click`, `fill`,
  `type`, `press`, `select`, `scroll`, `open`, `back`, `wait`, `screenshot`, `console`, `errors`, `network`,
  `eval` (solo lectura), `storage`, `fill_secret(handle)`, `done(verdict, evidence)`.
- **Contexto del recorrido**: el prompt de cada goal incluye un resumen de los pasos anteriores de la suite
  (goal, veredicto y URL final). Así el agente sabe que ya está en el panel de SAGE y no lo busca otra vez.
- Presupuestos por goal, detección de bucles y veredicto `passed|failed|blocked` con códigos.

### qaspec-cache
- Clave: `(proyecto, identidad, fichero, suite, paso, fingerprint del snapshot inicial)`. Como el paso
  hereda el estado, la huella del punto de partida forma parte de la clave.
- Graba locators semánticos (`find role button "Upgrade"`), no los `@ref` efímeros.
- Si el replay falla, el agente retoma desde la última acción válida y se reescribe la grabación.
  `--cache=strict` en CI.

---

## 6. CLI

```
qaspec run [paths] [--env dev] [--project neural] [--set params.question=...] [--keep-open]
           [--cache=auto|strict|off] [--jobs 1] [--headed] [--from 'paso'] [--only 'paso']
qaspec check                 # parsea, valida config, needs, secretos y DAG sin navegador
qaspec watch                 # re-ejecuta al guardar, con el navegador caliente (keep-open)
qaspec state list|clear      # estados de auth guardados por env/proyecto/identidad
qaspec record <spec>         # navegador visible + grabación de cache
qaspec explore <url> "goal"  # QA libre (como dogfood) -> borrador de .qa.ts
qaspec report                # último reporte HTML
```
`--from` y `--only` vuelven a ejecutar desde un paso concreto. Los pasos previos se reproducen desde la
cache, o se salta directamente a un paso con `start`.

## 7. Reporte
Árbol `proyecto > fichero > suite > paso` con veredicto, duración, origen de cada paso (`replay` o `agent`),
llamadas y tokens, timeline (acción + señales), evidencia de cada expect, capturas (sin secretos) y vídeo
en los fallos. También se reportan los cambios de identidad y la memoria pico del navegador.
Salidas: JSON (`schemaVersion`), JUnit para CI y HTML.

## 8. Decisiones abiertas
- Parser: propuesta `oxc_parser`, frente a `swc`.
- Proceso por comando frente a conexión persistente al daemon de agent-browser: empezar con procesos y medir.
- Limpieza por origen: `cookies clear` es global. Como alternativa a "limpiar todo y volver a cargar el resto",
  se pueden expirar las cookies del dominio con `cookies set ... --expires 0`. Hay que medir las dos en la fase 0
  (las cookies `HttpOnly` no se ven desde `eval`, pero sí desde `cookies get`).
- ¿`eval` con efectos en `given` (seed)? Sí, pero solo en `given`, nunca desde el agente.
