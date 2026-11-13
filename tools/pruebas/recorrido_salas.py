#!/usr/bin/env python3
# Recorrido por salas con detección de cuelgues: tour.py <lista de salas separada por comas>
import os, sys, subprocess, time
S=os.path.dirname(os.path.abspath(__file__)); D=os.environ.get('RLD_TOUR_DIR', S)  # carpeta con game.exe, reloaded.pak, la partida, keys.txt y tour_spawns.txt
BIN=os.environ.get('RLD_BIN', os.path.join(S, '..', '..', 'runner', 'target', 'release', 'gm8emulator'))
todo=[int(x) for x in sys.argv[1].split(',')]
keys=open(D+'/keys.txt').read().strip()
env=dict(os.environ, DISPLAY=':77', RLD_GOTO_FROM='16', RLD_LAZY_TEX='1', RLD_SIMPLE_GL='1', RLD_SEED='7', RLD_MANUAL='1', RLD_HANG_TRACE='1', RLD_KEYS=keys, RLD_GOTO_DWELL=os.environ.get('DWELL','240'))
def done_ids():
    try: return {int(l.split('\t')[0]) for l in open(D+'/tour.tsv') if l[0].isdigit()}
    except FileNotFoundError: return set()
while True:
    rest=[r for r in todo if r not in done_ids()]
    if not rest: break
    env['RLD_GOTO_LIST']=','.join(map(str,rest))
    for fn in ('tour_cur.txt','tour_hang.txt'):
        try: os.remove(D+'/'+fn)
        except FileNotFoundError: pass
    try: nlines0=len(open(D+'/tour.tsv').read().splitlines())
    except FileNotFoundError: nlines0=0
    p=subprocess.Popen([BIN,'game.exe','-B','10000000'],cwd=D,env=env,stdout=subprocess.DEVNULL,stderr=open(D+'/tour_err.txt','a'))
    last=time.time(); lastcur=None
    while p.poll() is None:
        time.sleep(2)
        cur=open(D+'/tour_cur.txt').read() if os.path.exists(D+'/tour_cur.txt') else None
        if cur!=lastcur: lastcur=cur; last=time.time()
        # una sala con errores del juego o que acaba en otra deja el estado tocado: empezar de nuevo
        try: tl=open(D+'/tour.tsv').read().splitlines()
        except FileNotFoundError: tl=[]
        if len(tl)>nlines0:
            q=tl[-1].split('\t')
            if len(q)>=8 and (q[7] not in ('0','?') or q[2]!=q[0]):
                p.kill(); p.wait(); break
        if time.time()-last > (120 if cur is None else 45):
            p.kill(); p.wait()
            if cur is not None:
                h=open(D+'/tour_hang.txt').read() if os.path.exists(D+'/tour_hang.txt') else ''
                with open(D+'/tour.tsv','a') as f: f.write(f"{cur}\t?\t?\t?\t?\t?\t?\t?\tCUELGUE {h}\n")
            else:
                print('no llega a la sala de inicio', flush=True); sys.exit(1)
            break
    print(len(done_ids()), 'salas hechas', flush=True)
