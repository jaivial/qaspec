# 03. Roadmap

## Fase 0: Spike (validar el punto intermedio y el modelo de sesión)
- [ ] `qaspec-browser`: wrapper de `open/snapshot/click/fill/console/errors/network/eval/tab/state` con `--json`.
- [ ] Bucle mínimo de agente (1 goal) contra una página de prueba local.
- [ ] Ventanas de señales por paso (`--clear` antes de cada paso) y comprobar que no se pierden eventos.
- [ ] Medir el cambio de identidad (`state save` → `cookies clear` → `state load`) y el cambio de pestaña.
- [ ] Medir: memoria de 1 sesión y N pestañas, tiempo por acción y tiempo por goal frente a `e2e` (unos 12-17 s por paso de agente).
- **Aceptación**: un fichero con 3 pasos encadenados en una sola sesión. El 2.º paso parte del estado que dejó
  el 1.º, y el 3.º detecta un `console.error` y un `fetch` fallido ocurridos solo en ese paso.

## Fase 1: MVP (recorridos y sesión única)
- [ ] `qaspec.toml`: proyectos, entornos, `base_url`, identidades, secretos `{file|env|cmd}`, params y precedencia.
- [ ] Parser `.qa.ts`: `suite`, `step`, `goal`, `expect`, `capture` y `${...}`. `qaspec check` con errores con posición.
- [ ] Ejecución en orden con herencia de estado, `onFail` (`stop|continue`), `start` y `needs` entre pasos.
- [ ] **BrowserPool con `max_sessions = 1`**: un Chromium por ejecución y una pestaña por proyecto.
- [ ] Identidades: login spec, `state save/load` en `.qaspec/state/`, `valid_if` y re-login automático.
- [ ] Aserciones: `expect('<texto>')`, `console`, `errors`, `network`, `state`, `url`.
- [ ] Secretos con `fill_secret`, con redacción en todas las salidas.
- [ ] Reporte JSON (proyecto > fichero > suite > paso) y salida de terminal.
- **Aceptación**: las suites del proyecto interno (Neural + Cortex) se portan a **2 ficheros** (uno por
  proyecto) con varios pasos cada uno, sin selectores. La ejecución completa usa **un solo Chromium**
  (el pico de memoria del navegador queda por debajo de 2 GB) y **un login por identidad**.

## Fase 2: Proyectos interrelacionados y velocidad
- [ ] `depends_on`, `health` (`blocked` en cascada), `needs` entre ficheros y `capture` entre ficheros (`${neural.orders.orderId}`).
- [ ] Pasos con `project`/`as` distintos dentro de una suite (flujo cruzado), `via` para SSO e `[include]` de otros `qaspec.toml`.
- [ ] Planificador que agrupa por (proyecto, identidad) respetando el DAG.
- [ ] Replay cache (clave con el fingerprint de partida) + `batch --bail`, y self-healing. Login reproducido sin modelo.
- [ ] `--keep-open` y `qaspec watch` (navegador y sesiones calientes entre ejecuciones).
- [ ] Evaluación del juez en segundo plano mientras el navegador sigue con el siguiente paso.
- **Aceptación**:
  - flujo "publicar en Neural y verlo en Cortex" en una sola suite;
  - `qaspec run --project cortex` arrastra solo lo necesario de neural;
  - segunda ejecución con cache en **menos de 30 s**, **0 llamadas al modelo** y **0 logins**
    (estado reutilizado), frente a unos 3,5 min con e2e.

## Fase 3: Ergonomía
- [ ] `--from` / `--only` para retomar un recorrido en un paso.
- [ ] `suite.each` (datos en serie, mismo navegador).
- [ ] `qaspec explore` que genera borradores de spec con varios pasos.
- [ ] Formato `.qa.md` y `qaspec.d.ts` para el autocompletado.
- [ ] Reporte HTML con timeline, vídeo y screenshots. JUnit para CI.
- [ ] Aserciones `vitals`, `storage`, `cookie` y `react`. TOTP en identidades.

## Fase 4: Endurecimiento
- [ ] Recuperación ante una caída de Chromium (relanzar, recargar el estado y seguir).
- [ ] `--jobs N` opcional con guardia de memoria (unos 1,5 GB por sesión).
- [ ] Tests de contrato por versión de agent-browser.
- [ ] GitHub Action (cache del estado de auth entre jobs, `--cache=strict`).
- [ ] Documentación y publicación (`cargo install qaspec`).

## Métricas de éxito
| Métrica | e2e hoy | Objetivo qaspec |
|---|---|---|
| Ficheros para la suite neural + cortex | 4 (2 tests + 2 setups) | 2 + `qaspec.toml` |
| Líneas por comprobación | unas 15-30, con helpers y locators | 3-6, sin locators |
| Navegadores simultáneos | 1 contexto por test y workers en paralelo | 1 Chromium en toda la ejecución |
| Logins por ejecución | 1 setup por app y ejecución (20-90 s si lo hace el agente) | 0 si el estado sigue siendo válido; 1 replay si no |
| Suite completa, primera vez | unos 3,5 min | 2 min o menos |
| Suite completa, con cache | n/d | menos de 30 s, 0 llamadas |
| Detecta errores de consola y red por paso | No | Sí, por defecto |
| Proyectos interrelacionados | Targets independientes | `depends_on`, `needs`, captures y flujos cruzados |
| Dependencias de runtime | Node 22+, Playwright, AI SDK | binario Rust + agent-browser |

## Estado (v0.1.0)
Implementado de las fases 0-1: parser `.qa.ts`, `qaspec.toml` (proyectos, entornos, identidades,
secretos, params, `--set`), una sesión agent-browser por ejecución con pestaña por proyecto,
estado de login guardado y reutilizado, ventanas de señales por paso, aserciones deterministas y juez
LLM, `capture`, `onFail`/`needs`/`start`, `depends_on` + `health`, reportes JSON/JUnit.
Validado contra la suite real de Neural (5 pasos en un único recorrido, sesión reutilizada sin login).
Pendiente: replay cache, `needs` entre ficheros, `.qa.md`, `explore`, reporte HTML.
