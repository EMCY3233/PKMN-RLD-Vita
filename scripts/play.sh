#!/usr/bin/env bash
# Prueba automática en Linux: arranca el juego en Xvfb, pulsa teclas y saca capturas.
# Uso: play.sh "z:6 z:2 ... shot:nombre ..." [segundos_arranque]
export DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1
pgrep Xvfb >/dev/null || (Xvfb :99 -screen 0 1280x800x24 >/dev/null 2>&1 &); sleep 1
pkill gm8emulator; sleep 1; cd ~/game
(timeout 600 ~/OpenGMK/target/release/gm8emulator "$(ls *.exe|head -1)" > ~/run.log 2>&1 &)
sleep ${2:-30}
W=$(xdotool search --class . 2>/dev/null | tail -1); [ -n "$W" ] && xdotool windowactivate --sync $W 2>/dev/null; xdotool windowfocus $W 2>/dev/null
for step in $1; do k=${step%%:*}; v=${step#*:}
  if [ "$k" = shot ]; then import -window root ~/shots/$v.png; elif [ "$k" = wait ]; then sleep $v;
  else xdotool keydown $k; sleep 0.12; xdotool keyup $k; sleep ${v:-1}; fi; done
pgrep gm8emulator >/dev/null && echo VIVO || { echo MUERTO; tail -5 ~/run.log; }
