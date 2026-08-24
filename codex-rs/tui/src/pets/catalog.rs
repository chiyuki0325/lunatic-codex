//! Built-in pet catalog ported from the Codex App avatar catalog.

#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::io::Cursor;
#[cfg(test)]
use std::sync::LazyLock;

pub(super) const DEFAULT_FRAME_WIDTH: u32 = 192;
pub(super) const DEFAULT_FRAME_HEIGHT: u32 = 208;
pub(super) const DEFAULT_FRAME_COLUMNS: u32 = 8;
pub(super) const DEFAULT_FRAME_ROWS: u32 = 9;
pub(super) const SPRITESHEET_WIDTH: u32 = DEFAULT_FRAME_WIDTH * DEFAULT_FRAME_COLUMNS;
pub(super) const SPRITESHEET_HEIGHT: u32 = DEFAULT_FRAME_HEIGHT * DEFAULT_FRAME_ROWS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BuiltinPet {
    pub(super) id: &'static str,
    pub(super) display_name: &'static str,
    pub(super) description: &'static str,
    pub(super) spritesheet_file: &'static str,
}

pub(super) const BUILTIN_PETS: &[BuiltinPet] = &[
    BuiltinPet {
        id: "codex",
        display_name: "Codex",
        description: "最初的 Codex 伙伴",
        spritesheet_file: "codex-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "dewey",
        display_name: "Dewey",
        description: "为平静工作日准备的整洁小鸭",
        spritesheet_file: "dewey-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "fireball",
        display_name: "Fireball",
        description: "为快速迭代注入热点能量",
        spritesheet_file: "fireball-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "rocky",
        display_name: "Rocky",
        description: "差异变大时的沉稳磐石",
        spritesheet_file: "rocky-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "seedy",
        display_name: "Seedy",
        description: "为新想法萌发的绿色嫩芽",
        spritesheet_file: "seedy-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "stacky",
        display_name: "Stacky",
        description: "适合深度工作的平衡堆栈",
        spritesheet_file: "stacky-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "bsod",
        display_name: "BSOD",
        description: "小小的蓝屏捣蛋鬼",
        spritesheet_file: "bsod-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "null-signal",
        display_name: "Null Signal",
        description: "来自虚空的安静信号",
        spritesheet_file: "null-signal-spritesheet-v4.webp",
    },
];

pub(super) fn builtin_pet(id: &str) -> Option<BuiltinPet> {
    BUILTIN_PETS.iter().copied().find(|pet| pet.id == id)
}

#[cfg(test)]
pub(super) fn write_test_spritesheet(path: &std::path::Path) {
    static TEST_SPRITESHEET: LazyLock<Vec<u8>> = LazyLock::new(|| {
        let image = image::RgbaImage::new(SPRITESHEET_WIDTH, SPRITESHEET_HEIGHT);
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::WebP)
            .unwrap();
        bytes
    });
    fs::write(path, TEST_SPRITESHEET.as_slice()).unwrap();
}
