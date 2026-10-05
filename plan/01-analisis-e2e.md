# 01. Análisis de `e2e` (tester.army)

Analizado: `e2e@0.16.0` + `@e2e-dev/web@0.11.2`, tal como se usa en un proyecto interno de dos apps (Neural y Cortex)
(suites de Neural y MythCortex, modelo `claude-haiku-4-5` vía gateway local).

## Cómo funciona

- **SDK + runner + CLI en Node** (unas 45k líneas JS). El motor web usa **Playwright**.
- Los tests son **TypeScript real** con estilo Playwright:
  ```ts
  test('a member upgrades to Pro', async ({ app, agent, screen }) => {
    await app.open('/settings/billing');
    await agent.act('upgrade the workspace to the Pro plan');
    await agent.assert('the invoice preview shows a prorated amount');
    await expect(screen.getByRole('status')).toContainText('Pro');
  });
  ```
- Pasos de agente: `agent.act`, `agent.assert`, `agent.waitFor`, `agent.extract`. Cada paso tiene un
  presupuesto de llamadas al modelo y un deadline. El veredicto es `passed`, `failed` o `blocked`, con
  códigos de error (`STEP_BUDGET_EXHAUSTED`, `ENVIRONMENT_UNAVAILABLE`...).
- **Observación**: snapshot de accesibilidad redactado (roles, nombres, textos, estados) más diffs tras
  cada acción. Screenshot opcional (`vision`).
- **Replay cache**: un `act` verificado por una aserción posterior se graba y se repite sin modelo
  hasta que la app cambia.
- Lo que está bien resuelto: secretos (`Secret`, el modelo nunca ve el valor), sesiones guardadas
  (`test.setup` + `session.save`), reportes JSON y Markdown, detección de bucles, executors propios,
  `explore` y `bug-bash`.

## Por qué no convence (y evidencia en el proyecto interno)

| Problema | Evidencia |
|---|---|
| **Lento de escribir.** Al final se vuelve a escribir Playwright: locators, `try/catch`, helpers y `waitFor`. | `tests/neural.e2e.ts`: `openSagePanel()` con `getByRole('tab','SAGE')`, timeout manual y fallback al toggle. El README admite que el login es "deterministic (locators, no model)" porque el agente se ahogaba. |
| **Lento de ejecutar.** | Último `report.json`: los pasos con agente tardan entre 12 y 17 s y la suite entera, unos 3,5 min para 8 tests. Login por agente: entre 20 y 90 s. |
| **El agente es miope.** Por diseño, el modelo **no ve** consola, red, cookies ni estado (`agent-steps.mdx`: "excludes raw HTML, cookies, headers"). | Un QA humano tampoco los ve, pero un test sí debería. `e2e` se queda con lo peor de los dos mundos. |
| **Aserciones débiles.** `agent.assert` juzga solo la pantalla actual, sin historial ni señales técnicas. | "La respuesta aparece" no detecta un `500` silencioso ni un `TypeError` en consola. |
| **Pesado.** Node 22.12+, Playwright, nvm, AI SDK y un proveedor `openai-compatible`. | `run.sh` hace `nvm use 24`. Los modelos que no hacen tool-calls (MiniMax) no sirven. |
| **Dos mundos mezclados.** Pasos deterministas y pasos de agente en el mismo TS. Hay que saber cuándo usar cada uno. | Es la principal fuente de lentitud al escribir. |

## Qué nos quedamos (ideas buenas)

1. Veredicto en tres estados (`passed`, `failed`, `blocked`) con códigos que separan un fallo del producto de un fallo del entorno.
2. Replay cache de acciones verificadas, para que el modelo solo intervenga cuando la app cambia.
3. Secretos como handles: el modelo nunca ve el valor.
4. Sesiones con nombre: el login se hace una vez.
5. Presupuestos por paso y detección de bucles.
6. Reporte JSON estable y legible por máquinas.

## El punto intermedio: agent-browser

agent-browser (v0.27, ya instalado en `/usr/bin/agent-browser`, con cliente Rust) da, en `--json`:

| Necesidad del QA | Comando |
|---|---|
| Ver la página | `snapshot -i -c` (árbol a11y con `@refs`), `screenshot --annotate` |
| Actuar | `click/fill/type/press/select/find ...` |
| **Consola** | `console`, `errors` |
| **Red** | `network requests --filter`, `network route` (mocks), `har start/stop` |
| **Estado** | `eval <js>`, `storage local/session`, `cookies`, `react tree/inspect` |
| Rendimiento | `vitals --json` |
| Sesión y auth | `--session`, `--session-name`, `--state`, `auth save/login` |
| Varios pasos de una vez | `batch --bail` |
| Evidencia | `record start/stop`, `trace`, `diff snapshot` |

Lo verifiqué en local: tras hacer clic en un botón que hace `console.error('boom')` y un `fetch` que
falla, `console`, `errors`, `network requests` y `eval window.appState.n` devuelven todos JSON
estructurado. **Es justo la capa que le falta a `e2e`.**
