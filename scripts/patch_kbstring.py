# keyboard_string / keyboard_lastchar (pantalla de nombre, buscador de Pokédex...)
import sys
r=sys.argv[1]
g=open(r+'/game.rs').read()
g=g.replace("    pub pending_keys: Vec<(u8, bool)>,\n","    pub pending_keys: Vec<(u8, bool)>,\n    pub keyboard_string: gml::String,\n    pub keyboard_lastchar: gml::String,\n",1)
g=g.replace("            pending_keys: Vec::new(),\n","            pending_keys: Vec::new(),\n            keyboard_string: \"\".into(),\n            keyboard_lastchar: \"\".into(),\n",1)
g=g.replace("Event::KeyboardDown(key) => self.input.button_press(input::ramen2vk(key), true),",
 "Event::KeyboardDown(key) => { let vk = input::ramen2vk(key); self.input.button_press(vk, true); self.type_vk(vk); },",1)
g=g.replace("    pub fn process_window_events(&mut self) {","""    /// Traduce una tecla a texto para keyboard_string (en Vita lo rellena el teclado del sistema).
    pub fn type_vk(&mut self, vk: u8) {
        let shift = self.input.keyboard_check_direct(16);
        let ch: Option<char> = match vk {
            b'A'..=b'Z' => Some(if shift { vk as char } else { (vk as char).to_ascii_lowercase() }),
            b'0'..=b'9' | b' ' => Some(vk as char),
            _ => None,
        };
        let mut s = String::from_utf8_lossy(self.keyboard_string.as_ref()).into_owned();
        if vk == 8 { s.pop(); }
        else if let Some(c) = ch { if s.len() < 1024 { s.push(c); } self.keyboard_lastchar = c.to_string().as_str().into(); }
        else { return; }
        self.keyboard_string = s.as_str().into();
    }

    pub fn process_window_events(&mut self) {""",1)
open(r+'/game.rs','w').write(g)
t=open(r+'/gml/runtime.rs').read()
t=t.replace('InstanceVariable::KeyboardLastchar => todo!("keyboard_lastchar getter"),','InstanceVariable::KeyboardLastchar => Ok(self.keyboard_lastchar.clone().into()),')
t=t.replace('InstanceVariable::KeyboardString => todo!("keyboard_string getter"),','InstanceVariable::KeyboardString => Ok(self.keyboard_string.clone().into()),')
t=t.replace('InstanceVariable::KeyboardLastchar => todo!("keyboard_lastchar setter"),','InstanceVariable::KeyboardLastchar => self.keyboard_lastchar = value.into(),')
t=t.replace('InstanceVariable::KeyboardString => todo!("keyboard_string setter"),','InstanceVariable::KeyboardString => self.keyboard_string = value.into(),')
open(r+'/gml/runtime.rs','w').write(t); print("keyboard_string parcheado")
