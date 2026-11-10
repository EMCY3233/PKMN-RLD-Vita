# Implementa en OpenGMK las funciones GM8 que usa Pokémon Reloaded y faltaban.
import re,sys
p=sys.argv[1]; s=open(p,encoding='utf-8').read()
def rep(name, body, selfmut=True):
    global s
    pat=re.compile(r'(    pub fn '+name+r'\(&(?:mut )?self, _?args: &\[Value\]\) -> gml::Result<Value> \{\n)(.*?)(\n    \}\n)', re.S)
    m=pat.search(s); assert m, name
    sig='    pub fn %s(&%sself, args: &[Value]) -> gml::Result<Value> {\n'%(name,'mut ' if selfmut else '')
    s=s[:m.start()]+sig+body+m.group(3)+s[m.end():]
NOOP='        let _ = args;\n        Ok(Default::default())'
rep('room_set_code','''        let (room_id, code) = expect_args!(args, [int, bytes])?;
        let compiled = self.compiler.compile(code.as_ref()).map(|x| x.into()).map_err(|e| e.message);
        if let Some(room) = self.assets.rooms.get_asset_mut(room_id) {
            room.creation_code = compiled;
        }
        Ok(Default::default())''')
rep('display_mouse_get_x','        expect_args!(args, [])?;\n        Ok(self.input.mouse_x().into())',False)
rep('display_mouse_get_y','        expect_args!(args, [])?;\n        Ok(self.input.mouse_y().into())',False)
for n in ['display_mouse_set','window_mouse_set','show_image','show_info','load_info','window_set_rectangle',
          'message_text_font','message_button_font','message_button','message_background','message_alpha','clipboard_set_text']:
    rep(n,NOOP)
rep('get_open_filename','        let _ = args;\n        Ok("".into())')
rep('sprite_add_sprite','        let _ = args;\n        Ok((-1).into())')
rep('sprite_duplicate','''        let src_id = expect_args!(args, [int])?;
        let new_id = self.assets.sprites.len() as i32;
        self.assets.sprites.push(None);
        self.sprite_assign(&[new_id.into(), src_id.into()])?;
        Ok(new_id.into())''')
rep('sound_fade','''        let (sound_id, volume, _time) = expect_args!(args, [int, real, real])?;
        self.sound_volume(&[sound_id.into(), volume.into()])''')
open(p,'w',encoding='utf-8').write(s); print("kernel parcheado")
