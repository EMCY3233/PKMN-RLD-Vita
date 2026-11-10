# keyboard_key_press/release: cola de teclas simuladas aplicada en el siguiente sondeo de eventos.
import sys
root=sys.argv[1]
g=open(root+'/game.rs').read()
g=g.replace("    pub close_requested: bool,\n","    pub close_requested: bool,\n    pub pending_keys: Vec<(u8, bool)>,\n",1)
g=g.replace("            close_requested: false,\n","            close_requested: false,\n            pending_keys: Vec::new(),\n",1)
g=g.replace("""        self.window.poll_events();
        match self.play_type {
            PlayType::Normal => {""","""        self.window.poll_events();
        for (vk, down) in std::mem::take(&mut self.pending_keys) {
            if down { self.input.button_press(vk, true) } else { self.input.button_release(vk, true) }
        }
        match self.play_type {
            PlayType::Normal => {""",1)
open(root+'/game.rs','w').write(g)
k=open(root+'/gml/kernel.rs').read()
for fn_,down in (("keyboard_key_press","true"),("keyboard_key_release","false")):
    i=k.index("    pub fn %s(&mut self, _args"%fn_); j=k.index("todo!() // should go on next event poll\n    }",i)+len("todo!() // should go on next event poll\n    }")
    k=k[:i]+"""    pub fn %s(&mut self, args: &[Value]) -> gml::Result<Value> {
        let key = expect_args!(args, [int])?;
        if let Ok(vk) = u8::try_from(key) {
            self.pending_keys.push((vk, %s));
        }
        Ok(Default::default())
    }"""%(fn_,down)+k[j:]
open(root+'/gml/kernel.rs','w').write(k); print("input parcheado")
