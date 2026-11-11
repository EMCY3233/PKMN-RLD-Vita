/// Converts BGRA pixeldata to RGBA pixeldata in-place.
pub fn bgra2rgba(data: &mut [u8]) {
    assert_eq!(data.len() % 4, 0);
    data.chunks_exact_mut(4).for_each(|chunk| chunk.swap(0, 2));
}

/// Converts RGBA pixeldata to BGRA pixeldata in-place.
pub use bgra2rgba as rgba2bgra;

// Helper fn: rotate mutable x and y around a center point, given sin and cos of the angle to rotate by
pub fn rotate_around(x: &mut f64, y: &mut f64, center_x: f64, center_y: f64, sin: f64, cos: f64) {
    *x -= center_x;
    *y -= center_y;
    rotate_around_center(x, y, sin, cos);
    *x += center_x;
    *y += center_y;
}

pub fn rotate_around_center(x: &mut f64, y: &mut f64, sin: f64, cos: f64) {
    let x_new = (*x * cos) - (*y * sin);
    let y_new = (*x * sin) + (*y * cos);
    *x = x_new;
    *y = y_new;
}

#[cfg(test)]
mod tests {
    use super::{bgra2rgba, rgba2bgra};

    #[test]
    fn bgra_rgba() {
        let bgra_pixels = [0u8, 1, 54, 242, 192, 24, 65, 6, 35, 98, 4, 20];
        let rgba_pixels = [54u8, 1, 0, 242, 65, 24, 192, 6, 4, 98, 35, 20];
        let mut cool_pixels = bgra_pixels.to_vec();

        bgra2rgba(&mut cool_pixels);
        assert_eq!(cool_pixels, rgba_pixels);
        rgba2bgra(&mut cool_pixels);
        assert_eq!(cool_pixels, bgra_pixels);
    }
}

/// Hash rápido (algoritmo FxHash, el de rustc) para los mapas que el intérprete consulta sin parar:
/// variables de instancia, alarmas, arrays y eventos de objeto. El SipHash por defecto de Rust es
/// resistente a ataques pero lento, y aquí las claves son números internos del juego.
/// En la Zona Safari (1235 instancias) SipHash era ~17 % del tiempo de CPU.
#[derive(Default, Clone, Copy)]
pub struct FastHasher(u64);

impl std::hash::Hasher for FastHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut w = [0u8; 8];
            w[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(w));
        }
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.write_u64(i as u64)
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64)
    }
    #[inline]
    fn write_i32(&mut self, i: i32) {
        self.write_u64(i as u32 as u64)
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.write_u64(i as u64)
    }
    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.write_u64(i as u64)
    }
}

pub type FastHashMap<K, V> = std::collections::HashMap<K, V, std::hash::BuildHasherDefault<FastHasher>>;
pub type FastIndexMap<K, V> = indexmap::IndexMap<K, V, std::hash::BuildHasherDefault<FastHasher>>;
pub type FastHashSet<K> = std::collections::HashSet<K, std::hash::BuildHasherDefault<FastHasher>>;
