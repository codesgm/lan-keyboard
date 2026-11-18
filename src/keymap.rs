//! Tradução de teclas: evdev keycode (Linux/libei) <-> USB HID usage <-> scancode set 1 do Windows.
//! Scancode `0xE0xx` = tecla estendida. `PAUSE_SC` é tratado à parte (Windows usa VK_PAUSE).

pub const PAUSE_SC: u16 = 0xE11D;

/// (evdev, hid, windows scancode)
const TABLE: &[(u16, u16, u16)] = &[
    (1, 0x29, 0x01),   // Esc
    (2, 0x1E, 0x02),   // 1
    (3, 0x1F, 0x03),
    (4, 0x20, 0x04),
    (5, 0x21, 0x05),
    (6, 0x22, 0x06),
    (7, 0x23, 0x07),
    (8, 0x24, 0x08),
    (9, 0x25, 0x09),
    (10, 0x26, 0x0A),  // 9
    (11, 0x27, 0x0B),  // 0
    (12, 0x2D, 0x0C),  // -
    (13, 0x2E, 0x0D),  // =
    (14, 0x2A, 0x0E),  // Backspace
    (15, 0x2B, 0x0F),  // Tab
    (16, 0x14, 0x10),  // Q
    (17, 0x1A, 0x11),  // W
    (18, 0x08, 0x12),  // E
    (19, 0x15, 0x13),  // R
    (20, 0x17, 0x14),  // T
    (21, 0x1C, 0x15),  // Y
    (22, 0x18, 0x16),  // U
    (23, 0x0C, 0x17),  // I
    (24, 0x12, 0x18),  // O
    (25, 0x13, 0x19),  // P
    (26, 0x2F, 0x1A),  // [  (ABNT2: ´)
    (27, 0x30, 0x1B),  // ]  (ABNT2: [)
    (28, 0x28, 0x1C),  // Enter
    (29, 0xE0, 0x1D),  // LCtrl
    (30, 0x04, 0x1E),  // A
    (31, 0x16, 0x1F),  // S
    (32, 0x07, 0x20),  // D
    (33, 0x09, 0x21),  // F
    (34, 0x0A, 0x22),  // G
    (35, 0x0B, 0x23),  // H
    (36, 0x0D, 0x24),  // J
    (37, 0x0E, 0x25),  // K
    (38, 0x0F, 0x26),  // L
    (39, 0x33, 0x27),  // ;  (ABNT2: Ç)
    (40, 0x34, 0x28),  // '  (ABNT2: ~)
    (41, 0x35, 0x29),  // `  (ABNT2: ')
    (42, 0xE1, 0x2A),  // LShift
    (43, 0x31, 0x2B),  // \  (ABNT2: ])
    (44, 0x1D, 0x2C),  // Z
    (45, 0x1B, 0x2D),  // X
    (46, 0x06, 0x2E),  // C
    (47, 0x19, 0x2F),  // V
    (48, 0x05, 0x30),  // B
    (49, 0x11, 0x31),  // N
    (50, 0x10, 0x32),  // M
    (51, 0x36, 0x33),  // ,
    (52, 0x37, 0x34),  // .
    (53, 0x38, 0x35),  // /  (ABNT2: ;)
    (54, 0xE5, 0x36),  // RShift
    (55, 0x55, 0x37),  // KP *
    (56, 0xE2, 0x38),  // LAlt
    (57, 0x2C, 0x39),  // Space
    (58, 0x39, 0x3A),  // CapsLock
    (59, 0x3A, 0x3B),  // F1
    (60, 0x3B, 0x3C),
    (61, 0x3C, 0x3D),
    (62, 0x3D, 0x3E),
    (63, 0x3E, 0x3F),
    (64, 0x3F, 0x40),
    (65, 0x40, 0x41),
    (66, 0x41, 0x42),
    (67, 0x42, 0x43),
    (68, 0x43, 0x44),  // F10
    (69, 0x53, 0xE045), // NumLock (SendInput exige flag estendida)
    (70, 0x47, 0x46),  // ScrollLock
    (71, 0x5F, 0x47),  // KP7
    (72, 0x60, 0x48),  // KP8
    (73, 0x61, 0x49),  // KP9
    (74, 0x56, 0x4A),  // KP -
    (75, 0x5C, 0x4B),  // KP4
    (76, 0x5D, 0x4C),  // KP5
    (77, 0x5E, 0x4D),  // KP6
    (78, 0x57, 0x4E),  // KP +
    (79, 0x59, 0x4F),  // KP1
    (80, 0x5A, 0x50),  // KP2
    (81, 0x5B, 0x51),  // KP3
    (82, 0x62, 0x52),  // KP0
    (83, 0x63, 0x53),  // KP .
    (86, 0x64, 0x56),  // 102nd (ABNT2: \)
    (87, 0x44, 0x57),  // F11
    (88, 0x45, 0x58),  // F12
    (89, 0x87, 0x73),  // RO (ABNT2: /?)
    (92, 0x8A, 0x79),  // Henkan
    (93, 0x88, 0x70),  // Katakana/Hiragana
    (94, 0x8B, 0x7B),  // Muhenkan
    (96, 0x58, 0xE01C), // KP Enter
    (97, 0xE4, 0xE01D), // RCtrl
    (98, 0x54, 0xE035), // KP /
    (99, 0x46, 0xE037), // PrintScreen
    (100, 0xE6, 0xE038), // RAlt / AltGr
    (102, 0x4A, 0xE047), // Home
    (103, 0x52, 0xE048), // Up
    (104, 0x4B, 0xE049), // PageUp
    (105, 0x50, 0xE04B), // Left
    (106, 0x4F, 0xE04D), // Right
    (107, 0x4D, 0xE04F), // End
    (108, 0x51, 0xE050), // Down
    (109, 0x4E, 0xE051), // PageDown
    (110, 0x49, 0xE052), // Insert
    (111, 0x4C, 0xE053), // Delete
    (113, 0xEF, 0xE020), // Mute
    (114, 0xEE, 0xE02E), // VolumeDown
    (115, 0xED, 0xE030), // VolumeUp
    (117, 0x67, 0x59),   // KP =
    (119, 0x48, PAUSE_SC), // Pause
    (121, 0x85, 0x7E),   // KP , (ABNT2: KP .)
    (124, 0x89, 0x7D),   // Yen
    (125, 0xE3, 0xE05B), // LMeta / Win
    (126, 0xE7, 0xE05C), // RMeta
    (127, 0x65, 0xE05D), // Menu
    (163, 0xEB, 0xE019), // NextSong
    (164, 0xE8, 0xE022), // PlayPause
    (165, 0xEA, 0xE010), // PreviousSong
    (166, 0xE9, 0xE024), // StopCD
    (183, 0x68, 0x64),   // F13
    (184, 0x69, 0x65),
    (185, 0x6A, 0x66),
    (186, 0x6B, 0x67),
    (187, 0x6C, 0x68),
    (188, 0x6D, 0x69),
    (189, 0x6E, 0x6A),
    (190, 0x6F, 0x6B),
    (191, 0x70, 0x6C),
    (192, 0x71, 0x6D),
    (193, 0x72, 0x6E),
    (194, 0x73, 0x76),   // F24
];

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn evdev_to_hid(code: u32) -> Option<u16> {
    TABLE.iter().find(|e| e.0 as u32 == code).map(|e| e.1)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn hid_to_evdev(hid: u16) -> Option<u16> {
    TABLE.iter().find(|e| e.1 == hid).map(|e| e.0)
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn hid_to_win_scancode(hid: u16) -> Option<u16> {
    TABLE.iter().find(|e| e.1 == hid).map(|e| e.2)
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn win_scancode_to_hid(sc: u16) -> Option<u16> {
    TABLE.iter().find(|e| e.2 == sc).map(|e| e.1)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn all_evdev() -> impl Iterator<Item = u16> {
    TABLE.iter().map(|e| e.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_entries() {
        for (i, a) in TABLE.iter().enumerate() {
            for b in &TABLE[i + 1..] {
                assert_ne!(a.0, b.0, "evdev duplicado {}", a.0);
                assert_ne!(a.1, b.1, "hid duplicado {:#x}", a.1);
                assert_ne!(a.2, b.2, "scancode duplicado {:#x}", a.2);
            }
        }
    }

    #[test]
    fn roundtrip() {
        for &(ev, hid, _) in TABLE {
            assert_eq!(evdev_to_hid(ev as u32), Some(hid));
            assert_eq!(hid_to_evdev(hid), Some(ev));
        }
        assert_eq!(hid_to_win_scancode(0x04), Some(0x1E)); // A
        assert_eq!(hid_to_win_scancode(0xE6), Some(0xE038)); // AltGr
        for &(_, hid, sc) in TABLE {
            assert_eq!(win_scancode_to_hid(sc), Some(hid));
        }
    }
}
