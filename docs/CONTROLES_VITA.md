# Controles en PS Vita

Fuente: `DefaultKeyAssignment` del juego (248 asignaciones de teclas) y lecturas directas de teclado
(`keyboard_check(vk_control)`, `ord('P')`, cifras…). Mapa completo y editable en `vita/keymap.ini`
(copiarlo a `ux0:data/reloaded/keymap.ini` para cambiarlo sin recompilar).

| Botón | Solo | Mantener L | Mantener R | Mantener SELECT |
|---|---|---|---|---|
| Cruceta / stick | Flechas | Re Pág / Av Pág / Inicio / Fin | P / K / Y / H | F5 guardar / F6 cargar / F8 móvil / F11 opciones |
| ✕ | Z aceptar | S cambiar Pokémon | A andar | T hablar |
| ○ | X cancelar | E huir | O mochila | N usar objetos |
| □ | C | B lanzar Ball | F buscar | R exclamación |
| △ | V | D | M mapa | teclado del sistema |
| START | ENTER menú | | | ESC salir (SELECT+START) |
| L / R / SELECT solos | SHIFT (toque) | | CTRL (mantener) | ESPACIO (toque) |

Combinaciones (L+R+…): ○ = **guía de controles en pantalla** (se genera desde `keymap.ini`),
□ o SELECT = mostrar/ocultar la barra táctil, △ = CTRL+P (desbloquear todos los personajes en la
selección), ✕ = F2 reiniciar, START = F3 código, ↓ = 0 comandos. SELECT+L = F1 ayuda, SELECT+R = Q misiones.
Quitados a propósito: escala (F7), captura (F9; la Vita tiene PS+START) y música sí/no (F10; está en Opciones).

Barra de teclas en pantalla (**laterales**: 1-6 a la izquierda, 7 8 9 0 \ L a la derecha; L+R+□):
Pokémon 1-6 en combate, MO 5/6/7, 8 habilidades externas, 9 Xpod, 0 comandos, \ ver Pokémon, L áreas.
Sustituye al panel trasero, desactivado por defecto para evitar roces (se puede activar en `keymap.ini`).

Páginas del runner (guía, manual del juego con show_info, imágenes con show_image): ↑/↓ desplazar,
L/R o ←/→ página, ○/✕/START cerrar. El juego queda en pausa mientras están abiertas.

Pantalla táctil = ratón en coordenadas del juego: tocar = clic, mantener y arrastrar = arrastrar
(puzle Unown, botones de póker y tragaperras, decoración).

Teclado del sistema: se abre solo cuando el juego pide escribir (nombre del personaje, apodos;
script `GetDialogString`) o con SELECT+△; lo escrito sustituye al texto, como al teclear en un PC.
El teclado en pantalla del juego sigue funcionando con la cruceta.

Sin botón: segundo jugador local (I/U/J) y pantalla completa (F4), que no tienen sentido en Vita.
