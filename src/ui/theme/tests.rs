use super::*;

#[test]
fn presets_have_distinct_surfaces_and_modes() {
    let countertop = resolve_palette(ThemePreset::Countertop, RoleColorOverrides::default());
    let blue = resolve_palette(ThemePreset::BlueSteel, RoleColorOverrides::default());
    let paper = resolve_palette(ThemePreset::PaperBag, RoleColorOverrides::default());
    assert_ne!(countertop.base, blue.base);
    assert!(countertop.base.r < 0.2);
    assert!(paper.base.r > 0.8);
}

#[test]
fn semantic_overrides_replace_only_the_requested_role() {
    let baseline = resolve_palette(ThemePreset::Countertop, RoleColorOverrides::default());
    let custom = HexColor::from_rgb(0x12, 0x34, 0x56);
    let palette = resolve_palette(
        ThemePreset::Countertop,
        RoleColorOverrides {
            danger: Some(custom),
            ..RoleColorOverrides::default()
        },
    );
    assert_eq!(palette.danger, color_from_hex(custom));
    assert_eq!(palette.primary, baseline.primary);
}

#[test]
fn contrasting_text_changes_for_light_and_dark_colors() {
    assert_eq!(
        contrasting_text(Color::WHITE),
        Color::from_rgb8(0x18, 0x16, 0x14)
    );
    assert_eq!(
        contrasting_text(Color::BLACK),
        Color::from_rgb8(0xFA, 0xF7, 0xF2)
    );
}

#[test]
fn scrollbar_interaction_is_axis_specific() {
    assert_eq!(
        scrollbar_state(
            true,
            scrollable::Status::Active {
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            },
        ),
        ((false, false), (false, false))
    );
    assert_eq!(
        scrollbar_state(
            true,
            scrollable::Status::Hovered {
                is_horizontal_scrollbar_hovered: false,
                is_vertical_scrollbar_hovered: false,
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            },
        ),
        ((true, false), (true, false))
    );
    assert_eq!(
        scrollbar_state(
            false,
            scrollable::Status::Hovered {
                is_horizontal_scrollbar_hovered: false,
                is_vertical_scrollbar_hovered: true,
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            },
        ),
        ((false, false), (true, true))
    );
    assert_eq!(
        scrollbar_state(
            false,
            scrollable::Status::Dragged {
                is_horizontal_scrollbar_dragged: true,
                is_vertical_scrollbar_dragged: false,
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            },
        ),
        ((true, true), (false, false))
    );
    assert_eq!(
        scrollbar_interaction(scrollable::Status::Active {
            is_horizontal_scrollbar_disabled: false,
            is_vertical_scrollbar_disabled: false,
        }),
        (false, false)
    );
    assert_eq!(
        scrollbar_interaction(scrollable::Status::Hovered {
            is_horizontal_scrollbar_hovered: false,
            is_vertical_scrollbar_hovered: true,
            is_horizontal_scrollbar_disabled: false,
            is_vertical_scrollbar_disabled: false,
        }),
        (false, true)
    );
    assert_eq!(
        scrollbar_interaction(scrollable::Status::Dragged {
            is_horizontal_scrollbar_dragged: true,
            is_vertical_scrollbar_dragged: false,
            is_horizontal_scrollbar_disabled: false,
            is_vertical_scrollbar_disabled: false,
        }),
        (true, false)
    );
}
