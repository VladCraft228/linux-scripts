//! XKB-aware symbol lookup for ordinary focused settings-window key events.
use crate::Result;
use x11rb::{connection::Connection, protocol::xkb};

pub fn initialize<C: Connection>(connection: &C) -> Result<()> {
    // Without negotiation, the server reports compatibility state with zero
    // group bits to this client, even when the user's active group is not zero.
    if !xkb::use_extension(connection, 1, 0)?.reply()?.supported {
        return Err("XKB 1.0 is required for settings keyboard input".into());
    }
    Ok(())
}

pub struct Symbols {
    pub plain: u32,
    pub shifted: u32,
    pub shortcut: u32,
}

pub fn symbols<C: Connection>(connection: &C, key: u8, state: u16) -> Result<Symbols> {
    // Request only this key. Read the actual per-key group width rather than
    // assuming two levels or flattening the core compatibility map.
    let reply = xkb::get_map(
        connection, u16::from(xkb::ID::USE_CORE_KBD),
        xkb::MapPart::default(), xkb::MapPart::KEY_SYMS,
        0, 0, key, 1, 0, 0, 0, 0, xkb::VMod::default(), 0, 0, 0, 0, 0, 0,
    )?.reply()?;
    let map = reply.map.syms_rtrn.as_ref().and_then(|maps| maps.first());
    Ok(map.map_or(Symbols { plain: 0, shifted: 0, shortcut: 0 }, |map| {
        from_map(map, ((state >> 13) & 3) as usize)
    }))
}

fn from_map(map: &xkb::KeySymMap, requested: usize) -> Symbols {
    let count = usize::from(map.group_info & 0x0f);
    let width = usize::from(map.width);
    let shortcut = map.syms.first().copied().unwrap_or(0);
    if count == 0 || width == 0 {
        return Symbols { plain: 0, shifted: 0, shortcut };
    }
    let group = if requested < count {
        requested
    } else {
        match map.group_info & 0xc0 {
            0x40 => count - 1, // clamp
            0x80 => { // redirect, with group zero for an invalid redirect target
                let target = usize::from((map.group_info >> 4) & 3);
                if target < count { target } else { 0 }
            },
            _ => requested % count, // wrap
        }
    };
    let base = group * width;
    let plain = map.syms.get(base).copied().unwrap_or(0);
    let shifted = if width > 1 {
        map.syms.get(base + 1).copied().filter(|symbol| *symbol != 0).unwrap_or(plain)
    } else { plain };
    Symbols { plain, shifted, shortcut }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_ukrainian_group_with_four_levels_per_group() {
        let map = xkb::KeySymMap {
            group_info: 2, width: 4,
            syms: vec![u32::from('a'), u32::from('A'), 0, 0, 0x06c6, 0x06e6, 0, 0],
            ..Default::default()
        };
        let symbols = from_map(&map, 1);
        assert_eq!(crate::applications::character(symbols.plain), Some('ф'));
        assert_eq!(crate::applications::character(symbols.shifted), Some('Ф'));
        assert_eq!(symbols.shortcut, u32::from('a'));
    }
    #[test]
    fn single_group_keys_wrap_and_missing_upper_level_falls_back() {
        let map = xkb::KeySymMap { group_info: 1, width: 2, syms: vec![0xff08, 0], ..Default::default() };
        assert_eq!(from_map(&map, 2).plain, 0xff08);
        assert_eq!(from_map(&map, 2).shifted, 0xff08);
    }
}
