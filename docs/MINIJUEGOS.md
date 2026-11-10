# Minijuegos de Pokémon Reloaded 1.9.2.2 en el port de PS Vita

Fecha: 10/11/2026. Método: análisis del código decompilado (objetos de control, scripts y teclas de
cada minijuego) y ejecución de cada sala en el runner (`RLD_GOTO=<sala>` salta a ella tras llegar a la
casa; `RLD_GOTO_IDLE=1` la mide sin pulsar teclas). FPS de lógica medidos en PC sin gráficos (mismo
runner que la Vita); la estimación para Vita usa la proporción medida en consola (≈ ÷24).

## Resultados por minijuego

| Minijuego | Sala (id) | Control | Ratón/táctil | FPS PC → Vita estimado | Estado |
|---|---|---|---|---|---|
| Tragaperras | Tragaperras (312) | ControlTragaperras | No (Z/X/C, espacio) | 3292 → ~137 | ✅ |
| Póker / cartas | CardsRoom (265) | GameCardsControl | Opcional (botones en pantalla) | 2131 → ~89 | ✅ Apuestas con el diálogo numérico (flechas) |
| Puzle Unown | PuzleUNW (202) | ControlPuzle | **Obligatorio**: las piezas solo se mueven con clic | 3383 → ~141 | ⚠️ Necesita la pantalla táctil (ratón) |
| Carrera Pokémon | PokemonRace (99), RaceN1 (92) | ControlRace | No | 1122 / 671 → ~47 / ~28 | ✅ (RaceN1 al límite; mejorará con el renderizador en GPU) |
| FlyGol | PokemonFlyGol (300), _Playing (301) | ControlFlyGol, ControlTorneoFlygol | No (flechas, E) | 959 / 780 → ~40 / ~32 | ✅ |
| Dojo Marte | PokemonDojo (342), _Playing (343) | — | No (Z) | 942 / 1342 → ~39 / ~56 | ✅ |
| Zona Safari | ZonaSafari (145) y salas Safari | — (1235 instancias de hierba) | No | **382 → 571** (tras optimizar) → ~24 | ⚠️ Sala más pesada medida; ver "Rendimiento" |
| Casino | CasinoPolar (165) | — | No | 1075 → ~45 | ✅ |
| Torre de batalla | Torre_Piso (548), Torre_Batalla (550) | — | No | 2432 → ~101 | ✅ |
| Concurso de Bichos | (objeto ObjectConcursoBichos) | ConcursoBichos* | No | — | ✅ Sin funciones especiales (solo `sleep`) |
| Examen Universidad | (objeto ControlUniExam) | ControlUniExam* | No | — | ✅ Sin funciones especiales |
| Lotería | Ruta_22, PuertoFrente | obj_puestoloteria | No | — | ✅ Usa la fecha real (reloj de la Vita) |

Ninguna sala produjo errores del runner. Los dos errores que aparecieron al entrar *saltando*
(`C1` en RaceN1, `torre_estrato` en Torre_Piso) vienen de no pasar por el NPC que prepara el
minijuego (`DefineControlRace`, `GoTorreMesoHabla`): jugando normalmente no ocurren.

## Controles: cobertura de `vita/keymap.ini`

| Teclas del juego | Para qué | En la Vita |
|---|---|---|
| `key_vicio_accion1/2/3` = Z, X, C; `key_vicio_jugar` = espacio | Tragaperras | ✕, ○, □, SELECT |
| `key_poker_accion/cancel/reiniciar` = Z, X, espacio | Póker | ✕, ○, SELECT |
| `key_puzzle_cancel/reiniciar` = X, espacio | Puzle | ○, SELECT (+ táctil para las piezas) |
| `key_bt_escapar` / `key_bt_escapar_safari` = E | Salir de carrera/FlyGol/Safari | L + ○ |
| `key_bt_mov_*` = flechas | FlyGol, combate | Cruceta / stick |
| `key_bt_pkmn1..4` = 1..4 | Atajos en combate | Panel trasero (4 cuadrantes) |
| Diálogo numérico (`GetDialogNumber`: tiendas, banco, PC, apuestas) | Escribir cantidades | **Flechas**: ↑/↓ cambian la cifra, ←/→ cambian de cifra (no hace falta teclado numérico) |
| Diálogo de texto (`GetDialogString`: nombres) | Escribir | Teclado del sistema (IME), SELECT + △ |

`key_bt_pkmn5/6` existen pero el juego no los usa. Conclusión: todo se juega con botones salvo el
**puzle**, que exige la pantalla táctil como ratón (`mouse_x/mouse_y` escalados a la resolución del juego).

## Otros hallazgos

- **Imágenes informativas (`show_image`)**: científicos de los ataques (`data/image/at_*.png`), tipos,
  grupos huevo y ayudas de teclas usan ventanas de imagen a pantalla completa. Ahora mismo se omiten;
  hay que implementarlas con el renderizador (el port de Android lo tiene en `game/info_window.rs`).
- **Fecha y hora reales**: día/noche, eventos diarios y lotería usan la hora local. En Vita la crate
  `time` no conoce la zona horaria (daba UTC, 1 h de desfase en España); ahora se pide al reloj del
  sistema (`sceRtcConvertUtcToLocalTime`) y `runner.log` la muestra.
- **Errores no fatales**: el juego tiene "Abortar en todos los errores" desactivado, así que en Windows
  un error enseña un aviso y la partida sigue. El runner cerraba el juego. Ahora el error se apunta en
  `runner.log` y solo se abandona ese evento; leer una variable sin asignar vale 0. `RLD_STRICT=1`
  vuelve al comportamiento estricto para depurar.

## Rendimiento

La Zona Safari (1235 instancias) mostró que ~17 % del tiempo se iba en el hash de las tablas internas
(SipHash). Cambiado por FxHash en variables de instancia, alarmas, eventos y lista de instancias:

| Sala | Antes | Después |
|---|---|---|
| Zona Safari | 382 FPS | 571 FPS (+49 %) |
| Casa del protagonista | 2458 FPS | 3800 FPS (+55 %) |
| Recorrido completo del benchmark | 3,9 s | 3,2 s |

El resto del coste en salas grandes es el intérprete (`eval`, `event_inherited`: la hierba hereda
eventos de su padre). Siguiente paso posible: caché de la búsqueda de eventos heredados.
