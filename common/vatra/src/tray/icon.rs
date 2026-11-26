// StatusNotifierItem transmits network-byte-order ARGB32 pixels.
const SIZE: usize = 24;
const DIGITS: [[u8; 5]; 11] = [
    [7, 5, 5, 5, 7], [2, 6, 2, 2, 7], [7, 1, 7, 4, 7], [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1], [7, 4, 7, 1, 7], [7, 4, 7, 5, 7], [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7], [7, 5, 7, 1, 7], [5, 1, 2, 4, 5],
];

pub fn draw(level: Option<u8>, charging: bool, accent: [u8; 3], percent: u8, bolt: u8) -> Vec<u8> {
    let mut pixels = vec![0; SIZE * SIZE * 4];
    let theme = [255, accent[0], accent[1], accent[2]];
    let bg = [255, 29, 38, 49];
    let white = [255; 4];
    let label = level.filter(|_| percent != 0).map(|value| format!("{}%", value.min(100)));
    let beside = label.is_some() && percent == 2;
    let mut rect = |x: usize, y: usize, w: usize, h: usize, color: [u8; 4]| {
        for row in y..(y + h).min(SIZE) {
            for col in x..(x + w).min(SIZE) {
                let offset = (row * SIZE + col) * 4;
                pixels[offset..offset + 4].copy_from_slice(&color);
            }
        }
    };
    let charge = usize::from(level.unwrap_or(0).min(100));
    if beside {
        rect(2, 3, 4, 2, theme);
        rect(0, 5, 8, 15, theme);
        rect(1, 6, 6, 13, bg);
        let fill = (charge * 13 + 50) / 100;
        rect(1, 19 - fill, 6, fill, theme);
    } else {
        rect(1, 5, 20, 14, theme);
        rect(3, 7, 16, 10, bg);
        rect(22, 9, 2, 6, theme);
        rect(4, 8, (charge * 14 + 50) / 100, 8, theme);
    }
    if let Some(text) = &label {
        let width = text.len() * 4 - 1;
        let left = if beside { 9 + (15 - width) / 2 } else { (22 - width) / 2 };
        if !beside { rect(left - 1, 7, width + 2, 10, bg); }
        for (col, byte) in text.bytes().enumerate() {
            let index = if byte == b'%' { 10 } else { usize::from(byte - b'0') };
            for (row, bits) in DIGITS[index].into_iter().enumerate() {
                for px in 0..3 {
                    if bits & (4 >> px) != 0 {
                        rect(left + col * 4 + px, 7 + row * 2, 1, 2, white);
                    }
                }
            }
        }
    }
    if charging && bolt != 2 {
        let color = if bolt == 1 { white } else { theme };
        let (x, y) = if beside { (0, 7) } else if label.is_some() { (16, 1) } else { (8, 7) };
        let shape = [(4, 0, 1), (3, 1, 2), (2, 2, 2), (1, 3, 5), (3, 4, 2), (2, 5, 2), (2, 6, 1)];
        for (dx, dy, w) in shape { rect(x + dx - 1, y + dy - 1, w + 2, 3, bg); }
        for (dx, dy, w) in shape { rect(x + dx, y + dy, w, 1, color); }
    }
    pixels
}
